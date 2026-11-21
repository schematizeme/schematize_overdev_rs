//! Parse dos argumentos e despacho de `handoff-vivo {run|agora|verificar|status}`.
//!
//! **O quê:** [`executar`] recebe os argumentos DEPOIS do nome do subcomando e devolve o código
//! de saída (0 ok, 1 erro, 2 uso). Flags: `--mecanico-s N`, `--claude-s N`, `--sem-claude`,
//! `--modelo M`, `--projeto DIR`, `--reparar` (só `verificar`).
//! **Onde:** [`super::executar`] (ponto de integração do CLI).

use super::ciclo::{relogio_real, Motor};
use super::config::Config;
use super::{laco, verificar};
use std::path::PathBuf;

const USO: &str =
    "uso: overdev handoff-vivo <run|agora|verificar|status> [--mecanico-s N] [--claude-s N] \
[--sem-claude] [--modelo M] [--projeto DIR] [--reparar]";

/// Opções já lidas.
struct Opcoes {
    cfg: Config,
    reparar: bool,
}

/// **O quê:** o valor da flag `nome` (próximo argumento). **Onde:** [`ler_opcoes`].
fn valor<'a>(it: &mut std::slice::Iter<'a, String>, nome: &str) -> Result<&'a String, String> {
    it.next().ok_or_else(|| format!("{nome} exige um valor"))
}

/// **O quê:** inteiro positivo de uma flag. **Onde:** [`ler_opcoes`].
fn numero(s: &str, nome: &str) -> Result<u64, String> {
    s.parse::<u64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| format!("{nome}: esperava inteiro > 0, recebi `{s}`"))
}

/// **O quê:** lê as flags. **Onde:** [`executar`].
fn ler_opcoes(args: &[String]) -> Result<Opcoes, String> {
    let mut projeto: Option<PathBuf> = None;
    let (mut mec, mut cla, mut sem, mut modelo, mut reparar) = (None, None, false, None, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--mecanico-s" => mec = Some(numero(valor(&mut it, a)?, a)?),
            "--claude-s" => cla = Some(numero(valor(&mut it, a)?, a)?),
            "--sem-claude" => sem = true,
            "--reparar" => reparar = true,
            "--modelo" => modelo = Some(valor(&mut it, a)?.clone()),
            "--projeto" => projeto = Some(PathBuf::from(valor(&mut it, a)?)),
            outro => return Err(format!("argumento desconhecido `{outro}`")),
        }
    }
    let inicio = match projeto {
        Some(p) => p,
        None => std::env::current_dir().map_err(|e| format!("cwd: {e}"))?,
    };
    let mut cfg = Config::detectar(&inicio);
    cfg.mecanico_s = mec.unwrap_or(cfg.mecanico_s);
    cfg.claude_s = cla.unwrap_or(cfg.claude_s);
    cfg.sem_claude = sem;
    if let Some(m) = modelo {
        cfg.modelo = m;
    }
    Ok(Opcoes { cfg, reparar })
}

/// **O quê:** executa o subcomando. **Onde:** CLI. **Efeitos:** conforme o subcomando.
pub fn executar(args: &[String]) -> i32 {
    let Some((sub, resto)) = args.split_first() else {
        eprintln!("{USO}");
        return 2;
    };
    let op = match ler_opcoes(resto) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("erro: {e}\n{USO}");
            return 2;
        }
    };
    match sub.as_str() {
        "run" => laco::rodar(op.cfg),
        "agora" => agora(op.cfg),
        "verificar" => match verificar::verificar(&op.cfg, op.reparar) {
            Ok(r) => {
                print!("{}", r.texto);
                i32::from(!r.ok)
            }
            Err(e) => {
                eprintln!("erro: {e}");
                1
            }
        },
        "status" => {
            print!("{}", verificar::status(&op.cfg));
            0
        }
        outro => {
            eprintln!("subcomando desconhecido `{outro}`\n{USO}");
            2
        }
    }
}

/// **O quê:** um ciclo imediato. **Onde:** `handoff-vivo agora`.
fn agora(cfg: Config) -> i32 {
    laco::instalar_sinais();
    match Motor::abrir(cfg, relogio_real()).and_then(|mut m| m.agora()) {
        Ok(t) => {
            println!(
                "handoff gravado: context seq={} checklist seq={}",
                t.seq_context, t.seq_checklist
            );
            0
        }
        Err(e) => {
            eprintln!("erro: {e}");
            1
        }
    }
}
