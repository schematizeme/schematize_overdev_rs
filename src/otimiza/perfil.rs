//! Modo mínimo de contexto por unidade (IA-09).
//!
//! **O quê:** flags que cortam do contexto da unidade o que ela não precisa (MCPs extras,
//! skills, CLAUDE.md auto-descoberto via `--bare`) e verificador do evento `system/init`
//! que reprova MCP fora do esperado.
//! **Onde:** `comando::aplicar` (monta) e consumidor do stream-json (verifica).
//! ATENÇÃO: `--bare` usa SÓ `ANTHROPIC_API_KEY` (nunca o login de assinatura); por isso é
//! opt-in e, sem a chave, [`flags`] devolve erro acionável em vez de uma unidade sem auth.

use serde_json::Value;

/// Perfil de contexto da unidade.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Perfil {
    /// Usar `--bare` (exige `ANTHROPIC_API_KEY`).
    pub bare: bool,
    /// Arquivos `--mcp-config` (só estes MCPs; `--strict-mcp-config` sempre vai junto).
    pub mcp_configs: Vec<String>,
    /// Arquivos `--append-system-prompt-file`.
    pub prompt_files: Vec<String>,
    /// Desliga skills/slash commands (`--disable-slash-commands`).
    pub sem_skills: bool,
}

/// Monta as flags do perfil.
///
/// **O quê:** `--strict-mcp-config` sempre (zero MCP se a lista é vazia); `--bare` só com chave.
/// Argumento começando com `-` é recusado (não vira flag por engano).
/// **Onde:** `comando::aplicar`. `tem_api_key` é injetado (testável, sem ler o ambiente).
pub fn flags(p: &Perfil, tem_api_key: bool) -> Result<Vec<String>, String> {
    if p.bare && !tem_api_key {
        return Err("contexto minimo com `bare` exige ANTHROPIC_API_KEY (o --bare nao le o \
                    login de assinatura); defina a chave ou desligue `bare` em otimiza.toml"
            .into());
    }
    let mut f: Vec<String> = Vec::new();
    if p.bare {
        f.push("--bare".into());
    }
    f.push("--strict-mcp-config".into());
    for (flag, lista) in
        [("--mcp-config", &p.mcp_configs), ("--append-system-prompt-file", &p.prompt_files)]
    {
        for arq in lista {
            if arq.is_empty() || arq.starts_with('-') {
                return Err(format!("caminho invalido para {flag}: `{arq}`"));
            }
            f.push(flag.into());
            f.push(arq.clone());
        }
    }
    if p.sem_skills {
        f.push("--disable-slash-commands".into());
    }
    Ok(f)
}

/// Nomes dos MCPs de um evento `system/init` (`mcp_servers[].name`).
/// **O quê:** `None` se a linha não é JSON ou não é o init. **Onde:** [`verificar_init`].
pub fn mcps_do_init(linha: &str) -> Option<Vec<String>> {
    let v: Value = serde_json::from_str(linha).ok()?;
    if v.get("type").and_then(Value::as_str) != Some("system")
        || v.get("subtype").and_then(Value::as_str) != Some("init")
    {
        return None;
    }
    let l = v.get("mcp_servers").and_then(Value::as_array);
    Some(
        l.into_iter().flatten().filter_map(|m| m.get("name")?.as_str().map(String::from)).collect(),
    )
}

/// Reprova MCP fora do esperado.
///
/// **O quê:** `Ok` se todo MCP do init está em `esperados`; `Err` lista os extras (ordenados).
/// Linha que não é init = `Err` (o init é obrigatório: sem ele não há como provar o contexto).
/// **Onde:** consumidor do stream-json da unidade.
pub fn verificar_init(linha: &str, esperados: &[String]) -> Result<(), Vec<String>> {
    let Some(vistos) = mcps_do_init(linha) else {
        return Err(vec!["evento system/init ausente ou ilegivel".to_string()]);
    };
    let mut extras: Vec<String> = vistos.into_iter().filter(|m| !esperados.contains(m)).collect();
    extras.sort();
    if extras.is_empty() {
        Ok(())
    } else {
        Err(extras)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn p() -> Perfil {
        Perfil { mcp_configs: vec!["mcp.json".into()], sem_skills: true, ..Perfil::default() }
    }

    #[test]
    fn flags_sem_bare_e_com_bare() {
        let f = flags(&p(), false).unwrap();
        assert_eq!(
            f,
            ["--strict-mcp-config", "--mcp-config", "mcp.json", "--disable-slash-commands"]
        );
        let b = Perfil { bare: true, ..p() };
        assert!(flags(&b, false).unwrap_err().contains("ANTHROPIC_API_KEY"));
        assert_eq!(flags(&b, true).unwrap()[0], "--bare");
        assert_eq!(flags(&Perfil::default(), false).unwrap(), ["--strict-mcp-config"]);
    }

    #[test]
    fn caminho_hostil_e_recusado() {
        for ruim in ["", "--dangerously-skip-permissions"] {
            let x = Perfil { prompt_files: vec![ruim.into()], ..Perfil::default() };
            assert!(flags(&x, false).is_err(), "{ruim}");
        }
    }

    #[test]
    fn init_com_mcp_extra_falha() {
        let ok = r#"{"type":"system","subtype":"init","mcp_servers":[{"name":"fs","status":"connected"}]}"#;
        let ruim = r#"{"type":"system","subtype":"init","mcp_servers":[{"name":"fs"},{"name":"zz"},{"name":"aa"}]}"#;
        let esp = vec!["fs".to_string()];
        assert!(verificar_init(ok, &esp).is_ok());
        assert_eq!(verificar_init(ruim, &esp).unwrap_err(), ["aa", "zz"]);
        assert!(verificar_init("{\"type\":\"assistant\"}", &esp).is_err());
        assert!(verificar_init("lixo", &esp).is_err());
        let sem = r#"{"type":"system","subtype":"init"}"#;
        assert!(verificar_init(sem, &[]).is_ok());
    }
}
