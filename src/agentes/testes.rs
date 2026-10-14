//! Testes unitários de schema, carga, merge e validação (sem tocar o HOME real).

use super::carga::{carregar_caminhos, de_texto};
use super::*;
use std::path::PathBuf;

/// Caso hostil: texto TOML + predicado que reconhece o erro tipado esperado.
type Caso = (String, fn(&ErroAgentes) -> bool);

/// **O quê:** diretório limpo sob `target/test-tmp` (nunca o /tmp, nunca o HOME real).
fn tmp(nome: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-tmp")
        .join(format!("agentes-{}-{nome}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("criar dir de teste");
    d
}

/// Os defaults batem com a tabela do contrato e passam na própria validação.
#[test]
fn defaults_batem_com_o_contrato() {
    let a = Agentes::padrao();
    a.validar().expect("defaults válidos");
    assert_eq!(a.papeis.len(), 9);
    let esperado = [
        ("principal", Motor::Claude, "opus"),
        ("trivial", Motor::Claude, "haiku"),
        ("executor", Motor::Claude, "sonnet"),
        ("fallback-1", Motor::Codex, "gpt-5.6-terra"),
        ("fallback-cursor", Motor::Cursor, "auto"),
        ("fallback-2", Motor::Claude, "opus"),
        ("visual", Motor::Codex, "gpt-5.6-terra"),
        ("imagem", Motor::Codex, "gpt-5.6-terra"),
        ("complementar", Motor::Claude, "sonnet"),
    ];
    for (n, m, mod_) in esperado {
        let p = a.papel(n).unwrap_or_else(|| panic!("falta {n}"));
        assert_eq!((p.motor, p.modelo.as_str(), p.ativo), (m, mod_, true), "{n}");
    }
    assert_eq!(a.escada(), ["trivial", "executor", "fallback-1", "fallback-cursor", "fallback-2"]);
    assert_eq!(a.roteamento.get("visual").map(String::as_str), Some("visual"));
    assert_eq!(a.roteamento.get("imagem").map(String::as_str), Some("imagem"));
}

/// Serializar e reler devolve exatamente a mesma config.
#[test]
fn toml_faz_roundtrip() {
    let a = Agentes::padrao();
    let t = a.para_toml().expect("serializa");
    assert!(t.contains("versao = 1"), "{t}");
    let b = de_texto(&t).expect("relê");
    assert_eq!(a, b);
}

/// Projeto sobrescreve global por papel; escada/roteamento inteiros; o resto vem dos padrões.
#[test]
fn projeto_sobrepoe_global() {
    let d = tmp("merge");
    let g = d.join("global.toml");
    let p = d.join("proj.toml");
    std::fs::write(
        &g,
        "versao = 1\n[papeis.executor]\nmotor = \"codex\"\nmodelo = \"gpt-x\"\n\
         [papeis.extra]\nmotor = \"cursor\"\nmodelo = \"auto\"\nativo = false\n",
    )
    .expect("w");
    std::fs::write(
        &p,
        "escada = [\"extra\", \"executor\"]\n[papeis.executor]\nmotor = \"cursor\"\nmodelo = \"m\"\n",
    )
    .expect("w");
    let so_global = carregar_caminhos(&g, None).expect("global");
    assert_eq!(so_global.papel("executor").map(|x| x.motor), Some(Motor::Codex));
    assert!(!so_global.papel("extra").expect("extra").ativo);
    let a = carregar_caminhos(&g, Some(&p)).expect("efetivo");
    assert_eq!(
        a.papel("executor").map(|x| (x.motor, x.modelo.as_str())),
        Some((Motor::Cursor, "m"))
    );
    assert_eq!(a.escada(), ["extra", "executor"]);
    assert_eq!(a.papel("trivial").map(|x| x.modelo.as_str()), Some("haiku"), "padrão preservado");
    assert!(a.papel("extra").is_some(), "papel extra do global preservado");
}

/// Arquivos ausentes nunca são erro: valem os padrões.
#[test]
fn ausente_vira_padrao() {
    let d = tmp("ausente");
    let a =
        carregar_caminhos(&d.join("nao-existe.toml"), Some(&d.join("nem-este.toml"))).expect("ok");
    assert_eq!(a, Agentes::padrao());
}

/// Entradas hostis viram `Err` TIPADO, nunca pânico.
#[test]
fn entradas_hostis_viram_erro_tipado() {
    let longo = "a".repeat(33);
    let casos: Vec<Caso> = vec![
        ("[papeis.x]\nmotor=\"gemini\"\nmodelo=\"m\"".into(), |e| {
            matches!(e, ErroAgentes::MotorDesconhecido { .. })
        }),
        ("[papeis.x]\nmotor=\"Claude\"\nmodelo=\"m\"".into(), |e| {
            matches!(e, ErroAgentes::MotorDesconhecido { .. })
        }),
        ("[papeis.x]\nmotor=\"claude\"\nmodelo=\"\"".into(), |e| {
            matches!(e, ErroAgentes::ModeloVazio(_))
        }),
        ("[papeis.x]\nmotor=\"claude\"\nmodelo=\"   \"".into(), |e| {
            matches!(e, ErroAgentes::ModeloVazio(_))
        }),
        ("[papeis.x]\nmotor=\"claude\"".into(), |e| matches!(e, ErroAgentes::CampoAusente { .. })),
        ("[papeis.x]\nmodelo=\"m\"".into(), |e| matches!(e, ErroAgentes::CampoAusente { .. })),
        ("escada = [\"fantasma\"]".into(), |e| matches!(e, ErroAgentes::PapelAusente { .. })),
        ("[roteamento]\nvisual = \"fantasma\"".into(), |e| {
            matches!(e, ErroAgentes::PapelAusente { .. })
        }),
        ("versao = 2".into(), |e| matches!(e, ErroAgentes::Versao(2))),
        ("versao = 0".into(), |e| matches!(e, ErroAgentes::Versao(0))),
        ("versao = \"1\"".into(), |e| matches!(e, ErroAgentes::Toml { .. })),
        ("isto { não é toml ===".into(), |e| matches!(e, ErroAgentes::Toml { .. })),
        ("\u{0}\u{1}\u{2}".into(), |e| matches!(e, ErroAgentes::Toml { .. })),
        ("[papeis.\"Ação\"]\nmotor=\"claude\"\nmodelo=\"m\"".into(), |e| {
            matches!(e, ErroAgentes::NomeInvalido(_))
        }),
        ("[papeis.\"a\\u0000b\"]\nmotor=\"claude\"\nmodelo=\"m\"".into(), |e| {
            matches!(e, ErroAgentes::NomeInvalido(_))
        }),
        ("[papeis.MAIUSCULO]\nmotor=\"claude\"\nmodelo=\"m\"".into(), |e| {
            matches!(e, ErroAgentes::NomeInvalido(_))
        }),
        (format!("[papeis.{longo}]\nmotor=\"claude\"\nmodelo=\"m\""), |e| {
            matches!(e, ErroAgentes::NomeInvalido(_))
        }),
        ("[papeis.\"\"]\nmotor=\"claude\"\nmodelo=\"m\"".into(), |e| {
            matches!(e, ErroAgentes::NomeInvalido(_))
        }),
    ];
    for (texto, confere) in casos {
        let r = de_texto(&texto);
        let e = r.expect_err(&format!("deveria falhar: {texto:?}"));
        assert!(confere(&e), "erro de tipo inesperado para {texto:?}: {e:?}");
        assert!(!e.to_string().is_empty());
    }
}

/// `principal` é obrigatório; a validação direta também o exige.
#[test]
fn sem_principal_e_erro() {
    let mut a = Agentes::padrao();
    a.papeis.remove("principal");
    assert_eq!(a.validar(), Err(ErroAgentes::SemPrincipal));
}

/// Nome com exatamente 32 caracteres vale; 33 não.
#[test]
fn limite_de_32_caracteres() {
    let ok = "a".repeat(32);
    let t = format!("[papeis.{ok}]\nmotor=\"claude\"\nmodelo=\"m\"");
    assert!(de_texto(&t).is_ok());
}

/// `salvar_*` valida antes de gravar e a gravação é relida igual (override do projeto).
#[test]
fn salvar_projeto_valida_e_grava() {
    let d = tmp("salvar");
    let mut a = Agentes::padrao();
    a.papeis.get_mut("executor").expect("executor").modelo = "sonnet-x".into();
    salvar_projeto(&d, &a).expect("salva");
    let lido = carregar_caminhos(&d.join("nao.toml"), Some(&caminho_projeto(&d))).expect("lê");
    assert_eq!(lido, a);
    let mut ruim = a.clone();
    ruim.escada.push("fantasma".into());
    assert!(salvar_projeto(&d, &ruim).is_err(), "config inválida não pode ser gravada");
    let ainda = carregar_caminhos(&d.join("nao.toml"), Some(&caminho_projeto(&d))).expect("lê");
    assert_eq!(ainda, a, "arquivo intacto após recusa");
}

/// `proximo_ativo` pula inativos e devolve None fora da escada / no fim.
#[test]
fn proximo_ativo_pula_inativos() {
    let mut a = Agentes::padrao();
    a.papeis.get_mut("fallback-1").expect("p").ativo = false;
    assert_eq!(proximo_ativo(&a, "executor").as_deref(), Some("fallback-cursor"));
    assert_eq!(proximo_ativo(&a, "fallback-2"), None);
    assert_eq!(proximo_ativo(&a, "visual"), None);
}

/// O argv de cada motor é exatamente o do contrato.
#[test]
fn argv_dos_motores() {
    let p = std::path::Path::new("/proj");
    assert_eq!(
        linha_de_comando(Motor::Claude, "opus", p, "oi"),
        ["-p", "--model", "opus", "--output-format", "json", "oi"]
    );
    assert_eq!(
        linha_de_comando(Motor::Codex, "g", p, "oi"),
        ["exec", "-m", "g", "-s", "workspace-write", "--json", "-C", "/proj", "oi"]
    );
    assert_eq!(
        linha_de_comando(Motor::Cursor, "auto", p, "-x"),
        ["-p", "--model", "auto", "--output-format", "json", "--force", "\n-x"]
    );
}
