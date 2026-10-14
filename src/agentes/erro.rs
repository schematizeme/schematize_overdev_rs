//! Erros tipados da estrutura de agentes — nada aqui entra em pânico, tudo vira `Err`.
//!
//! **O quê:** um enum com `Display` (o crate não depende de `thiserror`).
//! **Onde:** toda a API de [`super`].

use std::fmt;
use std::path::PathBuf;

/// Tudo que pode dar errado ao ler, validar, gravar ou executar a config de agentes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroAgentes {
    /// Falha de E/S em `caminho`.
    Io { caminho: PathBuf, msg: String },
    /// TOML malformado (ou de tipo errado) em `origem`.
    Toml { origem: String, msg: String },
    /// `versao` diferente de 1.
    Versao(i64),
    /// `motor` fora de claude | codex | cursor.
    MotorDesconhecido { papel: String, motor: String },
    /// `modelo` vazio (ou só espaços).
    ModeloVazio(String),
    /// Nome de papel fora de `[a-z0-9-]{1,32}`.
    NomeInvalido(String),
    /// `escada`/`roteamento` apontam para um papel que não existe.
    PapelAusente { onde: String, papel: String },
    /// O papel `principal` é obrigatório.
    SemPrincipal,
    /// Campo obrigatório ausente no papel.
    CampoAusente { papel: String, campo: String },
    /// Papel pedido não existe na config efetiva.
    PapelDesconhecido(String),
    /// Falha ao serializar para TOML.
    Serializar(String),
    /// Falha ao executar o motor (spawn) ou ao gravar os arquivos da delegação.
    Execucao(String),
}

impl fmt::Display for ErroAgentes {
    /// **O quê:** mensagem acionável em português. **Onde:** `eprintln!` da CLI e logs.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { caminho, msg } => write!(f, "{}: {msg}", caminho.display()),
            Self::Toml { origem, msg } => write!(f, "TOML inválido em {origem}: {msg}"),
            Self::Versao(v) => write!(f, "`versao = {v}` não suportada (esperado 1)"),
            Self::MotorDesconhecido { papel, motor } => write!(
                f,
                "papel `{papel}`: motor `{}` desconhecido (use claude, codex ou cursor)",
                motor.escape_debug()
            ),
            Self::ModeloVazio(p) => write!(f, "papel `{p}`: `modelo` não pode ser vazio"),
            Self::NomeInvalido(n) => write!(
                f,
                "nome de papel inválido `{}` (use [a-z0-9-], 1 a 32 caracteres)",
                n.escape_debug()
            ),
            Self::PapelAusente { onde, papel } => {
                write!(f, "{onde} referencia o papel `{}` que não existe", papel.escape_debug())
            }
            Self::SemPrincipal => write!(f, "o papel `principal` é obrigatório"),
            Self::CampoAusente { papel, campo } => {
                write!(f, "papel `{papel}`: falta o campo `{campo}`")
            }
            Self::PapelDesconhecido(p) => write!(f, "papel `{}` não existe", p.escape_debug()),
            Self::Serializar(m) => write!(f, "não consegui serializar agentes.toml: {m}"),
            Self::Execucao(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for ErroAgentes {}
