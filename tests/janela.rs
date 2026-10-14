//! A janela (`schematize-overdev-gui`) como PROCESSO: o que ela faz antes de abrir qualquer coisa.
//!
//! **Onde:** `cargo test`, e o CI.
//!
//! **Por que de integração:** "`--version` responde e SAI sem abrir janela" é propriedade do
//! processo, não de uma função. As janelas irmãs tinham o defeito de ignorar a flag e ABRIR — e o
//! `debugreport` do hub, que pergunta a versão de cada binário, ficava esperando uma janela fechar.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const GUI: &str = env!("CARGO_BIN_EXE_schematize-overdev-gui");

/// **O quê:** roda a janela com `args`, SEM display algum, e espera até `limite`.
///
/// **Onde:** os testes daqui. Sem `DISPLAY`/`WAYLAND_DISPLAY` uma janela que tentasse abrir
/// falharia na hora; uma que ficasse esperando o usuário estouraria o limite. `None` = estourou.
fn rodar_sem_display(args: &[&str], limite: Duration) -> Option<(bool, String)> {
    let mut filho = Command::new(GUI)
        .args(args)
        .env("SCHEMATIZE_SEM_MARKET", "1")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("subir a janela");
    let t0 = Instant::now();
    loop {
        if let Some(st) = filho.try_wait().expect("esperar") {
            let mut out = String::new();
            if let Some(mut s) = filho.stdout.take() {
                std::io::Read::read_to_string(&mut s, &mut out).expect("ler stdout");
            }
            return Some((st.success(), out));
        }
        if t0.elapsed() > limite {
            let _ = filho.kill();
            let _ = filho.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// **`--version` e `-V` respondem e SAEM com 0, antes de qualquer coisa gráfica.**
#[test]
fn version_responde_e_sai_sem_abrir_janela() {
    for flag in ["--version", "-V"] {
        let (ok, out) = rodar_sem_display(&[flag], Duration::from_secs(10))
            .unwrap_or_else(|| panic!("`{flag}` não saiu: a janela abriu em vez de responder"));
        assert!(ok, "`{flag}` saiu com erro");
        assert!(out.starts_with("schematize-overdev-gui "), "{out}");
        assert!(out.contains('('), "sem procedência (o commit) na versão: {out}");
    }
}

/// **Self-check: o teste enxerga uma janela que ABRE.** Sem a flag e sem display, o binário
/// tenta abrir a janela e falha (saída != 0) — se isto passasse com `ok`, o teste acima estaria
/// cego a uma regressão em que `--version` voltasse a abrir.
#[test]
fn sem_a_flag_a_janela_tenta_abrir_e_o_teste_percebe() {
    // `None` (ficou esperando) também prova que NÃO respondeu a versão.
    if let Some((ok, out)) = rodar_sem_display(&[], Duration::from_secs(10)) {
        assert!(!ok, "sem display a janela não pode ter aberto com sucesso: {out}");
        assert!(!out.starts_with("schematize-overdev-gui "), "{out}");
    }
}
