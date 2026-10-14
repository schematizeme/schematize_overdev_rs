//! Escrita no checklist em QUALQUER layout (monolito ou particionado) — ADR-0027.
//!
//! Os escritores (`hold`, `park`, `human`, `answer`/`refuse`, `caixa merge`) deixaram de
//! assumir que os itens vivem em `CHECKLIST.md`: aqui se localiza o arquivo que contém o item
//! (índice/monolito e TODAS as partições, inclusive `--completed`), edita-se nele e roda-se o
//! `sync` (sufixo + índice) ainda sob a mesma trava.

use super::estado::{self, Estado, INDICE, PASTA};
use super::indice::sincronizar;
use super::migrar::montar;
use super::nome::Nome;
use crate::overdev::trava::{com_trava, escreve_atomico};
use std::path::Path;

/// Limite de itens ao anexar (mesmo teto do `check`).
const TETO: usize = super::lint::MAX_ITENS;

/// **O quê:** aplica `f` a cada arquivo do checklist (na ordem de leitura: `CHECKLIST.md`, depois
/// `checklist/*.md`) até um devolver `Ok`; grava só esse arquivo, atômico, e roda o `sync`.
/// **Onde:** `hold`, `park`, `human_done`, `resolver` (via `overdev::notas`).
///
/// `f(texto, offset)` recebe o texto do arquivo e quantos itens humanos ABERTOS vieram antes dele,
/// para o alvo posicional (`--done N`, `answer N`) valer no checklist inteiro, como a UI numera.
/// Nenhum casou: devolve o erro do 1º arquivo (mesma mensagem do layout antigo).
pub fn editar<T>(
    od: &Path,
    mut f: impl FnMut(&str, usize) -> Result<(String, T), String>,
) -> Result<T, String> {
    com_trava(&od.join(INDICE), || {
        let arquivos = crate::paths::multidoc_files(od, INDICE, PASTA);
        if arquivos.is_empty() {
            return Err(format!("{}: checklist inexistente", od.join(INDICE).display()));
        }
        let mut primeiro_erro: Option<String> = None;
        let mut offset = 0usize;
        for p in &arquivos {
            let texto = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
            match f(&texto, offset) {
                Ok((novo, t)) => {
                    escreve_atomico(p, &novo)?;
                    if estado::tem_particoes(od) {
                        sincronizar(od)?;
                    }
                    return Ok(t);
                }
                Err(e) => primeiro_erro = primeiro_erro.or(Some(e)),
            }
            offset += Estado::de(&texto).humanos;
        }
        Err(primeiro_erro.unwrap_or_else(|| "nenhum item casou".into()))
    })
}

/// **O quê:** anexa linhas de item ao layout particionado: na partição ABERTA de maior NNN com
/// menos de 50 itens; estourou, cria `p2-<max+1>-demandas[-k]`. Roda o `sync`. **SEM trava** — o
/// chamador (`caixa::mesclar`) já a segura. **Onde:** `caixa::mesclar` quando há partições.
pub fn anexar_itens(od: &Path, itens: &[String]) -> Result<(), String> {
    let l = estado::listar(od)?;
    let mut max_id = l.validas.iter().map(|p| p.nome.id).max().unwrap_or(0);
    // (caminho, texto, total) da partição aberta de maior NNN.
    let mut alvo = l
        .validas
        .iter()
        .filter(|p| !p.nome.concluida)
        .max_by_key(|p| p.nome.id)
        .map(|p| (p.caminho.clone(), std::fs::read_to_string(&p.caminho), p.estado.total()));
    let mut atual: Option<(std::path::PathBuf, String, usize)> = match alvo.take() {
        Some((c, t, n)) => Some((c.clone(), t.map_err(|e| format!("{}: {e}", c.display()))?, n)),
        None => None,
    };
    let mut novas = 0usize;
    for item in itens {
        let cheio = atual.as_ref().is_none_or(|(_, _, n)| *n >= TETO);
        if cheio {
            if let Some((c, t, _)) = atual.take() {
                escreve_atomico(&c, &t)?;
            }
            max_id += 1;
            if max_id > 999 {
                return Err("ids de partição esgotados (NNN > 999)".into());
            }
            novas += 1;
            let slug =
                if novas == 1 { "demandas".to_string() } else { format!("demandas-{novas}") };
            let nm = Nome { prio: 2, id: max_id, slug, concluida: false };
            let caminho = od.join(PASTA).join(nm.formatar());
            atual = Some((caminho, montar(&nm, "Demandas", "Demandas", &[], &[]), 0));
        }
        if let Some((_, t, n)) = atual.as_mut() {
            if !t.ends_with('\n') {
                t.push('\n');
            }
            t.push_str(item);
            t.push('\n');
            *n += Estado::de(item).total().max(1);
        }
    }
    if let Some((c, t, _)) = atual {
        escreve_atomico(&c, &t)?;
    }
    sincronizar(od)?;
    Ok(())
}
