//! **schematize-overdev** — o motor de um run: fila, trava, supervisor e gate.
//!
//! **Onde:** o ícone do app, a janela (por `--json`), a mão de quem digita, e os HOOKS do
//! Claude Code.
//!
//! ## Este binário é chamado a CADA chamada de ferramenta
//!
//! O hook `PreToolUse` roda `overdev guard` antes de toda ferramenta da sessão, e o `Stop`
//! roda `overdev check` a cada parada. Isso põe dois requisitos que nenhum outro app da casa
//! tem: ele precisa **subir rápido** e **nunca sair com código errado por acidente** — um
//! `guard` que sai não-zero por um motivo que não seja o veto bloqueia a sessão inteira.

mod cli;

use clap::Parser;
use cli::args::Cli;

/// **O quê:** devolve o `SIGPIPE` ao padrão do Unix.
///
/// **Onde:** a primeira linha de `main`.
#[cfg(unix)]
fn restaurar_sigpipe() {
    // SAFETY: chamada única, antes de qualquer thread, restaurando o handler padrão do SO.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}
#[cfg(not(unix))]
fn restaurar_sigpipe() {}

fn main() {
    restaurar_sigpipe();
    let cli = Cli::parse();
    // Parasita do market: depois do parse (o clap já tratou --version/--help). Os hooks
    // `guard`/`check` ficam de fora dentro do módulo — rodam a cada tool call.
    overdev_casa::market_parasita::garantir();
    if let Err(e) = cli::comandos::executar(cli.cmd) {
        // Piso 4: erro NUNCA é engolido.
        eprintln!("erro: {e}");
        std::process::exit(1);
    }
}
