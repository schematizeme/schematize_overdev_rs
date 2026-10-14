//! Estrutura de AGENTES do overdev (ADR-0024): quem faz o quê, com qual motor e modelo.
//!
//! **O quê:** o arquivo `agentes.toml` (global + override por projeto), com defaults embutidos,
//! validação tipada, a disponibilidade dos motores (claude/codex/cursor) e a execução de uma
//! delegação (`schematize-overdev delegar`).
//!
//! **Onde:** o `ClaudeRunner` (passa `--model` do papel `principal`), o prompt do overdev
//! (tabela de papéis + escada), o subcomando `delegar` e a janela (salva pelos `salvar_*`).
//!
//! ## API que a janela consome
//! - [`carregar`] / [`carregar_ou_padrao`] — a config EFETIVA (padrões < global < projeto).
//! - [`Agentes::papel`], [`Agentes::escada`], [`Agentes::validar`], [`Agentes::para_toml`].
//! - [`salvar_global`] / [`salvar_projeto`] — escrita atômica (valida antes de gravar).
//! - [`disponibilidade`] — o motor está instalado e logado?
//!
//! A divisão segue a razão de mudança: `schema` (forma + defaults), `erro`, `carga`
//! (arquivo/merge/validação de entrada), `motores` (linha de comando + disponibilidade) e
//! `delegar` (execução).

mod carga;
mod delegar;
mod erro;
mod motores;
mod schema;

pub use carga::{
    caminho_global, caminho_projeto, carregar, carregar_ou_padrao, salvar_global, salvar_projeto,
};
pub use delegar::{delegar, proximo_ativo, proximo_disponivel, Desfecho, Pedido, Resultado};
pub use erro::ErroAgentes;
pub use motores::{disponibilidade, linha_de_comando, Disponibilidade};
pub use schema::{Agentes, Motor, Papel};

#[cfg(test)]
mod testes;
