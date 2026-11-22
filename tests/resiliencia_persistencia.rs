//! Persistência sob dano: SIGKILL durante a gravação do journal e do snapshot, e estado corrompido
//! (bit-flip, truncado, esquema alheio) — cai no `.prev`/replay, reporta, NUNCA panic.
//!
//! Reproduzir: `PERSIST_SEED=<seed impressa> cargo test --test resiliencia_persistencia -- --nocapture`.

mod resiliencia_comum;

use overdev_casa::resiliencia::duravel::{Duravel, PoliticaSnapshot};
use overdev_casa::resiliencia::modelo::{EstadoOverdev, Evento, ItemPlano};
use resiliencia_comum::*;
use schematize_plataforma::persistencia::snapshot::Snapshot;
use schematize_plataforma::persistencia::Rng;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Variável que transforma `filho_martelador` no processo que martela o journal.
const ENV_DIR: &str = "OD_MARTELO_DIR";

/// **O quê:** o evento k: planeja o item `i<k>` (então, sempre, `itens == seq`). **Onde:** martelador e fixtures.
fn evento(k: u64) -> Evento {
    let item = ItemPlano {
        id: format!("i{k}"),
        brief: format!("brief {k}"),
        tipo: "t".into(),
        teto_mib: None,
        marca: None,
        argv: vec![],
    };
    Evento::Plano { prefixo: String::new(), modelo_base: String::new(), itens: vec![item] }
}

/// **O quê:** processo FILHO: martela `registrar` com snapshot a cada 2 eventos até levar SIGKILL;
/// anota em `ack` cada seq confirmado. Fora do modo filho, passa direto. **Onde:** o teste (b).
#[test]
fn filho_martelador() {
    let Ok(dir) = std::env::var(ENV_DIR) else { return };
    let dir = PathBuf::from(dir);
    let pol = PoliticaSnapshot { cada_eventos: 2, cada_segundos: 1 };
    let mut d = Duravel::abrir(&dir.join("estado"), pol).expect("abrir no filho");
    let mut ack =
        std::fs::OpenOptions::new().create(true).append(true).open(dir.join("ack")).expect("ack");
    for _ in 0..1_000_000 {
        let k = d.seq() + 1;
        d.registrar(&evento(k)).expect("registrar no filho");
        writeln!(ack, "{k}").expect("ack");
    }
}

/// **O quê:** maior seq confirmado pelo filho. **Onde:** o teste (b).
fn ultimo_ack(dir: &Path) -> u64 {
    let t = std::fs::read_to_string(dir.join("ack")).unwrap_or_default();
    t.lines().filter_map(|l| l.trim().parse().ok()).max().unwrap_or(0)
}

/// **O quê:** o segmento mais novo do journal. **Onde:** simular cauda torn.
fn ultimo_segmento(estado: &Path) -> Option<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(estado).ok()?.flatten().map(|e| e.path()).collect();
    v.retain(|p| p.extension().is_some_and(|e| e == "wal"));
    v.sort();
    v.pop()
}

/// **O quê:** invariantes do estado recuperado: abre sem erro nem quarentena e `itens == seq` com
/// ids sequenciais. Devolve o seq. **Onde:** o teste (b).
fn confere_consistente(dir: &Path, ctx: &str) -> u64 {
    let d = Duravel::abrir(&dir.join("estado"), PoliticaSnapshot::default())
        .unwrap_or_else(|e| panic!("{ctx}: não abriu: {e}"));
    assert!(
        d.abertura.quarentena.is_none(),
        "{ctx}: kill virou quarentena: {:?}",
        d.abertura.motivo_quarentena
    );
    let (e, seq) = (d.estado(), d.seq());
    assert_eq!(e.itens.len() as u64, seq, "{ctx}: itens != seq");
    for k in 1..=seq {
        assert!(e.itens.contains_key(&format!("i{k}")), "{ctx}: faltou i{k} (buraco)");
    }
    seq
}

/// (b) SIGKILL no meio de gravações de journal e snapshot, 40 vezes; metade com cauda torn extra.
#[test]
fn kill_durante_gravacao_de_journal_e_snapshot() {
    let seed = semente("persist");
    let mut rng = Rng::novo(seed);
    let dir = tmp("martelo");
    let (mut descartes, mut maior) = (0u64, 0u64);
    for it in 0..40 {
        let mut c = Command::new(std::env::current_exe().expect("exe"))
            .args(["--exact", "filho_martelador", "--nocapture", "--test-threads=1"])
            .env(ENV_DIR, &dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("filho");
        std::thread::sleep(atraso(&mut rng, 30, 250));
        matar(&mut c);
        let acked = ultimo_ack(&dir);
        let estado = dir.join("estado");
        let chop = it % 2 == 1;
        if chop {
            if let Some(seg) = ultimo_segmento(&estado) {
                let len = std::fs::metadata(&seg).map_or(0, |m| m.len());
                if len > 64 {
                    let f = std::fs::OpenOptions::new().write(true).open(&seg).expect("seg");
                    f.set_len(len - 1 - rng.ate(40)).expect("truncar");
                    descartes += 1;
                }
            }
        }
        let seq = confere_consistente(&dir, &format!("it{it} (seed {seed})"));
        if !chop {
            assert!(
                seq >= acked,
                "it{it}: evento CONFIRMADO perdido (seq {seq} < ack {acked}, seed {seed})"
            );
        }
        maior = maior.max(seq);
    }
    eprintln!("[persist] 40 kills, {descartes} caudas truncadas, seq final {maior}, seed {seed}");
    assert!(maior > 40, "o martelador quase não gravou (seed {seed})");
    let _ = std::fs::remove_dir_all(&dir);
}

/// **O quê:** estado com `n` eventos e snapshots a cada 3 (atual + `.prev`). **Onde:** testes (f).
fn estado_pronto(dir: &Path, n: u64) -> EstadoOverdev {
    let pol = PoliticaSnapshot { cada_eventos: 3, cada_segundos: 3600 };
    let mut d = Duravel::abrir(&dir.join("estado"), pol).expect("abrir");
    for k in 1..=n {
        d.registrar(&evento(k)).expect("registrar");
    }
    d.estado().clone()
}

/// **O quê:** reabre e devolve (estado, aviso do snapshot, bytes descartados). **Onde:** testes (f).
fn reabrir(dir: &Path) -> (EstadoOverdev, Option<String>, u64) {
    let d = Duravel::abrir(&dir.join("estado"), PoliticaSnapshot::default()).expect("reabrir");
    (
        d.estado().clone(),
        d.abertura.relatorio.snapshot_aviso.clone(),
        d.abertura.relatorio.bytes_descartados,
    )
}

/// (f) snapshot atual com bit-flip ou truncado: cai no `.prev`, reporta e o estado é IDÊNTICO.
#[test]
fn snapshot_ruim_cai_no_prev_e_reporta() {
    for (nome, dano) in [("bitflip", 0), ("truncado", 1), ("vazio", 2)] {
        let dir = tmp(&format!("snap-{nome}"));
        let original = estado_pronto(&dir, 10);
        let snap = dir.join("estado/snapshot.bin");
        let mut b = std::fs::read(&snap).expect("snapshot");
        match dano {
            0 => {
                let m = b.len() / 2;
                b[m] ^= 0x40;
            }
            1 => b.truncate(b.len() / 2),
            _ => b.clear(),
        }
        std::fs::write(&snap, b).expect("danificar");
        let (e, aviso, _) = reabrir(&dir);
        assert!(aviso.is_some_and(|a| a.contains(".prev")), "{nome}: não reportou o .prev");
        assert_eq!(e, original, "{nome}: estado diferente do original");
    }
}

/// (f) cauda do journal com bit-flip/truncada: descarta SÓ a cauda, reporta e segue consistente.
#[test]
fn journal_com_cauda_ruim_descarta_so_a_cauda() {
    for dano in 0..2 {
        let dir = tmp(&format!("cauda-{dano}"));
        estado_pronto(&dir, 10);
        let seg = ultimo_segmento(&dir.join("estado")).expect("segmento");
        let mut b = std::fs::read(&seg).expect("seg");
        if dano == 0 {
            let n = b.len();
            b[n - 3] ^= 0x01;
        } else {
            b.truncate(b.len() - 5);
        }
        std::fs::write(&seg, b).expect("danificar");
        let (e, _, descartados) = reabrir(&dir);
        assert!(descartados > 0, "dano {dano}: não reportou a cauda descartada");
        assert_eq!(e.itens.len(), e.ordem.len());
        assert!(e.itens.len() >= 9, "perdeu mais que a cauda: {}", e.itens.len());
    }
}

/// (f) os dois snapshots destruídos: replay do zero pelo journal reconstrói o estado inteiro.
#[test]
fn snapshots_destruidos_replay_do_zero() {
    let dir = tmp("sem-snap");
    let original = estado_pronto(&dir, 10);
    for f in ["snapshot.bin", "snapshot.prev"] {
        std::fs::write(dir.join("estado").join(f), b"lixo").expect("danificar");
    }
    let (e, aviso, _) = reabrir(&dir);
    assert!(aviso.is_some_and(|a| a.contains("replay do zero")));
    assert_eq!(e, original);
}

/// (f) esquema alheio (não recuperável): quarentena, recomeço limpo, tudo reportado, sem panic.
#[test]
fn esquema_incompativel_vai_para_quarentena() {
    let dir = tmp("esquema");
    estado_pronto(&dir, 5);
    let s = Snapshot { versao_esquema: 99, seq: 5, dados: b"{}".to_vec() };
    s.gravar(&dir.join("estado")).expect("gravar");
    let d =
        Duravel::abrir(&dir.join("estado"), PoliticaSnapshot::default()).expect("não pode falhar");
    let q = d.abertura.quarentena.clone().expect("quarentena");
    assert!(d.abertura.motivo_quarentena.as_deref().is_some_and(|m| m.contains("v99")));
    assert!(d.estado().itens.is_empty() && q.is_dir());
    assert!(
        std::fs::read_dir(q).expect("q").count() >= 1,
        "os arquivos originais ficaram na quarentena"
    );
}

/// (f) fuzz: 300 bit-flips aleatórios em qualquer arquivo de estado nunca causam panic, e o que
/// abre é consistente (`itens == ordem`).
#[test]
fn fuzz_de_bitflips_nunca_da_panic() {
    let seed = semente("fuzz");
    let mut rng = Rng::novo(seed);
    let base = tmp("fuzz-base");
    estado_pronto(&base, 12);
    let arqs: Vec<PathBuf> =
        std::fs::read_dir(base.join("estado")).expect("ls").flatten().map(|e| e.path()).collect();
    let arqs: Vec<PathBuf> =
        arqs.into_iter().filter(|p| p.file_name().is_some_and(|n| n != "journal.lock")).collect();
    for i in 0..300 {
        let w = tmp("fuzz-w");
        std::fs::create_dir_all(w.join("estado")).expect("w");
        for p in &arqs {
            std::fs::copy(p, w.join("estado").join(p.file_name().expect("nome"))).expect("copiar");
        }
        let alvo = w
            .join("estado")
            .join(arqs[rng.ate(arqs.len() as u64) as usize].file_name().expect("n"));
        let mut b = std::fs::read(&alvo).expect("ler");
        let pos = rng.ate(b.len() as u64) as usize;
        b[pos] ^= 1 << rng.ate(8);
        std::fs::write(&alvo, b).expect("flip");
        if let Ok(d) = Duravel::abrir(&w.join("estado"), PoliticaSnapshot::default()) {
            let e = d.estado();
            assert_eq!(e.itens.len(), e.ordem.len(), "flip {i} em {alvo:?} (seed {seed})");
        }
        let _ = std::fs::remove_dir_all(&w);
    }
    let _ = std::fs::remove_dir_all(&base);
}
