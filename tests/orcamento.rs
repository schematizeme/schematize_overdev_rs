//! Orçamento de memória (BUDGET.md): o binário REAL, em operações representativas, não pode
//! passar do teto de RSS de pico.
//!
//! **O quê:** sobe `schematize-overdev <op>` e lê o pico de RSS do processo (`ru_maxrss` do
//! `wait4`, o mesmo número que `VmHWM` de `/proc/<pid>/status`) — o polling de `/proc` perderia
//! o pico de um processo que vive milissegundos. **Onde:** gate local (`cargo test`).
//!
//! **Limite do método:** o `ru_maxrss` herda o RSS do processo-teste no instante do `exec`, então
//! o número é um TETO HONESTO (nunca subestima). Os tetos de [`OPERACOES`] são o medido + ~30%, num build de DEBUG (o release mede ~5 MiB:
//! BUDGET.md).
//!
//! **Sem `/proc`** o teste AVISA no stderr e pula (nunca `#[ignore]` mudo).

use std::process::{Command, Stdio};

/// (nome, argumentos, teto de RSS em KiB). Tetos = medido + ~30% (ver BUDGET.md); nenhum acima
/// do teto de 256 MiB da casa para CLI.
const OPERACOES: &[(&str, &[&str], u64)] = &[
    ("version", &["--version"], 10 * 1024),
    ("agents", &["agents"], 21 * 1024),
    ("hooks", &["hooks"], 12 * 1024),
    ("status", &["status"], 21 * 1024),
    ("checklist", &["checklist"], 21 * 1024),
];

/// **O quê:** caminho do binário do overdev (o que o cargo compilou para este teste).
/// **Onde:** [`pico_kib`].
fn bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_schematize-overdev"))
}

/// **O quê:** roda o binário até sair e devolve o pico de RSS em KiB. **Onde:** o teste abaixo.
/// Usa `wait4` direto (a `libc` já é dependência) porque `Child::wait` descarta o `rusage`.
fn pico_kib(args: &[&str]) -> Result<u64, String> {
    let mut c = Command::new(bin());
    c.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // HOME isolado: o orçamento mede o binário, não o que há na casa de quem roda o teste.
    let home = std::env::temp_dir().join(format!("od-orcamento-{}", std::process::id()));
    std::fs::create_dir_all(&home).map_err(|e| format!("tmp: {e}"))?;
    c.env("HOME", &home).env("LANG", "en_US.UTF-8").current_dir(&home);
    let child = c.spawn().map_err(|e| format!("spawn {}: {e}", bin().display()))?;
    let pid = child.id() as libc::pid_t;
    // Quem reapa é o `wait4` abaixo; `child` sai de escopo sem `wait` (não há zumbi: já reapado).
    let mut status: libc::c_int = 0;
    // SAFETY: `rusage` é POD (zerado é válido) e `status`/`rusage` vivem durante a chamada.
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    let r = unsafe { libc::wait4(pid, &mut status, 0, &mut ru) };
    let _ = std::fs::remove_dir_all(&home);
    if r != pid {
        return Err(format!("wait4 devolveu {r}"));
    }
    std::mem::forget(child);
    u64::try_from(ru.ru_maxrss).map_err(|e| format!("ru_maxrss: {e}"))
}

/// Cada operação fica abaixo do teto declarado em BUDGET.md.
#[test]
fn rss_de_pico_dentro_do_orcamento() {
    if !std::path::Path::new("/proc/self/status").exists() {
        eprintln!("PULEI orcamento: /proc indisponível (sem como medir RSS)");
        return;
    }
    for (nome, args, teto) in OPERACOES {
        let pico = pico_kib(args).unwrap_or_else(|e| panic!("{nome}: {e}"));
        eprintln!("orcamento {nome}: pico {} KiB (teto {teto} KiB)", pico);
        assert!(pico <= *teto, "{nome}: RSS de pico {pico} KiB passou do teto {teto} KiB");
    }
}

/// Self-check: o medidor NÃO está cego — um teto absurdamente baixo tem de ser violado.
#[test]
fn medidor_enxerga_o_pico() {
    if !std::path::Path::new("/proc/self/status").exists() {
        eprintln!("PULEI medidor: /proc indisponível");
        return;
    }
    let pico = pico_kib(&["--version"]).expect("mede");
    assert!(pico > 256, "pico {pico} KiB implausível (< 256 KiB): medidor cego");
}
