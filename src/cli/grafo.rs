//! O que o HUB lê/dispara por este app: o grafo do índice, os documentos do run e o terminal com tarefa.
//!
//! **O quê:** `graph json [--servico X]`, `documentos --json` e `terminal --prompt|--receita`.
//!
//! **Onde:** a aba Grafo e a aba Overdev do hub (`schematize-gui`). Despachados por
//! `comandos::executar`. Existem porque o hub deixou de ter o domínio do overdev (E6, ADR-0018):
//! o parser do `GRAFO_GLOBAL.md` e dos `<servico>.md`, o leitor de `DECISOES/PLAN` e o lançador do
//! `claude` moram neste app, e o hub passa a PERGUNTAR em vez de reimplementar.
//!
//! ## Escrito à mão, como `saidajson`
//!
//! O documento é CONTRATO com outro processo; `tests/json_grafo.rs` fixa cada nome e tipo.

use crate::agentrun;
use crate::cli::saidajson::{esc, texto_ou_null};
use crate::panel::{self, Edge, Node};
use std::collections::HashMap;
use std::path::Path;

/// **O quê:** o documento `graph json` — função PURA, para o teste travar o shape.
///
/// **Onde:** [`grafo_json`]. `loc` e `rotulo` ausentes são `null`, nunca `""`. `descricoes` é o
/// mapa nó -> «O quê» do índice (só os nós que têm descrição). Ordem das chaves fixa.
pub fn documento_grafo(
    servico: Option<&str>,
    agregado: bool,
    nos: &[Node],
    arestas: &[Edge],
    descricoes: &HashMap<String, String>,
) -> String {
    let n = nos
        .iter()
        .map(|n| {
            format!(
                "{{\"id\":\"{}\",\"loc\":{}}}",
                esc(&n.id),
                texto_ou_null(n.loc.as_deref().unwrap_or(""))
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let a = arestas
        .iter()
        .map(|e| {
            format!(
                "{{\"de\":\"{}\",\"para\":\"{}\",\"rotulo\":{}}}",
                esc(&e.from),
                esc(&e.to),
                texto_ou_null(e.label.as_deref().unwrap_or(""))
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let mut ds: Vec<(&String, &String)> = descricoes.iter().collect();
    ds.sort();
    let d = ds
        .iter()
        .map(|(k, v)| format!("\"{}\":\"{}\"", esc(k), esc(v)))
        .collect::<Vec<_>>()
        .join(",");
    let serv = servico.map_or("null".to_string(), |s| format!("\"{}\"", esc(s)));
    format!(
        "{{\"servico\":{serv},\"agregado\":{agregado},\"nos\":[{n}],\"arestas\":[{a}],\"descricoes\":{{{d}}}}}"
    )
}

/// **O quê:** imprime o grafo do projeto atual como JSON. **Onde:** `graph json`.
///
/// Sem `--servico`: a visão GLOBAL (paginada; agrega por serviço quando passa do teto). Com ele:
/// o detalhe daquele serviço. Índice ausente é um grafo VAZIO (`nos: []`), não um erro: «não há
/// grafo aqui» é um estado da tela.
pub fn grafo_json(servico: Option<&str>) -> Result<(), String> {
    let raiz = crate::cli::comandos::cwd_project()?;
    let (nos, arestas, agregado) = match servico {
        Some(s) => {
            let (n, e) = panel::load_service_graph(&raiz, s);
            (n, e, false)
        }
        None => {
            let (n, e, _idx, agg) = panel::load_graph_global(&raiz);
            (n, e, agg)
        }
    };
    let desc = panel::node_descriptions(&raiz);
    println!("{}", documento_grafo(servico, agregado, &nos, &arestas, &desc));
    Ok(())
}

/// **O quê:** o documento `documentos --json` — decisões, plano e perguntas parkeadas.
///
/// **Onde:** [`documentos`]. Função PURA. Sempre strings (vazio é `""`): é o texto que o hub
/// mostra dobrado, e «sem decisões» não é diferente de «decisões vazias» para a tela.
pub fn documento_documentos(decisoes: &str, plano: &str, perguntas: &str) -> String {
    format!(
        "{{\"decisoes\":\"{}\",\"plano\":\"{}\",\"perguntas\":\"{}\"}}",
        esc(decisoes),
        esc(plano),
        esc(perguntas)
    )
}

/// **O quê:** imprime DECISOES, PLAN e PERGUNTAS-OVERDEV do projeto atual.
///
/// **Onde:** `documentos [--json]`. Sem `--json` imprime o texto cru com um cabeçalho por seção.
pub fn documentos(json: bool) -> Result<(), String> {
    let raiz = crate::cli::comandos::cwd_project()?;
    let o = panel::load_overdev(&raiz);
    if json {
        println!("{}", documento_documentos(&o.decisoes, &o.plano, &o.perguntas));
    } else {
        for (t, c) in [("DECISOES", &o.decisoes), ("PLAN", &o.plano), ("PERGUNTAS", &o.perguntas)] {
            println!("== {t} ==\n{c}");
        }
    }
    Ok(())
}

/// **O quê:** o prompt de uma receita nomeada, ou o erro com as que existem.
///
/// **Onde:** [`terminal`]. `overdev` usa o objetivo do run em `raiz` (vazio se não houver).
fn prompt_da_receita(nome: &str, raiz: &Path) -> Result<String, String> {
    match nome {
        "archive-todos" => Ok(agentrun::archive_todos_prompt()),
        "reindex" => Ok(agentrun::reindex_prompt()),
        "overdev" => Ok(agentrun::overdev_prompt_em(
            raiz,
            &crate::overdev::progresso::objetivo_at(raiz).unwrap_or_default(),
        )),
        outra => Err(format!(
            "não conheço a receita `{outra}`. As que existem: archive-todos, reindex, overdev"
        )),
    }
}

/// **O quê:** abre um terminal no projeto atual — vazio, com um prompt livre ou com uma receita.
///
/// **Onde:** `terminal [--prompt T | --receita R] [--json]`. Sem nenhum, é o terminal interativo de
/// sempre. Com `--receita overdev` sobe também o supervisor (a rede que cobre o agente morrer).
/// Com `--json` imprime `{"terminal":"<nome>"}`: o hub mostra QUAL terminal abriu.
pub fn terminal(prompt: Option<&str>, receita: Option<&str>, json: bool) -> Result<(), String> {
    let raiz = crate::cli::comandos::cwd_project()?;
    let nome = match (prompt, receita) {
        (Some(p), _) if p.trim().is_empty() => return Err("o --prompt está vazio".into()),
        (Some(p), _) => agentrun::launch_prompt_in_terminal(&raiz, p)?,
        (None, Some("overdev")) => {
            let obj = crate::overdev::progresso::objetivo_at(&raiz).unwrap_or_default();
            agentrun::launch_in_terminal(&raiz, &obj)?
        }
        (None, Some(r)) => {
            agentrun::launch_prompt_in_terminal(&raiz, &prompt_da_receita(r, &raiz)?)?
        }
        (None, None) => agentrun::abrir_terminal_no_projeto(&raiz)?,
    };
    if json {
        println!("{{\"terminal\":\"{}\"}}", esc(&nome));
    } else {
        println!(
            "terminal `{nome}` aberto em {} — o claude sobe com o bypass ligado.",
            raiz.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A receita desconhecida é recusada com a lista das que existem (nada de prompt inventado).
    #[test]
    fn receita_desconhecida_lista_as_validas() {
        let e = prompt_da_receita("nada", Path::new("/x")).expect_err("deve recusar");
        assert!(
            e.contains("archive-todos") && e.contains("reindex") && e.contains("overdev"),
            "{e}"
        );
        assert!(prompt_da_receita("reindex", Path::new("/x")).expect("reindex").contains("GRAFO"));
    }

    /// Documento de documentos: escapa aspas e quebra de linha, e as três chaves são fixas.
    #[test]
    fn documentos_escapa_e_tem_tres_chaves() {
        let d = documento_documentos("a\"b", "l1\nl2", "");
        let v: serde_json::Value = serde_json::from_str(&d).expect("json válido");
        assert_eq!(v["decisoes"], "a\"b");
        assert_eq!(v["plano"], "l1\nl2");
        assert_eq!(v["perguntas"], "");
    }
}
