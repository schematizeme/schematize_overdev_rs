//! A CASCA de plataforma — o que todo app da casa repete, e nada de domínio.
//!
//! **O quê:** procedência do binário, resolução de executável, utilitários de caminho.
//!
//! **Onde:** `main` e os subcomandos.
//!
//! **Por que é copiada e não uma lib comum (D4 do ADR-0016):** uma `commons` de domínio é vetada
//! pelo piso 6, e uma de plataforma amarraria a versão de sete repos por causa de duzentas
//! linhas. O duplicado aqui não sabe **nada** do negócio — se um dia algo de domínio migrar para
//! esta pasta, o corte foi feito errado.

pub mod agente;
pub mod bin;
pub mod caminhos;
pub mod desktop;
pub mod icone;

pub mod procedencia;
pub mod util;
