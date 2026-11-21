//! Configuração do handoff vivo: workspace, destinos, intervalos e parâmetros da camada Claude.
//!
//! **O quê:** [`Config`] (defaults: mecânico 30 s, Claude 1800 s, modelo `haiku`) e a descoberta do
//! workspace (`<projeto>/<projeto>_archive`) subindo a partir de um diretório.
//! **Onde:** [`super::comandos`] monta a partir dos argumentos; testes montam direto.

use std::path::{Path, PathBuf};

/// Intervalo padrão da camada mecânica (s).
pub const MECANICO_S: u64 = 30;
/// Intervalo padrão da camada Claude (s).
pub const CLAUDE_S: u64 = 1800;
/// Modelo barato padrão da camada Claude.
pub const MODELO: &str = "haiku";

/// Tudo que o motor precisa saber.
#[derive(Debug, Clone)]
pub struct Config {
    /// Raiz do workspace (pasta do projeto).
    pub workspace: PathBuf,
    /// Nome do projeto (basename do workspace).
    pub projeto: String,
    /// `<workspace>/<projeto>_archive`.
    pub archive: PathBuf,
    /// Intervalo mecânico (s).
    pub mecanico_s: u64,
    /// Intervalo da camada Claude (s).
    pub claude_s: u64,
    /// Desliga a camada Claude.
    pub sem_claude: bool,
    /// Modelo do `claude -p`.
    pub modelo: String,
    /// Comando substituto do `claude` (stub de teste); `None` = `claude` real, jaulado.
    pub claude_cmd: Option<Vec<String>>,
    /// Timeout do `claude -p` (s).
    pub claude_timeout_s: u64,
    /// Teto de memória do stub (MiB, `RLIMIT_AS`); o `claude` real usa a jaula do systemd.
    pub claude_mem_mib: u64,
    /// Tamanho máximo das notas gravadas (bytes).
    pub max_notas: usize,
    /// Janela de "arquivos tocados" (s).
    pub janela_tocados_s: u64,
}

impl Config {
    /// **O quê:** config com os defaults para `workspace`. **Onde:** [`Self::detectar`] e testes.
    pub fn para(workspace: &Path) -> Config {
        let projeto =
            workspace.file_name().and_then(|s| s.to_str()).unwrap_or("projeto").to_string();
        Config {
            archive: workspace.join(format!("{projeto}_archive")),
            workspace: workspace.to_path_buf(),
            projeto,
            mecanico_s: MECANICO_S,
            claude_s: CLAUDE_S,
            sem_claude: false,
            modelo: MODELO.to_string(),
            claude_cmd: None,
            claude_timeout_s: 300,
            claude_mem_mib: 1024,
            max_notas: 6000,
            janela_tocados_s: 1800,
        }
    }

    /// **O quê:** sobe a partir de `inicio` até achar `<d>/<nome(d)>_archive`; sem achar, usa
    /// `inicio` (o archive será criado). **Onde:** `executar` (cwd ou `--projeto`).
    pub fn detectar(inicio: &Path) -> Config {
        let base = inicio.canonicalize().unwrap_or_else(|_| inicio.to_path_buf());
        for d in base.ancestors() {
            let nome = d.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if !nome.is_empty() && d.join(format!("{nome}_archive")).is_dir() {
                return Config::para(d);
            }
        }
        Config::para(&base)
    }

    /// **O quê:** `<archive>/context` (handoffs). **Onde:** gravação e `verificar`.
    pub fn dir_context(&self) -> PathBuf {
        self.archive.join("context")
    }

    /// **O quê:** `<workspace>/.schematize/handoff-vivo` (operacional: lock, estado, erros).
    /// **Onde:** motor, `status`, `verificar`.
    pub fn dir_operacional(&self) -> PathBuf {
        crate::paths::schematize_dir_at(&self.workspace).join("handoff-vivo")
    }

    /// **O quê:** dir do overdev do workspace (checklist, `estado/`, `jaulas.jsonl`, ledger).
    /// **Onde:** [`super::coleta`], ledger.
    pub fn dir_overdev(&self) -> PathBuf {
        crate::paths::overdev_dir_at(&self.workspace)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn detecta_o_workspace_pelo_archive() {
        let d = std::env::temp_dir().join(format!("hv-cfg-{}", std::process::id())).join("proj");
        std::fs::create_dir_all(d.join("proj_archive")).unwrap();
        std::fs::create_dir_all(d.join("sub/x")).unwrap();
        let c = Config::detectar(&d.join("sub/x"));
        assert_eq!(c.projeto, "proj");
        assert!(c.dir_context().ends_with("proj_archive/context"));
        assert_eq!((c.mecanico_s, c.claude_s, c.modelo.as_str()), (30, 1800, "haiku"));
        let _ = std::fs::remove_dir_all(d.parent().unwrap());
    }
}
