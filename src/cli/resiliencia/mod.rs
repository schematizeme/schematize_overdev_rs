//! Comandos de resiliência: `executar`/`retomar`, `sistema diag`, `memoria status`, `vigia`.
//!
//! **O quê:** a borda de linha de comando sobre `crate::resiliencia`. O corpo (estado durável,
//! reconciliação, governador, executor) mora na lib; aqui é só entrada e saída.
//! **Onde:** `cli::comandos::executar` despacha para estas funções.

mod diagnostico;
mod executar;
mod vigia;

use crate::cli::args::{MemoriaCmd, SistemaCmd, VigiaCmd};

pub use executar::executar;

/// **O quê:** troca um `--resume` solto na primeira posição (`overdev --resume`) por `retomar`.
/// **Onde:** `cli::run`, antes do parse do clap. Nas demais posições `--resume` é no-op aceito.
pub fn normalizar_resume(mut args: Vec<String>) -> Vec<String> {
    if args.get(1).is_some_and(|a| a == "--resume") {
        args[1] = "retomar".to_string();
    }
    args
}

/// **O quê:** despacha `overdev sistema ...`. **Onde:** `comandos::executar`.
pub fn sistema(sub: SistemaCmd) -> Result<(), String> {
    match sub {
        SistemaCmd::Diag { json, lang } => diagnostico::sistema_diag(json, lang.as_deref()),
    }
}

/// **O quê:** despacha `overdev memoria ...`. **Onde:** `comandos::executar`.
pub fn memoria(sub: MemoriaCmd) -> Result<(), String> {
    match sub {
        MemoriaCmd::Status { json, estado } => diagnostico::memoria_status(json, estado),
    }
}

/// **O quê:** despacha `overdev vigia ...`. **Onde:** `comandos::executar`.
pub fn vigia(sub: VigiaCmd) -> Result<(), String> {
    match sub {
        VigiaCmd::Unit { estado } => vigia::unit(estado),
        VigiaCmd::Run { args } => vigia::run(args),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// `--resume` solto vira `retomar`; nas outras posições nada muda.
    #[test]
    fn resume_solto_vira_retomar() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(normalizar_resume(v(&["od", "--resume"])), v(&["od", "retomar"]));
        assert_eq!(
            normalizar_resume(v(&["od", "--resume", "--estado", "x"])),
            v(&["od", "retomar", "--estado", "x"])
        );
        assert_eq!(
            normalizar_resume(v(&["od", "status", "--resume"])),
            v(&["od", "status", "--resume"])
        );
        assert_eq!(normalizar_resume(v(&["od"])), v(&["od"]));
    }
}
