//! **Handoff vivo**: o handoff do projeto sempre atualizado, sem perder nada num kill e com
//! ZERO corrupção.
//!
//! **O quê:** duas camadas. (A) a cada 30 s, SEM Claude, monta `context.md` + `checklist.md` a
//! partir de estado determinístico ([`coleta`], [`render`]) e só reescreve se o hash mudou. (B) a
//! cada 30 min, só se o estado mudou, pede notas ao `claude -p` (modelo barato, jaulado, timeout)
//! sobre o que o estado não mostra, valida e REDIGE segredos ([`filtro`]) e anexa como
//! `## Notas do Claude (data/hora)`. Toda escrita é atômica com envelope versionado + checksum e
//! 3 gerações ([`formato`], [`atomico`]); um escritor só ([`trava`]).
//! **Onde:** `overdev handoff-vivo {run|agora|verificar|status}` — ver `INTEGRACAO.md`.

pub mod atomico;
pub mod ciclo;
pub mod claude;
pub mod coleta;
mod comandos;
pub mod config;
pub mod filtro;
pub mod formato;
pub mod laco;
pub mod proc;
pub mod render;
pub mod trava;
pub mod unit;
pub mod verificar;

/// **O quê:** ponto de entrada do CLI: `args` são os argumentos DEPOIS de `handoff-vivo`
/// (`["run", "--mecanico-s", "30"]`). Devolve o código de saída (0 ok, 1 erro, 2 uso).
/// **Onde:** o registro do subcomando no `cli` (ver `INTEGRACAO.md`).
pub fn executar(args: &[String]) -> i32 {
    comandos::executar(args)
}
