//! Utilidades dos testes de resiliência: binário real, plano de N unidades, execução com/sem kill.
//!
//! **O quê:** monta um plano de unidades FALSAS (`sh -c`) que registram `inicio <id>` num log de
//! execuções e gravam um marcador atômico ao terminar; lança o `schematize-overdev` de verdade,
//! mata com SIGKILL e confere o que sobrou. **Onde:** os testes `resiliencia_*`.

#![allow(dead_code)]

use schematize_plataforma::persistencia::Rng;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// **O quê:** caminho do binário real. **Onde:** `lanca`.
pub fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_schematize-overdev"))
}

/// **O quê:** dir temporário limpo e único por teste. **Onde:** todos os testes.
pub fn tmp(nome: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("od-res-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("criar tmp");
    d
}

/// **O quê:** a semente: `PERSIST_SEED` do ambiente ou derivada do relógio; SEMPRE impressa.
/// **Onde:** testes aleatórios (reproduzir = exportar `PERSIST_SEED`).
pub fn semente(teste: &str) -> u64 {
    let s = std::env::var("PERSIST_SEED").ok().and_then(|v| v.parse().ok()).unwrap_or_else(|| {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)
    });
    eprintln!("[{teste}] PERSIST_SEED={s}");
    s
}

/// **O quê:** escreve o plano de `n` unidades em `dir/plano.json`; cada unidade loga o início em
/// `dir/exec.log`, dorme `ms_base + (i % 5) * 15` ms e grava `dir/saida/<id>` de forma atômica.
/// **Onde:** os testes de kill.
pub fn escrever_plano(dir: &Path, n: usize, ms_base: u64, paralelo: usize) -> PathBuf {
    std::fs::create_dir_all(dir.join("saida")).expect("saida");
    let (log, saida) = (dir.join("exec.log"), dir.join("saida"));
    let itens: Vec<serde_json::Value> = (0..n)
        .map(|i| {
            let ms = ms_base + (i as u64 % 5) * 15;
            let script = format!(
                "echo inicio \"$0\" >> {log}; sleep {s}; echo ok > {out}/\"$0\".tmp; mv {out}/\"$0\".tmp {out}/\"$0\"",
                log = log.display(), s = ms as f64 / 1000.0, out = saida.display()
            );
            serde_json::json!({"id": format!("u{i:02}"), "brief": format!("brief {i}"), "tipo": "fake",
                "teto_mib": 64, "argv": ["sh", "-c", script, "{id}"]})
        })
        .collect();
    let plano = serde_json::json!({"modelo": "sonnet", "paralelo": paralelo, "margem_mib": 256,
        "prefixo": "PREFIXO-ESTAVEL", "itens": itens});
    let p = dir.join("plano.json");
    std::fs::write(&p, plano.to_string()).expect("gravar plano");
    p
}

/// **O quê:** lança `executar --plano ... --estado ...` (o `--resume` é acrescentado como o vigia
/// faz nos relançamentos). **Onde:** os testes de kill.
pub fn lanca(dir: &Path, plano: &Path, resume: bool, extra: &[&str]) -> Child {
    let mut c = Command::new(bin());
    c.arg("executar").arg("--plano").arg(plano).arg("--estado").arg(dir.join("ov/estado"));
    c.args(["--sem-jaula", "--json"]).args(extra);
    if resume {
        c.arg("--resume");
    }
    c.env("HOME", dir).env("OVERDEV_SNAPSHOT_EVENTOS", "7");
    c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("spawn overdev")
}

/// **O quê:** SIGKILL só no pid do filho (nunca em grupo) e colhe o zumbi. **Onde:** testes.
pub fn matar(c: &mut Child) {
    // SAFETY: `kill` não toca memória; o pid é de um filho nosso ainda não colhido.
    let r = unsafe { libc::kill(c.id() as i32, libc::SIGKILL) };
    assert_eq!(r, 0, "kill -9 do orquestrador falhou");
    let _ = c.wait();
}

/// **O quê:** espera o filho terminar (até `limite`) e devolve (código, stdout, stderr).
/// **Onde:** testes. Estourou o limite: mata e falha com a saída.
pub fn esperar(mut c: Child, limite: Duration) -> (Option<i32>, String, String) {
    let ini = Instant::now();
    while ini.elapsed() < limite {
        if let Ok(Some(_)) = c.try_wait() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    if matches!(c.try_wait(), Ok(None)) {
        matar(&mut c);
        panic!("o overdev não terminou em {limite:?}");
    }
    let o = c.wait_with_output().expect("saída");
    (o.status.code(), String::from_utf8_lossy(&o.stdout).into(), String::from_utf8_lossy(&o.stderr).into())
}

/// **O quê:** quantas vezes cada unidade INICIOU (linhas `inicio <id>` do log). **Onde:** testes.
pub fn inicios(dir: &Path) -> BTreeMap<String, usize> {
    let t = std::fs::read_to_string(dir.join("exec.log")).unwrap_or_default();
    let mut m = BTreeMap::new();
    for id in t.lines().filter_map(|l| l.strip_prefix("inicio ")) {
        *m.entry(id.trim().to_string()).or_insert(0) += 1;
    }
    m
}

/// **O quê:** sorteia um atraso em ms no intervalo `[min, max]`. **Onde:** momento do kill.
pub fn atraso(rng: &mut Rng, min: u64, max: u64) -> Duration {
    Duration::from_millis(min + rng.ate(max - min + 1))
}
