//! **schematize-overdev-gui** — a janela do `schematize-overdev` (iced, tiny-skia).
//!
//! Casca fina (ADR-0026): injeta a `Plataforma` Unix e os `Hooks` Unix na lib.

use schematize_plataforma::seams::Plataforma;

fn main() -> iced::Result {
    let args: Vec<String> = std::env::args().collect();
    overdev_casa::gui::run(args, &Plataforma::unix(), &overdev_casa::hooks::HooksUnix)
}
