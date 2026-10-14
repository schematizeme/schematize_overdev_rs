//! **schematize-overdev** — o motor de um run: fila, trava, supervisor e gate.
//!
//! Casca fina (ADR-0026): injeta a `Plataforma` Unix e os `Hooks` Unix na lib. O `.exe` do
//! Windows nasce no repo `schematize_windows_rs` com as costuras dele.

use schematize_plataforma::seams::Plataforma;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let code = overdev_casa::cli::run(args, &Plataforma::unix(), &overdev_casa::hooks::HooksUnix);
    std::process::exit(code);
}
