//! O `--json` é CONTRATO com a janela e o hub — e este arquivo é quem o trava.
//!
//! **Onde:** `cargo test`, e o CI.
//!
//! **Por que de integração:** o contrato é o que SAI do binário. Chamar a função de formatação
//! provaria a função; aqui se roda o comando e se PARSEIA o `stdout` com um parser estrito, que
//! é o que o leitor de verdade faz (um teste que casasse substring aprovaria JSON quebrado).
//!
//! **Diretórios de teste moram no `target/` do repo** (`CARGO_TARGET_TMPDIR`), nunca em `/tmp`.

use overdev_casa::overdev::pergunta::{self, Kind, Opcao, Pergunta};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_schematize-overdev");

/// **O quê:** um projeto temporário limpo, isolado por teste.
///
/// **Onde:** todos os testes daqui. PID + nome no caminho: dois testes em paralelo não disputam.
fn projeto(nome: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("overdev-json-{}-{nome}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("criar o projeto de teste");
    d
}

/// **O quê:** roda o binário no projeto e devolve o `stdout` já parseado.
///
/// **Onde:** os testes. Falha do comando ou JSON inválido derruba o teste com o texto cru.
fn rodar_json(proj: &Path, args: &[&str]) -> Value {
    let o = Command::new(BIN)
        .args(args)
        .current_dir(proj)
        .env("HOME", proj)
        .env("SCHEMATIZE_SEM_MARKET", "1")
        .output()
        .expect("rodar o binário");
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    assert!(o.status.success(), "falhou: {}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_str(&out).unwrap_or_else(|e| panic!("JSON inválido: {e}\n{out}"))
}

/// **O quê:** os nomes de chave de um objeto, em ordem alfabética (ordem não é contrato).
///
/// **Onde:** as asserções de shape.
fn chaves(v: &Value) -> Vec<&str> {
    v.as_object().expect("objeto").keys().map(|s| s.as_str()).collect()
}

/// **O quê:** monta `state.json` + `CHECKLIST.md` de um run ativo.
///
/// **Onde:** o teste do `progresso` com run.
fn com_run(proj: &Path) {
    let d = proj.join(".schematize/overdev");
    std::fs::create_dir_all(&d).expect("dir overdev");
    std::fs::write(
        d.join("state.json"),
        r#"{"mode":"active","max_iters":40,"objetivo":"x","started":1}"#,
    )
    .expect("state");
    std::fs::write(
        d.join("CHECKLIST.md"),
        "- [x] a\n- [x] b\n- [ ] c\n- [~] d\n- [H ] e\n- [H -] f\n",
    )
    .expect("checklist");
    std::fs::write(d.join("iterations"), "7").expect("iterations");
}

/// **O CONTRATO do `progresso --json`, com run:** cada nome e cada TIPO.
#[test]
fn o_shape_do_progresso_e_contrato() {
    let p = projeto("progresso");
    com_run(&p);
    let v = rodar_json(&p, &["progresso", "--json"]);
    assert_eq!(
        chaves(&v),
        [
            "abertos",
            "feitos",
            "humanos",
            "iteracoes",
            "modo",
            "on_hold",
            "raiz",
            "recusados",
            "teto"
        ]
    );
    assert!(v["raiz"].is_string(), "{v}");
    assert_eq!(v["modo"], "active");
    for k in ["feitos", "abertos", "on_hold", "humanos", "recusados", "iteracoes", "teto"] {
        assert!(v[k].is_u64(), "`{k}` é número inteiro não negativo: {v}");
    }
    assert_eq!(
        (v["feitos"].as_u64(), v["abertos"].as_u64(), v["on_hold"].as_u64()),
        (Some(2), Some(1), Some(1))
    );
    assert_eq!((v["humanos"].as_u64(), v["recusados"].as_u64()), (Some(1), Some(1)));
    assert_eq!((v["iteracoes"].as_u64(), v["teto"].as_u64()), (Some(7), Some(40)));
    let _ = std::fs::remove_dir_all(&p);
}

/// **Sem run, o ausente é `null` e NUNCA `""`** — `modo` e `teto`; os contadores seguem zero.
#[test]
fn progresso_sem_run_tem_null_e_nao_string_vazia() {
    let p = projeto("progresso-vazio");
    let v = rodar_json(&p, &["progresso", "--json"]);
    assert!(v["modo"].is_null(), "sem run `modo` é null: {v}");
    assert!(v["teto"].is_null(), "sem run `teto` é null: {v}");
    assert_eq!(v["abertos"], 0);
    for (k, x) in v.as_object().expect("objeto") {
        assert_ne!(x, "", "`{k}` nunca é string vazia");
    }
    let _ = std::fs::remove_dir_all(&p);
}

/// **O quê:** uma pergunta de teste; `titulo` e `contexto` configuráveis.
///
/// **Onde:** os testes da fila.
fn pergunta_de(id: &str, kind: Kind, titulo: &str, contexto: &str) -> Pergunta {
    let escolha = matches!(kind, Kind::Escolha | Kind::Multipla);
    Pergunta {
        id: id.into(),
        criada_em: 1_790_000_000,
        fase: String::new(),
        titulo: titulo.into(),
        contexto: contexto.into(),
        kind,
        opcoes: if escolha {
            vec![
                Opcao { id: "a".into(), rotulo: "A".into(), detalhe: String::new() },
                Opcao { id: "b".into(), rotulo: "B".into(), detalhe: "porque sim".into() },
            ]
        } else {
            vec![]
        },
        auto: String::new(),
        linka: vec![],
        resposta: None,
    }
}

/// **O CONTRATO do `fila --json`:** grupos, nomes de campo, tipos, `estado` e `null`.
#[test]
fn o_shape_da_fila_e_contrato() {
    let p = projeto("fila");
    let q = pergunta_de("q-1", Kind::Escolha, "Tela própria ou lista?", "contexto longo");
    pergunta::gravar(&p, &q).expect("gravar");
    let v = rodar_json(&p, &["fila", "--json"]);
    assert_eq!(chaves(&v), ["abertas", "aguardando_revisao", "ilegiveis", "revisadas"]);
    let a = &v["abertas"][0];
    assert_eq!(
        chaves(a),
        [
            "auto",
            "contexto",
            "criada_em",
            "estado",
            "fase",
            "id",
            "kind",
            "linka",
            "opcoes",
            "resposta",
            "titulo"
        ]
    );
    assert_eq!((a["id"].as_str(), a["estado"].as_str()), (Some("q-1"), Some("aberta")));
    assert_eq!(a["kind"], "escolha", "kind é slug, não traduzido");
    assert_eq!(a["titulo"], "Tela própria ou lista?");
    assert_eq!(a["contexto"], "contexto longo");
    assert!(a["criada_em"].is_u64() && a["linka"].is_array());
    // Ausente é null, nunca "".
    for k in ["fase", "auto", "resposta"] {
        assert!(a[k].is_null(), "`{k}` ausente é null: {a}");
    }
    assert_eq!(chaves(&a["opcoes"][0]), ["detalhe", "id", "rotulo"]);
    assert!(a["opcoes"][0]["detalhe"].is_null(), "detalhe vazio é null");
    assert_eq!(a["opcoes"][1]["detalhe"], "porque sim");
    assert!(v["aguardando_revisao"].as_array().expect("lista").is_empty());
    let _ = std::fs::remove_dir_all(&p);
}

/// **`questions` segue funcionando, com o MESMO documento** (apelido, não segunda implementação).
#[test]
fn questions_e_apelido_de_fila() {
    let p = projeto("apelido");
    pergunta::gravar(&p, &pergunta_de("q-1", Kind::Livre, "Como?", "")).expect("gravar");
    assert_eq!(rodar_json(&p, &["fila", "--json"]), rodar_json(&p, &["questions", "--json"]));
    let _ = std::fs::remove_dir_all(&p);
}

/// **Texto hostil sai como JSON VÁLIDO e volta idêntico:** aspas, quebra de linha, barra
/// invertida, unicode, caractere de controle e byte nulo.
#[test]
fn texto_hostil_na_fila_e_json_valido() {
    let p = projeto("hostil");
    let hostil =
        "aspas \" e \\ barra\nlinha\r\ttab \u{0001}\u{001f} nulo\u{0000} \u{1F600} é \u{2028}";
    let mut q = pergunta_de("q-h", Kind::Escolha, hostil, hostil);
    q.opcoes[0].rotulo = hostil.into();
    q.opcoes[0].detalhe = hostil.into();
    q.linka = vec![hostil.into()];
    pergunta::gravar(&p, &q).expect("gravar");
    let v = rodar_json(&p, &["fila", "--json"]);
    let a = &v["abertas"][0];
    assert_eq!(a["titulo"], hostil);
    assert_eq!(a["contexto"], hostil);
    assert_eq!(a["opcoes"][0]["rotulo"], hostil);
    assert_eq!(a["opcoes"][0]["detalhe"], hostil);
    assert_eq!(a["linka"][0], hostil);
    let _ = std::fs::remove_dir_all(&p);
}

/// **O CONTRATO do `checklist --json`:** chaves, tipos, classificação e o texto hostil.
#[test]
fn o_shape_do_checklist_e_contrato() {
    let p = projeto("checklist");
    com_run(&p);
    let d = p.join(".schematize/overdev");
    std::fs::write(
        d.join("CHECKLIST.md"),
        "- [x] a \"aspas\"\n- [ ] b\n- [H ] decida\n  - resposta: x\n- [H -] f\n",
    )
    .expect("checklist");
    let v = rodar_json(&p, &["checklist", "--json"]);
    assert_eq!(chaves(&v), ["itens", "modo", "objetivo", "raiz"]);
    assert_eq!((v["modo"].as_str(), v["objetivo"].as_str()), (Some("active"), Some("x")));
    let it = v["itens"].as_array().expect("lista");
    assert_eq!(it.len(), 5);
    assert_eq!(chaves(&it[0]), ["humano_n", "maquina", "sub", "texto", "tipo"]);
    let tipos: Vec<&str> = it.iter().map(|i| i["tipo"].as_str().expect("slug")).collect();
    assert_eq!(tipos, ["done", "open", "hopen", "nota", "hrec"]);
    assert_eq!(it[0]["texto"], "a \"aspas\"");
    assert_eq!((it[2]["humano_n"].as_u64(), it[2]["maquina"].as_bool()), (Some(1), Some(false)));
    assert_eq!(it[3]["sub"], true);
    let _ = std::fs::remove_dir_all(&p);
}

/// **Sem run, o checklist é vazio e `modo`/`objetivo` são `null`** — nunca `""`.
#[test]
fn checklist_sem_run_e_vazio_com_null() {
    let p = projeto("checklist-vazio");
    let v = rodar_json(&p, &["checklist", "--json"]);
    assert!(v["modo"].is_null() && v["objetivo"].is_null(), "{v}");
    assert!(v["itens"].as_array().expect("lista").is_empty());
    let _ = std::fs::remove_dir_all(&p);
}

/// **O CONTRATO do `history --json`:** sem snapshot é lista vazia (e saída 0), com snapshot
/// traz id, arquivo, epoch e bytes.
#[test]
fn o_shape_do_historico_e_contrato() {
    let p = projeto("historico");
    let v = rodar_json(&p, &["history", "--json"]);
    assert_eq!(chaves(&v), ["snapshots"]);
    assert!(v["snapshots"].as_array().expect("lista").is_empty());
    com_run(&p);
    let o = Command::new(BIN)
        .arg("snapshot")
        .current_dir(&p)
        .env("HOME", &p)
        .env("SCHEMATIZE_SEM_MARKET", "1")
        .output()
        .expect("snap");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v = rodar_json(&p, &["history", "--json"]);
    let s = &v["snapshots"][0];
    assert_eq!(chaves(s), ["arquivo", "bytes", "em", "id"]);
    assert!(
        s["id"].is_i64() && s["em"].is_i64() && s["bytes"].is_i64() && s["arquivo"].is_string()
    );
    let _ = std::fs::remove_dir_all(&p);
}
