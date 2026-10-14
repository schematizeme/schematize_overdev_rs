//! **schematize-overdev** — o motor que executa um overdev: fila, trava, supervisor e gate.
//!
//! **O quê:** lê e escreve o `CHECKLIST.md`, mantém a trava e o supervisor do run, guarda as
//! conclusões, divide o trabalho em multiagentes, e atende a fila de perguntas (o quiz).
//!
//! **Onde:** o binário `schematize-overdev`, a janela dele, os HOOKS do Claude Code
//! (`overdev guard` a cada chamada de ferramenta, `overdev check` a cada parada) e o hub —
//! que delega a aba de Overdev para cá (E6 do ADR-0018, ADR-0012 F5).
//!
//! ## Por que este foi o último a sair, e o mais arriscado
//!
//! É o MOTOR que executa a própria extração. Os hooks do Claude Code chamam
//! `schematize overdev guard` a cada chamada de ferramenta: uma janela em que o binário
//! instalado não sabe responder isso **bloqueia toda a sessão**. A ordem da migração foi
//! escolhida por causa disso — o app novo instalado e provado ANTES de o hub perder o domínio,
//! nunca o contrário.

pub mod nucleo;

/// Garante o gestor de atualizações (market) na máquina — o "parasita".
pub mod market_parasita;

pub mod agentes;
pub mod agentrun;
pub mod agents;
pub mod archive;
pub mod config;
pub mod overdev;
pub mod overdevdb;
pub mod panel;
pub mod paths;
pub mod projects;
pub mod settings;
pub mod usage;
