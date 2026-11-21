//! **Resiliência absoluta** do overdev: se o kernel/OOM/SIGKILL matar o processo, ele retoma
//! exatamente de onde parou, sem perder nem duplicar trabalho.
//!
//! **O quê:** estado durável ([`modelo`]) sobre journal WAL + snapshot da plataforma
//! ([`duravel`]); toda transição é registrada ANTES do efeito (write-ahead, fsync a cada evento);
//! a retomada ([`reconcilia`]) recupera, reconcilia unidades órfãs e re-despacha só o que estava
//! em voo; despacho de unidades sob governador de memória ([`controle`], [`despacho`],
//! [`executor`]).
//! **Onde:** `overdev executar|retomar` (CLI), `supervisor::supervise` (eventos) e o
//! `schematize-vigia` (relança o processo com `--resume`).
//!
//! Diretório de estado: `<control-plane>/estado/` (gitignored), com `estado/vigia/` para o vigia.

pub mod checklist;
pub mod controle;
pub mod despacho;
pub mod duravel;
pub mod execucao;
pub mod executor;
pub mod modelo;
pub mod oom;
pub mod plano;
pub mod proc;
pub mod reconcilia;
pub mod vigilancia;

use std::path::{Path, PathBuf};

/// **O quê:** diretório de estado de `projeto` (`<control-plane>/estado`). **Onde:** CLI,
/// supervisor e testes; segue a regra "ler ambos" de `paths::overdev_dir_at`.
pub fn dir_estado(projeto: &Path) -> PathBuf {
    crate::paths::overdev_dir_at(projeto).join("estado")
}

/// **O quê:** diretório do estado do SUPERVISOR (journal próprio: lock separado do executor).
/// **Onde:** `supervisor::supervise`.
pub fn dir_estado_supervisor(projeto: &Path) -> PathBuf {
    dir_estado(projeto).join("supervisor")
}
