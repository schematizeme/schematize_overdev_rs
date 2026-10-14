//! `schematize-overdev checklist particionar|sync|check|proxima` (ADR-0027): a borda de CLI.
//!
//! **Onde:** despachado por `comandos::executar`. O domínio mora em `overdev::particao`; aqui
//! só entrada, saída (texto ou `--json`, escrito à mão como `saidajson`) e o código de saída.

use crate::cli::args::ChecklistCmd;
use crate::cli::saidajson::esc;
use crate::overdev::particao::{self, Opcoes, Relatorio};
use std::path::{Path, PathBuf};

/// **O quê:** o dir do overdev do projeto atual. **Onde:** todos os comandos daqui.
fn od() -> Result<PathBuf, String> {
    let raiz = crate::cli::comandos::cwd_project()?;
    Ok(crate::paths::overdev_dir_at(&raiz))
}

/// **O quê:** lista JSON de strings. **Onde:** os documentos `--json` daqui.
fn lista(v: &[String]) -> String {
    format!("[{}]", v.iter().map(|s| format!("\"{}\"", esc(s))).collect::<Vec<_>>().join(","))
}

/// **O quê:** despacha o subcomando de partição. **Onde:** `comandos::executar`.
pub fn executar(sub: ChecklistCmd) -> Result<(), String> {
    let od = od()?;
    match sub {
        ChecklistCmd::Particionar { max, min, json } => particionar(&od, Opcoes { max, min }, json),
        ChecklistCmd::Sync { json } => sync(&od, json),
        ChecklistCmd::Check { json } => check(&od, json),
        ChecklistCmd::Proxima { json } => proxima(&od, json),
    }
}

/// **O quê:** `checklist particionar`. **Onde:** [`executar`].
fn particionar(od: &Path, op: Opcoes, json: bool) -> Result<(), String> {
    let r = particao::particionar(od, &op)?;
    let nomes: Vec<String> = r
        .criadas
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
        .collect();
    if json {
        let bk = r.backup.as_ref().map(|b| format!("\"{}\"", esc(&b.display().to_string())));
        println!(
            "{{\"itens\":{},\"particoes\":{},\"backup\":{}}}",
            r.itens,
            lista(&nomes),
            bk.unwrap_or_else(|| "null".into())
        );
    } else if nomes.is_empty() {
        println!("nada a particionar (o CHECKLIST.md não tem itens); índice em dia.");
    } else {
        println!("{} itens em {} partição(ões):", r.itens, nomes.len());
        nomes.iter().for_each(|n| println!("  checklist/{n}"));
        if let Some(b) = r.backup {
            println!("backup: {}", b.display());
        }
    }
    Ok(())
}

/// **O quê:** `checklist sync`. **Onde:** [`executar`].
fn sync(od: &Path, json: bool) -> Result<(), String> {
    let m = particao::sync(od)?;
    if json {
        let ren: Vec<String> = m.renomeados.iter().map(|(a, b)| format!("{a} -> {b}")).collect();
        println!(
            "{{\"renomeados\":{},\"indice_atualizado\":{},\"monolito\":{}}}",
            lista(&ren),
            m.indice_atualizado,
            m.monolito
        );
        return Ok(());
    }
    m.renomeados.iter().for_each(|(a, b)| println!("renomeado: {a} -> {b}"));
    if m.monolito {
        println!("CHECKLIST.md ainda tem itens (monolito): rode `checklist particionar`.");
    } else if m.indice_atualizado {
        println!("índice CHECKLIST.md atualizado.");
    } else if m.renomeados.is_empty() {
        println!("em dia: nada a fazer.");
    }
    Ok(())
}

/// **O quê:** `checklist check` — imprime e devolve `Err` se houver erro (exit 1).
/// **Onde:** [`executar`].
fn check(od: &Path, json: bool) -> Result<(), String> {
    let r: Relatorio = particao::checar(od)?;
    if json {
        println!(
            "{{\"ok\":{},\"erros\":{},\"avisos\":{}}}",
            r.erros.is_empty(),
            lista(&r.erros),
            lista(&r.avisos)
        );
    } else {
        r.avisos.iter().for_each(|a| println!("aviso: {a}"));
        r.erros.iter().for_each(|e| println!("ERRO: {e}"));
        if r.erros.is_empty() {
            println!("checklist ok ({} aviso(s)).", r.avisos.len());
        }
    }
    if r.erros.is_empty() {
        Ok(())
    } else {
        Err(format!("{} violação(ões) no checklist particionado", r.erros.len()))
    }
}

/// **O quê:** `checklist proxima`. Sem pendente: stdout vazio (ou `null`), exit 0.
/// **Onde:** [`executar`].
fn proxima(od: &Path, json: bool) -> Result<(), String> {
    let p = particao::proxima(od)?;
    match (p, json) {
        (Some(p), true) => println!("{{\"proxima\":\"{}\"}}", esc(&p.display().to_string())),
        (Some(p), false) => println!("{}", p.display()),
        (None, true) => println!("{{\"proxima\":null}}"),
        (None, false) => eprintln!("nenhuma partição pendente."),
    }
    Ok(())
}
