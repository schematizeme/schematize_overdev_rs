//! O `--json` que a janela usa para EXECUÇÃO, CAIXA, NOTAS e ARQUIVOS — contrato com a janela.
//!
//! **Onde:** `cargo test`, e o CI. Irmão de `json_e_contrato.rs` (que trava progresso, fila e
//! checklist); este cobre os comandos que a janela ganhou ao absorver o que só existia na aba
//! Overdev do hub: `caixa list --json`, `monitor --json`, `tokens --json`, `arquivos --json`,
//! `notas --json`, e as ações (`human --done`, `answer`, `refuse`, `note`) com o TEXTO que a
//! janela de fato manda — inclusive texto que começa com `-`.
//!
//! **Parser estrito, comando de verdade:** roda o binário e parseia o `stdout`.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_schematize-overdev");

/// **O quê:** projeto temporário limpo (PID + nome: testes em paralelo não disputam).
fn projeto(nome: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("overdev-exec-{}-{nome}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("criar o projeto de teste");
    d
}

/// **O quê:** roda o binário no projeto (HOME isolado no projeto) e devolve a saída crua.
fn rodar(proj: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(proj)
        .env("HOME", proj)
        .env("SCHEMATIZE_SEM_MARKET", "1")
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

/// **O quê:** run ativo com 2 itens humanos abertos, 1 máquina aberto, 1 feito.
fn com_run(proj: &Path) {
    let d = proj.join(".schematize/overdev");
    std::fs::create_dir_all(&d).expect("dir overdev");
    std::fs::write(
        d.join("state.json"),
        r#"{"mode":"active","max_iters":40,"objetivo":"meta","started":1}"#,
    )
    .expect("state");
    std::fs::write(
        d.join("CHECKLIST.md"),
        "- [x] feito um\n- [ ] aberto dois\n- [H ] decidir A\n- [H ] decidir B\n",
    )
    .expect("checklist");
    std::fs::write(d.join("PLAN.md"), "# plano\n").expect("plan");
}

/// **`caixa list --json`:** pendentes (texto cru) e processadas (com itens); vazia é `[]`.
#[test]
fn caixa_list_json_e_contrato() {
    let p = projeto("caixa");
    let v = json(&p, &["caixa", "list", "--json"]);
    assert_eq!(chaves(&v), ["pendentes", "processadas"]);
    assert!(v["pendentes"].as_array().unwrap().is_empty(), "caixa vazia é []: {v}");

    assert!(rodar(&p, &["add", "uma demanda com \"aspas\"\ne quebra"]).status.success());
    let v = json(&p, &["caixa", "list", "--json"]);
    let e = &v["pendentes"][0];
    assert_eq!(chaves(e), ["id", "itens", "texto", "ts"]);
    assert!(e["id"].is_string() && e["ts"].is_u64() && e["itens"].as_array().unwrap().is_empty());
    assert_eq!(e["texto"], "uma demanda com \"aspas\"\ne quebra", "texto cru, volta idêntico");

    let id = e["id"].as_str().unwrap().to_string();
    let o = rodar(&p, &["caixa", "organizar", &id, "--item", "item um", "--item", "item dois"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v = json(&p, &["caixa", "list", "--json"]);
    assert!(v["pendentes"].as_array().unwrap().is_empty());
    assert_eq!(v["processadas"][0]["itens"], serde_json::json!(["item um", "item dois"]));
    let _ = std::fs::remove_dir_all(&p);
}

/// **`caixa list` sem `--json` continua sendo a tela humana** (a flag não troca o padrão).
#[test]
fn caixa_list_sem_json_continua_humano() {
    let p = projeto("caixa-humano");
    let o = rodar(&p, &["caixa", "list"]);
    assert!(o.status.success());
    assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "caixa vazia.");
    let _ = std::fs::remove_dir_all(&p);
}

/// **`monitor --json`:** itens de máquina abertos e o log de conclusões; sem run, listas vazias.
#[test]
fn monitor_json_e_contrato() {
    let p = projeto("monitor");
    let v = json(&p, &["monitor", "--json"]);
    assert_eq!(chaves(&v), ["abertos", "conclusoes"]);
    assert!(
        v["abertos"].as_array().unwrap().is_empty()
            && v["conclusoes"].as_array().unwrap().is_empty()
    );

    com_run(&p);
    std::fs::write(p.join(".schematize/overdev/completions.json"), r#"{"feito um":1790000000}"#)
        .ok();
    let v = json(&p, &["monitor", "--json"]);
    assert_eq!(v["abertos"], serde_json::json!(["aberto dois"]));
    // Quem lê o `completions.json` é o domínio; o contrato é só o shape.
    for c in v["conclusoes"].as_array().unwrap() {
        assert_eq!(chaves(c), ["em", "texto"]);
        assert!(c["em"].is_i64() && c["texto"].is_string());
    }
    let _ = std::fs::remove_dir_all(&p);
}

/// **`tokens --json`:** sem transcript é ZERO com `modelo: null` (não é erro).
#[test]
fn tokens_json_e_contrato() {
    let p = projeto("tokens");
    let v = json(&p, &["tokens", "--json"]);
    assert_eq!(
        chaves(&v),
        ["cache_escrita", "cache_leitura", "entrada", "mensagens", "modelo", "saida", "total"]
    );
    for k in ["cache_escrita", "cache_leitura", "entrada", "mensagens", "saida", "total"] {
        assert_eq!(v[k], 0, "`{k}` sem transcript é 0");
    }
    assert!(v["modelo"].is_null(), "sem mensagem não há modelo: {v}");
    let _ = std::fs::remove_dir_all(&p);
}

/// **`arquivos --json`:** os três documentos editáveis, com caminho ABSOLUTO e se existem.
#[test]
fn arquivos_json_e_contrato() {
    let p = projeto("arquivos");
    com_run(&p);
    let v = json(&p, &["arquivos", "--json"]);
    assert_eq!(chaves(&v), ["arquivos", "dir"]);
    let nomes: Vec<&str> =
        v["arquivos"].as_array().unwrap().iter().map(|a| a["nome"].as_str().unwrap()).collect();
    assert_eq!(nomes, ["PLAN.md", "CHECKLIST.md", "NOTAS.md"]);
    let plan = &v["arquivos"][0];
    assert_eq!(chaves(plan), ["caminho", "existe", "nome"]);
    assert_eq!(plan["existe"], true);
    assert!(Path::new(plan["caminho"].as_str().unwrap()).is_absolute());
    assert!(plan["caminho"].as_str().unwrap().ends_with("/.schematize/overdev/PLAN.md"));
    assert_eq!(v["arquivos"][2]["existe"], false, "NOTAS.md ainda não existe");
    let _ = std::fs::remove_dir_all(&p);
}

/// **`note` + `notas --json`:** a nota entra pelo CLI (a janela não escreve) e volta pela leitura;
/// texto que COMEÇA com `-` passa quando precedido de `--`, como a janela envia.
#[test]
fn nota_entra_pelo_cli_e_volta_por_notas_json() {
    let p = projeto("notas");
    com_run(&p);
    assert_eq!(json(&p, &["notas", "--json"])["texto"], "", "sem notas é string vazia");
    let o = rodar(&p, &["note", "--kind", "task", "--", "-corrija o botão"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let o = rodar(&p, &["note", "--kind", "correcao", "--", "não foi isso"]);
    assert!(o.status.success());
    let t = json(&p, &["notas", "--json"])["texto"].as_str().unwrap().to_string();
    assert!(t.contains("PONTO POR TASK") && t.contains("-corrija o botão"), "{t}");
    assert!(t.contains("não foi isso"), "{t}");
    let _ = std::fs::remove_dir_all(&p);
}

/// **As ações de item humano com os args EXATOS da janela:** `human --done N`, `answer N -- txt`,
/// `refuse N -- txt`. O índice é o `humano_n` do `checklist --json`.
#[test]
fn acoes_de_item_humano_com_os_args_da_janela() {
    let p = projeto("acoes");
    com_run(&p);
    let c = json(&p, &["checklist", "--json"]);
    let ns: Vec<u64> = c["itens"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["tipo"] == "hopen")
        .map(|i| i["humano_n"].as_u64().unwrap())
        .collect();
    assert_eq!(ns, [1, 2], "os humanos abertos são numerados a partir de 1");

    let o = rodar(&p, &["human", "--done", "1"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    // O 1º saiu da lista de abertos: o antigo 2º agora é o 1º.
    let c = json(&p, &["checklist", "--json"]);
    let abertos: Vec<&str> = c["itens"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["tipo"] == "hopen")
        .map(|i| i["texto"].as_str().unwrap())
        .collect();
    assert_eq!(abertos, ["decidir B"]);

    let o = rodar(&p, &["refuse", "1", "--", "-não se aplica"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let c = json(&p, &["checklist", "--json"]);
    assert!(c["itens"].as_array().unwrap().iter().any(|i| i["tipo"] == "hrec"), "{c}");

    // Sem item humano aberto o CLI RECUSA com motivo (a janela repassa a mensagem).
    let o = rodar(&p, &["answer", "1", "--", "tarde demais"]);
    assert!(!o.status.success(), "answer num índice inexistente deve falhar");
    assert!(!String::from_utf8_lossy(&o.stderr).trim().is_empty(), "o motivo vai no stderr");
    let _ = std::fs::remove_dir_all(&p);
}

/// **`agents --json` (já existia) tem os campos que a coluna Sistema lê.** Travar aqui evita a
/// janela ler um nome que o CLI renomeou.
#[test]
fn agents_json_tem_os_campos_da_coluna_sistema() {
    let p = projeto("agents");
    let v = json(&p, &["agents", "--json", "--split", "2"]);
    for k in ["total_cap", "available", "running_claudes", "load1"] {
        assert!(v[k].is_number(), "`{k}` some do agents --json: {v}");
    }
    assert!(v["split"]["subagents_each"].is_u64() && v["split"]["mains"].is_u64(), "{v}");
    let _ = std::fs::remove_dir_all(&p);
}
