//! Checklist PARTICIONADO (ADR-0027): partições de 25–50 itens com prioridade e `--completed`
//! no nome. Quem lê afazeres vê só as abertas; contagens incluem tudo.
//!
//! **Onde:** CLI `schematize-overdev checklist particionar|sync|check|proxima` e o Stop-hook.

pub mod estado;
pub mod indice;
pub mod lint;
pub mod migrar;
pub mod nome;
pub mod secoes;

#[cfg(test)]
mod testes;

pub use estado::{listar, tem_particoes, Estado, Listagem, Particao};
pub use indice::{sync, Mudancas};
pub use lint::{checar, integridade, proxima, Relatorio};
pub use migrar::{particionar, Opcoes};
pub use nome::{parse, Nome};
