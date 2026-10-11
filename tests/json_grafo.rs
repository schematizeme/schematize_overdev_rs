//! O que o HUB lê por este app — `graph json`, `documentos --json` e as recusas do `terminal`.
//!
//! **Onde:** `cargo test`, e o CI. O hub (`schematize-gui`) perdeu o parser do índice e o leitor
//! de DECISOES/PLAN na E6 (ADR-0018) e agora consome estes documentos; cada nome e tipo abaixo é
//! contrato com ele.
//!
//! **Parser estrito, comando de verdade:** roda o binário e parseia o `stdout`. O lançamento real
//! de terminal NÃO é testado aqui (abriria uma janela); testam-se as recusas, que são o que
//! protege o chamador de um prompt inventado.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_schematize-overdev");

/// **O quê:** projeto temporário limpo (PID + nome: testes em paralelo não disputam).
fn projeto(nome: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("overdev-grafo-{}-{nome}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("criar o projeto de teste");
    d
}

/// **O quê:** roda o binário no projeto (HOME isolado) e devolve a saída crua.
fn rodar(proj: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(proj)
        .env("HOME", proj)
        .output()
        .expect("rodar o binário")
}

/// **O quê:** roda e parseia o `stdout` como JSON; falha com o texto cru.
fn json(proj: &Path, args: &[&str]) -> Value {
    let o = rodar(proj, args);
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    assert!(o.status.success(), "falhou: {}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_str(&out).unwrap_or_else(|e| panic!("JSON inválido: {e}\n{out}"))
}

/// Chaves de um objeto, em ordem alfabética (a ordem não é contrato).
fn chaves(v: &Value) -> Vec<&str> {
    v.as_object().expect("objeto").keys().map(|s| s.as_str()).collect()
}

/// **O quê:** índice com um global (3 serviços) e o detalhe de `api`, com descrição no MAPA.
fn com_indice(proj: &Path) {
    let g = proj.join(".schematize/grafos");
    std::fs::create_dir_all(&g).expect("dir de grafos");
    std::fs::write(g.join("GRAFO_GLOBAL.md"), "front -> api (login)\napi -> auth\n")
        .expect("global");
    std::fs::write(g.join("api.md"), "- `handler` — recebe · api/src/h.rs:7\nhandler -> repo\n")
        .expect("detalhe");
}

/// **`graph json` (global):** nós com `loc` nulo quando desconhecido, arestas com `rotulo`.
#[test]
fn graph_json_global_e_contrato() {
    let p = projeto("global");
    com_indice(&p);
    let v = json(&p, &["graph", "json"]);
    assert_eq!(chaves(&v), ["agregado", "arestas", "descricoes", "nos", "servico"]);
    assert_eq!(v["servico"], Value::Null, "global não tem serviço");
    assert_eq!(v["agregado"], false);
    let ids: Vec<&str> =
        v["nos"].as_array().unwrap().iter().map(|n| n["id"].as_str().unwrap()).collect();
    for esperado in ["front", "api", "auth"] {
        assert!(ids.contains(&esperado), "{esperado} faltou em {ids:?}");
    }
    assert!(!ids.contains(&"handler"), "o detalhe do serviço não entra no global: {ids:?}");
    let n = &v["nos"][0];
    assert_eq!(chaves(n), ["id", "loc"]);
    assert_eq!(n["loc"], Value::Null, "loc desconhecido é null, nunca \"\"");
    let login =
        v["arestas"].as_array().unwrap().iter().find(|e| e["de"] == "front").expect("aresta front");
    assert_eq!(chaves(login), ["de", "para", "rotulo"]);
    assert_eq!((login["para"].as_str(), login["rotulo"].as_str()), (Some("api"), Some("login")));
    let sem_rotulo =
        v["arestas"].as_array().unwrap().iter().find(|e| e["de"] == "api").expect("aresta api");
    assert_eq!(sem_rotulo["rotulo"], Value::Null);
}

/// **`graph json --servico`:** o detalhe do serviço, com `loc` preenchido e o nome do serviço.
#[test]
fn graph_json_servico_traz_o_detalhe() {
    let p = projeto("servico");
    com_indice(&p);
    let v = json(&p, &["graph", "json", "--servico", "api"]);
    assert_eq!(v["servico"], "api");
    let h = v["nos"].as_array().unwrap().iter().find(|n| n["id"] == "handler").expect("handler");
    assert_eq!(h["loc"], "api/src/h.rs:7");
    assert!(v["arestas"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["de"] == "handler" && e["para"] == "repo"));
}

/// **Sem índice o grafo é VAZIO, não erro:** «não há grafo aqui» é um estado da tela.
#[test]
fn graph_json_sem_indice_e_vazio() {
    let p = projeto("vazio");
    let v = json(&p, &["graph", "json"]);
    assert!(
        v["nos"].as_array().unwrap().is_empty() && v["arestas"].as_array().unwrap().is_empty(),
        "{v}"
    );
    assert_eq!(v["agregado"], false);
    let s = json(&p, &["graph", "json", "--servico", "nada"]);
    assert!(s["nos"].as_array().unwrap().is_empty(), "serviço inexistente: nós vazios, {s}");
}

/// **`documentos --json`:** as três chaves, sempre string; texto com aspas/quebra volta idêntico.
#[test]
fn documentos_json_e_contrato() {
    let p = projeto("docs");
    let v = json(&p, &["documentos", "--json"]);
    assert_eq!(chaves(&v), ["decisoes", "perguntas", "plano"]);
    assert_eq!(
        (v["decisoes"].as_str(), v["plano"].as_str(), v["perguntas"].as_str()),
        (Some(""), Some(""), Some(""))
    );

    let od = p.join(".schematize/overdev");
    std::fs::create_dir_all(&od).expect("dir overdev");
    std::fs::write(od.join("DECISOES.md"), "## RESPOSTA: x \"y\"\n").expect("decisoes");
    std::fs::write(od.join("PLAN.md"), "# plano\nlinha 2\n").expect("plan");
    std::fs::write(p.join("PERGUNTAS-OVERDEV.txt"), "pergunta?\n").expect("perguntas");
    let v = json(&p, &["documentos", "--json"]);
    assert_eq!(v["decisoes"], "## RESPOSTA: x \"y\"\n");
    assert_eq!(v["plano"], "# plano\nlinha 2\n");
    assert_eq!(v["perguntas"], "pergunta?\n");
}

/// **`terminal` recusa o que não sabe lançar** — sem abrir janela e sem inventar prompt.
#[test]
fn terminal_recusa_receita_desconhecida_prompt_vazio_e_a_mistura() {
    let p = projeto("term");
    let o = rodar(&p, &["terminal", "--receita", "nada"]);
    assert!(!o.status.success());
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("não conheço a receita `nada`") && err.contains("archive-todos"), "{err}");

    let o = rodar(&p, &["terminal", "--prompt", "  "]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("--prompt está vazio"));

    let o = rodar(&p, &["terminal", "--prompt", "x", "--receita", "reindex"]);
    assert!(!o.status.success(), "prompt + receita juntos é ambíguo");
}
