//! `checklist check` (lint) e `checklist proxima` (ADR-0027).

use super::estado::{self, Listagem, INDICE};
use super::secoes::eh_item;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Teto duro de itens por partição.
pub const MAX_ITENS: usize = 50;
/// Piso desejado (abaixo disso, só aviso).
pub const MIN_ITENS: usize = 25;

/// Resultado do lint: erros reprovam (exit != 0); avisos não.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Relatorio {
    /// Violações duras.
    pub erros: Vec<String>,
    /// Avisos.
    pub avisos: Vec<String>,
}

/// **O quê:** erros de integridade: `--completed` com item aberto. **Onde:** [`checar`] e o
/// Stop-hook (mensagem de bloqueio "reabra").
pub fn integridade(l: &Listagem) -> Vec<String> {
    l.validas
        .iter()
        .filter(|p| p.nome.concluida && p.estado.pendentes() > 0)
        .map(|p| {
            format!(
                "{}: marcada --completed mas tem {} item(ns) aberto(s) — reabra: remova o sufixo \
                 ou rode `schematize-overdev checklist sync`",
                p.nome.formatar(),
                p.estado.pendentes()
            )
        })
        .collect()
}

/// **O quê:** o lint completo do diretório `od`. **Onde:** CLI `checklist check` e o gate.
///
/// Erros: nome inválido, >50 itens, `--completed` com item aberto, NNN duplicado, `depende:`
/// para partição inexistente. Avisos: <25 itens, monolito legado >50 itens, `part-*.md` do split.
pub fn checar(od: &Path) -> Result<Relatorio, String> {
    let l = estado::listar(od)?;
    let mut r = Relatorio::default();
    for n in &l.invalidos {
        if n.starts_with("part-") {
            r.avisos.push(format!(
                "{n}: legado do `split` (round-robin); rode `checklist particionar`"
            ));
        } else {
            r.erros.push(format!("{n}: nome inválido (esperado pN-NNN-slug[--completed].md)"));
        }
    }
    let mut por_id: BTreeMap<u16, Vec<String>> = BTreeMap::new();
    for p in &l.validas {
        por_id.entry(p.nome.id).or_default().push(p.nome.formatar());
        let t = p.estado.total();
        if t > MAX_ITENS {
            r.erros.push(format!("{}: {t} itens (máximo {MAX_ITENS})", p.nome.formatar()));
        } else if t < MIN_ITENS {
            r.avisos.push(format!("{}: {t} itens (< {MIN_ITENS})", p.nome.formatar()));
        }
        for d in &p.depende {
            if !l.validas.iter().any(|q| q.nome.id == *d) {
                r.erros.push(format!("{}: depende de {d:03}, que não existe", p.nome.formatar()));
            }
        }
    }
    for (id, nomes) in por_id.iter().filter(|(_, v)| v.len() > 1) {
        r.erros.push(format!("id {id:03} duplicado: {}", nomes.join(", ")));
    }
    r.erros.extend(integridade(&l));
    if let Ok(t) = std::fs::read_to_string(od.join(INDICE)) {
        let n = t.lines().filter(|x| eh_item(x)).count();
        if n > MAX_ITENS {
            r.avisos.push(format!("CHECKLIST.md monolítico com {n} itens (> {MAX_ITENS}): rode `checklist particionar`"));
        }
    }
    Ok(r)
}

/// **O quê:** caminho da próxima partição não-concluída por `(pN, NNN)` cujos `depende:` estão
/// todos concluídos. Se todas as pendentes estiverem bloqueadas (ciclo), devolve a primeira
/// pendente para não travar o run. `None` = nada pendente. **Onde:** CLI `checklist proxima`.
pub fn proxima(od: &Path) -> Result<Option<PathBuf>, String> {
    let l = estado::listar(od)?;
    let feita = |id: u16| l.validas.iter().any(|p| p.nome.id == id && p.nome.concluida);
    let pend: Vec<_> = l.validas.iter().filter(|p| !p.nome.concluida).collect();
    let livre = pend.iter().find(|p| p.depende.iter().all(|d| feita(*d)));
    Ok(livre.or(pend.first()).map(|p| p.caminho.clone()))
}
