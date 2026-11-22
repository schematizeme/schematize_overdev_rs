//! Construtor da linha de comando do `claude -p` com as otimizações de IA (IA-05, IA-09, IA-14).
//!
//! **O quê:** injeta, ANTES do prompt posicional (último argumento), as flags de teto, de
//! contexto mínimo, de saída estruturada e o `--session-id` pré-gerado. Recusa flag duplicada
//! (não sobrescreve escolha do chamador em silêncio) e valor que pareça flag.
//! **Onde:** `agentes::delegar` (Motor Claude) por [`aplicar_do_projeto`]; o scheduler de
//! ondas pode chamar [`aplicar`] direto com [`OpcoesIa`] próprias. Sem `otimiza.toml` o argv
//! sai IDÊNTICO ao de antes.

use super::{config, limites, perfil, prompt, saida};
use serde_json::Value;
use std::path::Path;

/// O que aplicar na linha de comando.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpcoesIa {
    /// Tetos de budget/turnos (IA-05).
    pub limites: Option<limites::Limites>,
    /// Contexto mínimo (IA-09).
    pub perfil: Option<perfil::Perfil>,
    /// Exigir `--json-schema` (IA-14).
    pub saida_estruturada: bool,
    /// `--session-id` pré-gerado (UUID), para gravar no ledger antes de lançar.
    pub session_id: Option<String>,
}

/// Flags (só os nomes `--x`) presentes em `argv`. **Onde:** [`aplicar`] (detecta duplicata).
fn nomes_de_flag(argv: &[String]) -> Vec<&str> {
    argv.iter().filter(|a| a.starts_with("--")).map(String::as_str).collect()
}

/// Flags de `--session-id`. UUID inválido = `Err`. **Onde:** [`aplicar`].
fn flags_sessao(id: &str) -> Result<Vec<String>, String> {
    if id.len() == 36 && prompt::uuid_em(id.as_bytes(), 0) {
        Ok(vec!["--session-id".into(), id.to_string()])
    } else {
        Err(format!("session-id invalido (esperado UUID): `{id}`"))
    }
}

/// Todas as flags pedidas por `o`. **Onde:** [`aplicar`].
fn flags_pedidas(o: &OpcoesIa, tem_api_key: bool) -> Result<Vec<String>, String> {
    let mut f = Vec::new();
    if let Some(l) = &o.limites {
        f.extend(limites::flags(l));
    }
    if let Some(p) = &o.perfil {
        f.extend(perfil::flags(p, tem_api_key)?);
    }
    if o.saida_estruturada {
        f.extend(saida::flags());
    }
    if let Some(id) = &o.session_id {
        f.extend(flags_sessao(id)?);
    }
    Ok(f)
}

/// Aplica as opções ao argv (sem o binário), inserindo as flags antes do prompt (último item).
///
/// **O quê:** argv vazio, ou flag que já existe no argv, é `Err`. `tem_api_key` é injetado.
/// **Onde:** `delegar` e scheduler de ondas.
pub fn aplicar(argv: Vec<String>, o: &OpcoesIa, tem_api_key: bool) -> Result<Vec<String>, String> {
    let novas = flags_pedidas(o, tem_api_key)?;
    if novas.is_empty() {
        return Ok(argv);
    }
    let Some((prompt, base)) = argv.split_last() else {
        return Err("argv vazio: sem prompt para preceder as flags de otimizacao".into());
    };
    let existentes = nomes_de_flag(base);
    if let Some(dup) = nomes_de_flag(&novas).into_iter().find(|n| existentes.contains(n)) {
        return Err(format!(
            "flag `{dup}` ja presente na linha de comando; remova-a ou desligue a otimizacao"
        ));
    }
    let mut out: Vec<String> = base.to_vec();
    out.extend(novas);
    out.push(prompt.clone());
    Ok(out)
}

/// Converte a config do projeto em opções. **Onde:** [`aplicar_do_projeto`].
pub fn opcoes_da_cfg(c: &config::Cfg) -> OpcoesIa {
    OpcoesIa {
        limites: c.tetos,
        perfil: c.perfil.clone(),
        saida_estruturada: c.saida_estruturada,
        session_id: None,
    }
}

/// Lê `otimiza.toml` do projeto e aplica ao argv do Claude. Ausente = argv intacto.
///
/// **O quê:** a chave de API vem de `ANTHROPIC_API_KEY` (só presença, o valor nunca é lido além
/// de checar não vazio). **Onde:** `agentes::delegar`, Motor Claude.
pub fn aplicar_do_projeto(argv: Vec<String>, projeto: &Path) -> Result<Vec<String>, String> {
    let cfg = config::carregar_do_projeto(projeto)?;
    let tem_chave = std::env::var_os("ANTHROPIC_API_KEY").is_some_and(|v| !v.is_empty());
    aplicar(argv, &opcoes_da_cfg(&cfg), tem_chave)
}

/// Se o projeto exige saída estruturada, valida o JSON de resultado do Claude.
///
/// **O quê:** `Some(motivo)` = a unidade FALHOU por schema (`failed:schema`); `None` = ok ou
/// não exigido. Saída que nem é JSON conta como ausente. **Onde:** `agentes::delegar`, após rodar.
pub fn falha_de_saida(projeto: &Path, saida_txt: &str) -> Result<Option<String>, String> {
    if !config::carregar_do_projeto(projeto)?.saida_estruturada {
        return Ok(None);
    }
    let v: Value = serde_json::from_str(saida_txt.trim()).unwrap_or(Value::Null);
    Ok(saida::extrair(&v).err().map(|e| match e {
        saida::FalhaSchema::Ausente => "failed:schema (sem structured_output)".to_string(),
        saida::FalhaSchema::Invalido(v) => format!("failed:schema ({})", v.join("; ")),
    }))
}

#[cfg(test)]
mod testes {
    use super::*;
    use limites::{limites_da_classe, Classe};

    fn base() -> Vec<String> {
        ["-p", "--model", "sonnet", "--output-format", "json", "FAZ ISSO"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    fn tudo() -> OpcoesIa {
        OpcoesIa {
            limites: Some(limites_da_classe(Classe::Codigo)),
            perfil: Some(perfil::Perfil { sem_skills: true, ..Default::default() }),
            saida_estruturada: true,
            session_id: Some("123e4567-e89b-12d3-a456-426614174000".into()),
        }
    }

    #[test]
    fn sem_opcoes_argv_identico_e_com_opcoes_prompt_continua_por_ultimo() {
        assert_eq!(aplicar(base(), &OpcoesIa::default(), false).unwrap(), base());
        let v = aplicar(base(), &tudo(), false).unwrap();
        assert_eq!(v.last().unwrap(), "FAZ ISSO");
        assert_eq!(v[..5], base()[..5]);
        for f in [
            "--max-budget-usd",
            "--max-turns",
            "--strict-mcp-config",
            "--json-schema",
            "--session-id",
            "--disable-slash-commands",
        ] {
            assert!(v.iter().any(|a| a == f), "falta {f}");
        }
    }

    #[test]
    fn duplicata_uuid_ruim_bare_sem_chave_e_argv_vazio_sao_erro() {
        let mut dup = base();
        dup.insert(1, "--max-turns".into());
        dup.insert(2, "3".into());
        assert!(aplicar(dup, &tudo(), false).unwrap_err().contains("--max-turns"));
        let ruim = OpcoesIa { session_id: Some("nao-uuid".into()), ..Default::default() };
        assert!(aplicar(base(), &ruim, false).is_err());
        let bare = OpcoesIa {
            perfil: Some(perfil::Perfil { bare: true, ..Default::default() }),
            ..Default::default()
        };
        assert!(aplicar(base(), &bare, false).unwrap_err().contains("ANTHROPIC_API_KEY"));
        assert!(aplicar(base(), &bare, true).is_ok());
        assert!(aplicar(vec![], &tudo(), false).is_err());
    }

    #[test]
    fn projeto_sem_arquivo_nao_muda_nada_e_com_arquivo_aplica() {
        let d = std::env::temp_dir().join(format!("otz-cmd-{}", std::process::id()));
        let ov = d.join(".overdev");
        std::fs::create_dir_all(ov.join("overdev")).unwrap();
        assert_eq!(aplicar_do_projeto(base(), &d).unwrap(), base());
        assert_eq!(falha_de_saida(&d, "lixo").unwrap(), None);
        let alvo = config::caminho(&d);
        std::fs::write(&alvo, "[tetos]\nclasse=\"mecanica\"\n[saida]\nestruturada=true\n").unwrap();
        let v = aplicar_do_projeto(base(), &d).unwrap();
        assert!(v.contains(&"--max-turns".to_string()));
        assert!(falha_de_saida(&d, "lixo").unwrap().unwrap().contains("failed:schema"));
        let ok = r#"{"structured_output":{"status":"done","resumo":"r","arquivos":[],"gate":{"ok":true}}}"#;
        assert_eq!(falha_de_saida(&d, ok).unwrap(), None);
        std::fs::write(&alvo, "quebrado =").unwrap();
        assert!(aplicar_do_projeto(base(), &d).is_err());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
