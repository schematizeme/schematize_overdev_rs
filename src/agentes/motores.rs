//! Motores (claude/codex/cursor): linha de comando não-interativa e disponibilidade.
//!
//! **O quê:** monta o ARGV de cada motor (nunca uma string de shell) e responde "este motor
//! está instalado e logado?".
//! **Onde:** `delegar` (executa), `--listar` e a janela (badge de disponibilidade).

use super::schema::Motor;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Variável que restringe a busca de binários ao `$PATH` (sem os dirs de fallback do HOME e
/// /usr). Existe para o teste provar "motor ausente" sem depender da máquina onde roda.
pub const ENV_SO_PATH: &str = "SCHEMATIZE_AGENTES_SO_PATH";

/// Tempo máximo do checador de login: um `status` que pede input não pode travar o `delegar`.
const TEMPO_LOGIN: Duration = Duration::from_secs(15);

/// Estado de um motor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disponibilidade {
    /// Binário resolvido e login ok.
    Ok,
    /// Binário não encontrado.
    SemBinario,
    /// Binário existe mas o checador de login falhou (texto com o motivo).
    SemLogin(String),
}

impl Disponibilidade {
    /// **O quê:** `true` só para [`Disponibilidade::Ok`]. **Onde:** `delegar`, `--listar`.
    pub fn ok(&self) -> bool {
        matches!(self, Disponibilidade::Ok)
    }

    /// **O quê:** texto curto para humanos/JSON. **Onde:** `--listar`, mensagens de rc 3.
    pub fn detalhe(&self) -> String {
        match self {
            Disponibilidade::Ok => "ok".to_string(),
            Disponibilidade::SemBinario => "binário não encontrado no PATH".to_string(),
            Disponibilidade::SemLogin(m) => format!("sem login: {m}"),
        }
    }
}

/// **O quê:** o caminho absoluto do binário do motor, ou `None`.
/// **Onde:** [`disponibilidade`] e `delegar`.
pub fn resolver_binario(motor: Motor) -> Option<PathBuf> {
    let bin = motor.binario();
    if std::env::var_os(ENV_SO_PATH).is_some() {
        let path = std::env::var_os("PATH")?;
        return std::env::split_paths(&path).map(|d| d.join(bin)).find(|c| c.is_file());
    }
    crate::agentrun::resolve_bin(bin)
}

/// **O quê:** o argv (SEM o binário) do motor para executar `brief` em `projeto`, conforme o
/// contrato. **Onde:** `delegar`. Nunca passa por shell: cada item é um argumento do processo.
pub fn linha_de_comando(motor: Motor, modelo: &str, projeto: &Path, brief: &str) -> Vec<String> {
    let s = |x: &str| x.to_string();
    // Um brief que começa com `-` seria lido como flag pelo motor; a quebra de linha inicial
    // é inofensiva para o modelo e fecha essa porta.
    let brief = if brief.starts_with('-') { format!("\n{brief}") } else { brief.to_string() };
    match motor {
        Motor::Claude => {
            vec![s("-p"), s("--model"), s(modelo), s("--output-format"), s("json"), brief]
        }
        Motor::Codex => vec![
            s("exec"),
            s("-m"),
            s(modelo),
            s("-s"),
            s("workspace-write"),
            s("--json"),
            s("-C"),
            projeto.display().to_string(),
            brief,
        ],
        Motor::Cursor => vec![
            s("-p"),
            s("--model"),
            s(modelo),
            s("--output-format"),
            s("json"),
            s("--force"),
            brief,
        ],
    }
}

/// **O quê:** roda `bin args` com timeout e devolve o rc (None = não terminou/sinal).
/// **Onde:** [`disponibilidade`] (checagem de login).
fn rc_com_timeout(bin: &Path, args: &[&str]) -> Result<Option<i32>, String> {
    let mut filho = Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{}: {e}", bin.display()))?;
    let inicio = Instant::now();
    loop {
        match filho.try_wait() {
            Ok(Some(st)) => return Ok(st.code()),
            Ok(None) if inicio.elapsed() >= TEMPO_LOGIN => {
                let _ = filho.kill();
                let _ = filho.wait();
                return Err(format!("`{}` não respondeu em {TEMPO_LOGIN:?}", args.join(" ")));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// **O quê:** o motor está pronto para uso? Binário resolvível + login (`codex login status`,
/// `cursor-agent status` com rc 0); o claude é assumido logado. **Onde:** `delegar`, `--listar`.
pub fn disponibilidade(motor: Motor) -> Disponibilidade {
    let Some(bin) = resolver_binario(motor) else {
        return Disponibilidade::SemBinario;
    };
    let args: &[&str] = match motor {
        Motor::Claude => return Disponibilidade::Ok,
        Motor::Codex => &["login", "status"],
        Motor::Cursor => &["status"],
    };
    match rc_com_timeout(&bin, args) {
        Ok(Some(0)) => Disponibilidade::Ok,
        Ok(rc) => Disponibilidade::SemLogin(format!(
            "`{} {}` saiu com {}",
            motor.binario(),
            args.join(" "),
            rc.map_or("sinal".to_string(), |c| c.to_string())
        )),
        Err(e) => Disponibilidade::SemLogin(e),
    }
}
