//! Governo de memória no executor: PSI simulado estrangula o despacho (FixtureDir) e OOM real de
//! uma unidade jaulada não derruba o orquestrador nem as outras unidades.

mod resiliencia_comum;

use overdev_casa::resiliencia::controle::{politica_para, Controle};
use overdev_casa::resiliencia::despacho::DespachanteProcesso;
use overdev_casa::resiliencia::duravel::{Duravel, PoliticaSnapshot};
use overdev_casa::resiliencia::executor::{Executor, Opcoes};
use overdev_casa::resiliencia::modelo::{Evento, ItemPlano, StatusItem};
use resiliencia_comum::*;
use schematize_plataforma::jaula::{detectar_backend, Backend};
use schematize_plataforma::memoria::{FixtureDir, FonteDir};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::Duration;

/// PSI `some avg10=<x>`. **Onde:** fixtures.
fn psi(x: f64) -> String {
    format!("some avg10={x:.2} avg60=0.00 avg300=0.00 total=0\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0\n")
}

/// **O quê:** item cujo comando loga `inicio <id>` em `log`. **Onde:** teste (e).
fn item(id: &str, log: &Path) -> ItemPlano {
    let script = format!("echo inicio \"$0\" >> {}", log.display());
    ItemPlano {
        id: id.into(),
        brief: format!("b-{id}"),
        tipo: "fake".into(),
        teto_mib: Some(64),
        marca: None,
        argv: vec!["sh".into(), "-c".into(), script, "{id}".into()],
    }
}

/// (e) Sob PSI alto o governador estrangula e NADA é despachado; com PSI baixo libera e conclui.
#[test]
fn governador_estrangula_sob_psi_simulado() {
    let dir = tmp("psi");
    let fx = FixtureDir::nova("exec-psi").expect("fixture");
    fx.escrever("proc/meminfo", "MemTotal: 16384000 kB\nMemAvailable: 12288000 kB\n").unwrap();
    fx.escrever("proc/pressure/memory", &psi(45.0)).unwrap();
    let (raiz, d2) = (fx.raiz().to_path_buf(), dir.clone());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let log = d2.join("exec.log");
        let mut d =
            Duravel::abrir(&d2.join("ov/estado"), PoliticaSnapshot::default()).expect("abrir");
        let itens = ["a", "b", "c"].iter().map(|i| item(i, &log)).collect();
        d.registrar(&Evento::Plano { prefixo: String::new(), modelo_base: "sonnet".into(), itens })
            .unwrap();
        let ctl = Controle::new_fixture(&raiz);
        let r =
            Executor::novo(d, ctl, Box::new(DespachanteProcesso), Opcoes::default(), d2.join("ov"))
                .rodar();
        tx.send(r.map(|r| r.entregues).map_err(|e| e.to_string())).ok();
    });
    std::thread::sleep(Duration::from_millis(700));
    assert!(inicios(&dir).is_empty(), "despachou sob PSI alto: {:?}", inicios(&dir));
    fx.escrever("proc/pressure/memory", &psi(0.1)).unwrap();
    let r = rx.recv_timeout(Duration::from_secs(30)).expect("não terminou").expect("executor");
    assert_eq!(r, 3);
    assert_eq!(inicios(&dir).len(), 3);
    let d = Duravel::abrir(&dir.join("ov/estado"), PoliticaSnapshot::default()).expect("reabrir");
    let obs = d.estado().observacoes.join("\n");
    assert!(
        obs.contains("estrangular") && obs.contains("liberar"),
        "journal sem as observações:\n{obs}"
    );
    let jl = std::fs::read_to_string(dir.join("ov/jaulas.jsonl")).unwrap_or_default();
    assert!(jl.contains("estrangular"), "jaulas.jsonl sem o evento do governador");
}

/// Atalho de teste para montar o `Controle` sobre a fixture. **Onde:** teste (e).
trait ControleFixture {
    /// **O quê:** controle de margem 1 GiB e paralelo 3 lendo `raiz`. **Onde:** teste (e).
    fn new_fixture(raiz: &Path) -> Controle;
}

impl ControleFixture for Controle {
    /// **O quê:** vide trait. **Onde:** teste (e).
    fn new_fixture(raiz: &Path) -> Controle {
        Controle::novo(politica_para(3, 1024), Arc::new(FonteDir::com_raiz(raiz)), None, None)
    }
}

/// **O quê:** escreve um plano com uma unidade que estoura a memória e outra que conclui.
/// **Onde:** teste (d).
fn plano_oom(dir: &Path) -> PathBuf {
    let aloca = "b = b'x' * (800 * 1024 * 1024)\nimport time\ntime.sleep(30)";
    let plano = serde_json::json!({"modelo":"sonnet","paralelo":2,"margem_mib":512,"itens":[
        {"id":"oom","brief":"x","tipo":"oom","teto_mib":500,"argv":["python3","-c",aloca]},
        {"id":"ok","brief":"y","tipo":"ok","teto_mib":500,"argv":["sh","-c","exit 0"]}]});
    let p = dir.join("plano.json");
    std::fs::write(&p, plano.to_string()).expect("plano");
    p
}

/// (d) OOM real de UMA unidade jaulada: a vizinha conclui, a estourada é re-decomposta e por fim
/// parkeada, e o orquestrador segue vivo e sai com sucesso.
#[test]
fn oom_de_uma_unidade_com_orquestrador_vivo() {
    if detectar_backend() != Backend::SystemdScope {
        eprintln!("PULADO: sem systemd-run --user + cgroup memory delegado");
        return;
    }
    if std::process::Command::new("python3").arg("--version").output().is_err() {
        eprintln!("PULADO: python3 ausente");
        return;
    }
    let dir = tmp("oom");
    let plano = plano_oom(&dir);
    let c = std::process::Command::new(bin())
        .args(["executar", "--plano"])
        .arg(&plano)
        .arg("--estado")
        .arg(dir.join("ov/estado"))
        .arg("--json")
        .env("HOME", &dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    let (code, out, err) = esperar(c, Duration::from_secs(180));
    assert_eq!(code, Some(0), "o orquestrador deveria sobreviver e concluir\n{out}\n{err}");
    let d = Duravel::abrir(&dir.join("ov/estado"), PoliticaSnapshot::default()).expect("reabrir");
    let e = d.estado();
    assert_eq!(e.itens["ok"].status, StatusItem::Entregue);
    assert_eq!(e.itens["oom"].status, StatusItem::Parkeado);
    let motivo = e.itens["oom"].motivo_park.clone().unwrap_or_default();
    assert!(
        motivo.contains("EstourouMemoria"),
        "motivo: {motivo} / {:?}",
        e.itens["oom"].historico
    );
    let jl = std::fs::read_to_string(dir.join("ov/jaulas.jsonl")).expect("jaulas.jsonl");
    assert!(jl.contains("\"veredito\":\"oom\""), "sem veredito oom no jaulas.jsonl:\n{jl}");
    assert!(e.itens["oom"].geracao >= 1, "deveria ter re-decomposto antes de parkear");
}
