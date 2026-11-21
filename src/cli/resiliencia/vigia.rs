//! `overdev vigia unit` e `overdev vigia run`: o watchdog do orquestrador.
//!
//! **O quê:** `unit` imprime a unit systemd do `schematize-vigia` (NADA é instalado); `run`
//! delega ao binário `schematize-vigia` se estiver no PATH — senão devolve uma mensagem acionável
//! com como obtê-lo. Nunca baixa nada sozinho.
//! **Onde:** `cli::resiliencia::vigia`. O vigia relança `overdev retomar --estado <dir>` com
//! `--resume` quando o processo morre (SIGKILL/OOM/exit anormal), com backoff e circuit breaker.

use schematize_plataforma::bin::resolve_bin;
use schematize_plataforma::persistencia::{unit_systemd, VigiaCfg};
use std::path::PathBuf;
use std::process::Command;

/// Nome do binário do vigia.
const BIN_VIGIA: &str = "schematize-vigia";

/// **O quê:** o estado durável absoluto (`--estado` ou `<control-plane>/estado`). **Onde:** `unit`/`run`.
fn estado_abs(estado: Option<PathBuf>) -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| format!("cwd inacessível: {e}"))?;
    let e = estado.unwrap_or_else(|| crate::resiliencia::dir_estado(&cwd));
    Ok(if e.is_absolute() { e } else { cwd.join(e) })
}

/// **O quê:** `VigiaCfg` que vigia `overdev retomar --estado <dir>`. **Onde:** `unit` e `run`.
fn cfg_padrao(estado: PathBuf) -> VigiaCfg {
    let mut cfg = VigiaCfg::novo(crate::nucleo::util::self_exe(), estado.clone());
    cfg.exe_vigia = resolve_bin(BIN_VIGIA).unwrap_or_else(|| PathBuf::from(BIN_VIGIA));
    cfg.args = vec!["retomar".into(), "--estado".into(), estado.to_string_lossy().into_owned()];
    cfg
}

/// **O quê:** imprime a unit (slice + service). **Onde:** `overdev vigia unit`. Só texto.
pub fn unit(estado: Option<PathBuf>) -> Result<(), String> {
    let cfg = cfg_padrao(estado_abs(estado)?);
    print!("{}", unit_systemd(&cfg));
    Ok(())
}

/// **O quê:** mensagem acionável quando o `schematize-vigia` não está no PATH. **Onde:** [`run`].
pub fn mensagem_sem_vigia() -> String {
    format!(
        "o `{BIN_VIGIA}` não está no PATH, então não consegui iniciar o vigia. Eu não baixo nada \
         sozinho. Para obtê-lo: `cargo install --git \
         https://github.com/schematizeme/schematize_plataforma_rs --tag v0.4.0 --features \
         persistencia --bin {BIN_VIGIA} schematize_plataforma` (ou instale pelo \
         `schematize-market`) e repita. Sem o vigia, `overdev retomar` continua funcionando à mão."
    )
}

/// **O quê:** delega ao `schematize-vigia` repassando `args` (vazio = vigia o `retomar` padrão) e
/// propaga o código de saída (3 = circuito aberto/parked). **Onde:** `overdev vigia run -- ...`.
pub fn run(args: Vec<String>) -> Result<(), String> {
    let bin = resolve_bin(BIN_VIGIA).ok_or_else(mensagem_sem_vigia)?;
    let args = if args.is_empty() { args_padrao(estado_abs(None)?) } else { args };
    let st = Command::new(&bin)
        .args(&args)
        .status()
        .map_err(|e| format!("não consegui executar {}: {e}", bin.display()))?;
    match st.code() {
        Some(0) => Ok(()),
        Some(c) => std::process::exit(c),
        None => Err("o schematize-vigia terminou por sinal".to_string()),
    }
}

/// **O quê:** argv do vigia para o `retomar` padrão. **Onde:** [`run`]. PURA.
fn args_padrao(estado: PathBuf) -> Vec<String> {
    let e = estado.to_string_lossy().into_owned();
    vec![
        "--estado".into(),
        e.clone(),
        "--".into(),
        crate::nucleo::util::self_exe(),
        "retomar".into(),
        "--estado".into(),
        e,
    ]
}

#[cfg(test)]
mod testes {
    use super::*;

    /// A unit traz `Restart=always`, o `retomar` e o estado absoluto.
    #[test]
    fn unit_vigia_o_retomar() {
        let cfg = cfg_padrao(PathBuf::from("/p/.overdev/estado"));
        let u = unit_systemd(&cfg);
        for d in ["Restart=always", "\"retomar\"", "\"/p/.overdev/estado\"", "OOMPolicy=continue"] {
            assert!(u.contains(d), "faltou {d}\n{u}");
        }
    }

    /// A mensagem sem vigia diz como obter e que nada é baixado sozinho.
    #[test]
    fn mensagem_e_acionavel() {
        let m = mensagem_sem_vigia();
        assert!(m.contains("cargo install") && m.contains("não baixo nada"), "{m}");
    }
}
