//! A borda de LINHA DE COMANDO — o despacho e o que cada subcomando imprime.
//!
//! **Onde:** o `main` de `schematize-overdev` (e o do `.exe` no repo `schematize_windows_rs`)
//! chama [`run`]. O domínio está no resto do crate; aqui é só entrada e saída.
//!
//! ## Este binário é chamado a CADA chamada de ferramenta
//!
//! O hook `PreToolUse` roda `overdev guard` antes de toda ferramenta da sessão, e o `Stop`
//! roda `overdev check` a cada parada. Isso põe dois requisitos que nenhum outro app da casa
//! tem: ele precisa **subir rápido** e **nunca sair com código errado por acidente** — um
//! `guard` que sai não-zero por um motivo que não seja o veto bloqueia a sessão inteira.

use clap::Parser;
use schematize_plataforma::seams::Plataforma;

use crate::hooks::Hooks;
use args::Cli;

pub mod args;
pub mod caixa;
pub mod comandos;
pub mod delegar;
pub mod desktop;
pub mod grafo;
pub mod leituras;
pub mod perguntas;
pub mod prompt;
pub mod saidajson;

/// **O quê:** devolve o `SIGPIPE` ao padrão do Unix (o Rust o ignora; `| head` precisa morrer
/// em silêncio em vez de panicar em `println!`).
///
/// **Onde:** a primeira linha de [`run`]. No-op fora do Unix.
#[cfg(unix)]
fn restaurar_sigpipe() {
    // SAFETY: chamada única, antes de qualquer thread, restaurando o handler padrão do SO.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}
#[cfg(not(unix))]
fn restaurar_sigpipe() {}

/// **O quê:** a CLI inteira: parse de `args` (com o argv0), parasita do market, despacho. Devolve
/// o código de saída (0 ok, 1 erro de comando, o do clap em erro de uso).
///
/// **Onde:** o `main` de cada binário CLI. `plat` e `hooks` são as costuras de plataforma (Unix:
/// `Plataforma::unix()` + `HooksUnix`; Windows: as do repo `schematize_windows_rs`).
///
/// **Parasita depois do parse** (o clap já tratou `--version`/`--help`); os hooks
/// `guard`/`check` ficam de fora dentro do parasita — rodam a cada tool call. Erro NUNCA é
/// engolido (piso 4): vai ao stderr e vira código 1.
pub fn run(args: Vec<String>, plat: &Plataforma, hooks: &dyn Hooks) -> i32 {
    restaurar_sigpipe();
    crate::ambiente::com(plat, hooks, || {
        let cli = match Cli::try_parse_from(args) {
            Ok(c) => c,
            Err(e) => {
                // Mesmo caminho do `Cli::parse()`: help/version em stdout (0), uso errado em stderr (2).
                let _ = e.print();
                return e.exit_code();
            }
        };
        crate::ambiente::garantir_market();
        match comandos::executar(cli.cmd) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("erro: {e}");
                1
            }
        }
    })
}
