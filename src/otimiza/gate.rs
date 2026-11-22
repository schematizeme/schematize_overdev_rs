//! Resultado de gate comprimido (IA-15).
//!
//! **O quê:** reduz o log de um gate (fmt/clippy/test) a falhas resumidas — as primeiras N linhas
//! relevantes, cada uma cortada — e aponta o log completo, que continua em arquivo.
//! **Onde:** runner de gate do orquestrador, ANTES de montar o brief de retry: o agente recebe
//! o resumo (poucos tokens), não 10 mil linhas. Nada vai ao modelo daqui; é filtro local.

use std::path::{Path, PathBuf};

/// Limites do resumo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    /// Máximo de linhas no resumo.
    pub max_linhas: usize,
    /// Máximo de bytes no resumo.
    pub max_bytes: usize,
    /// Máximo de caracteres por linha.
    pub max_coluna: usize,
}

impl Default for Config {
    /// 40 linhas, 4 KiB, 240 colunas (~1.000 tokens no pior caso).
    fn default() -> Self {
        Config { max_linhas: 40, max_bytes: 4096, max_coluna: 240 }
    }
}

/// Resumo do gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comprimido {
    /// Texto para o agente (cabeçalho + linhas).
    pub texto: String,
    /// Linhas do log original.
    pub linhas_total: usize,
    /// Linhas relevantes achadas (antes do corte).
    pub linhas_relevantes: usize,
    /// Houve corte (linhas ou bytes)?
    pub truncado: bool,
}

const MARCAS: [&str; 10] = [
    "error",
    "failed",
    "panicked",
    "fail",
    "assertion",
    "warning:",
    "-->",
    "test result",
    "denied",
    "✗",
];

/// A linha é relevante para diagnóstico? **O quê:** casa marcas sem diferenciar caixa.
/// **Onde:** [`comprimir`].
pub fn e_relevante(l: &str) -> bool {
    let m = l.to_lowercase();
    MARCAS.iter().any(|k| m.contains(k))
}

/// Corta `s` em `max` caracteres (fronteira de char), com reticências. **Onde:** [`comprimir`].
fn cortar(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let t: String = s.chars().take(max).collect();
    format!("{t}…")
}

/// Comprime o log. Sem linha relevante, devolve a CAUDA (o fim costuma ter a causa).
///
/// **O quê:** respeita `max_linhas`/`max_bytes`/`max_coluna`; o cabeçalho informa totais e
/// `caminho_log` (onde o log completo está). **Onde:** runner de gate antes do retry.
pub fn comprimir(log: &str, cfg: &Config, caminho_log: Option<&Path>) -> Comprimido {
    let todas: Vec<&str> = log.lines().collect();
    let rel: Vec<&str> = todas.iter().copied().filter(|l| e_relevante(l)).collect();
    let base: Vec<&str> = if rel.is_empty() {
        let ini = todas.len().saturating_sub(cfg.max_linhas);
        todas[ini..].to_vec()
    } else {
        rel.iter().copied().take(cfg.max_linhas).collect()
    };
    let origem = if rel.is_empty() { todas.len() } else { rel.len() };
    let mut corpo = String::new();
    let mut usadas = 0usize;
    for l in &base {
        let l = cortar(l, cfg.max_coluna);
        if corpo.len() + l.len() + 1 > cfg.max_bytes {
            break;
        }
        corpo.push_str(&l);
        corpo.push('\n');
        usadas += 1;
    }
    let onde = caminho_log.map_or("indisponivel".to_string(), |p| p.display().to_string());
    Comprimido {
        texto: format!(
            "[gate: {usadas} de {} linhas ({} relevantes); log completo: {onde}]\n{corpo}",
            todas.len(),
            rel.len()
        ),
        linhas_total: todas.len(),
        linhas_relevantes: rel.len(),
        truncado: usadas < origem,
    }
}

/// Grava o log completo em `<dir>/<nome>.log` (nome só `[A-Za-z0-9_.-]`, sem `..`).
///
/// **O quê:** preserva o original que o resumo aponta. **Onde:** runner de gate, antes de [`comprimir`].
pub fn salvar_log(dir: &Path, nome: &str, log: &str) -> Result<PathBuf, String> {
    let ok = !nome.is_empty()
        && !nome.contains("..")
        && nome.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b));
    if !ok {
        return Err(format!("nome de log invalido: `{nome}`"));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("criar {}: {e}", dir.display()))?;
    let p = dir.join(format!("{nome}.log"));
    std::fs::write(&p, log).map_err(|e| format!("gravar {}: {e}", p.display()))?;
    Ok(p)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn log_grande() -> String {
        let mut s = String::new();
        for i in 0..10_000 {
            s.push_str(&format!("   Compiling crate{i} v0.1.0\n"));
            if i % 2_000 == 0 {
                s.push_str(&format!("error[E0599]: sem metodo {i}\n --> src/x.rs:{i}:1\n"));
            }
        }
        s.push_str("test result: FAILED. 1 passed; 1 failed\n");
        s
    }

    #[test]
    fn dez_mil_linhas_viram_resumo_com_as_falhas_e_o_caminho() {
        let d = std::env::temp_dir().join(format!("otz-gate-{}", std::process::id()));
        let log = log_grande();
        let p = salvar_log(&d, "clippy-1", &log).unwrap();
        let c = comprimir(&log, &Config::default(), Some(&p));
        assert!(c.texto.lines().count() <= 41, "{}", c.texto.lines().count());
        assert!(c.texto.len() <= 4096 + 200);
        assert!(c.texto.contains("E0599") && c.texto.contains("test result"));
        assert!(c.texto.contains(&p.display().to_string()));
        assert_eq!(std::fs::read_to_string(&p).unwrap(), log, "original intacto");
        assert_eq!(c.linhas_total, 10_000 + 5 * 2 + 1);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn limites_hostis_cauda_e_unicode() {
        let cfg = Config { max_linhas: 3, max_bytes: 10_000, max_coluna: 5 };
        let c = comprimir("a\nb\nc\nd\ne\n", &cfg, None);
        assert!(c.texto.ends_with("c\nd\ne\n"), "sem relevante: cauda");
        let c = comprimir(&format!("error {}\n", "ç".repeat(100)), &cfg, None);
        assert!(c.texto.contains("error…"));
        let min = Config { max_linhas: 5, max_bytes: 0, max_coluna: 10 };
        assert!(comprimir("error x", &min, None).truncado);
        assert_eq!(comprimir("", &Config::default(), None).linhas_total, 0);
        for ruim in ["", "../x", "a/b", "ç"] {
            assert!(salvar_log(Path::new("/x"), ruim, "l").is_err(), "{ruim}");
        }
    }
}
