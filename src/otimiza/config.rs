//! Config do otimizador de IA por projeto: `.overdev/otimiza.toml` (tudo DESLIGADO por padrão).
//!
//! **O quê:** liga, por projeto, os tetos por classe (IA-05), o contexto mínimo (IA-09) e a saída
//! estruturada (IA-14) na linha de comando do `claude -p`. Arquivo ausente = nada muda (default
//! seguro: o comportamento anterior). Arquivo inválido = ERRO (não é ignorado em silêncio).
//! **Onde:** `agentes::delegar` (via `comando::aplicar_do_projeto`).
//!
//! ```toml
//! [tetos]
//! classe = "codigo"          # mecanica | codigo | revisao
//! [contexto_minimo]
//! bare = false               # true exige ANTHROPIC_API_KEY
//! mcp_configs = []
//! sem_skills = true
//! [saida]
//! estruturada = true
//! ```

use super::limites::{classe_de_nome, limites_da_classe, Limites};
use super::perfil::Perfil;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Configuração efetiva.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cfg {
    /// Teto da classe (IA-05), se ligado.
    pub tetos: Option<Limites>,
    /// Perfil de contexto mínimo (IA-09), se ligado.
    pub perfil: Option<Perfil>,
    /// Exigir saída estruturada (IA-14).
    pub saida_estruturada: bool,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Arq {
    tetos: Option<Tetos>,
    contexto_minimo: Option<Min>,
    saida: Option<Saida>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Tetos {
    classe: String,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
struct Min {
    bare: bool,
    mcp_configs: Vec<String>,
    prompt_files: Vec<String>,
    sem_skills: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Saida {
    estruturada: bool,
}

/// Lê a config de um texto TOML. **O quê:** classe desconhecida = `Err` com as opções.
/// **Onde:** [`carregar`] e testes.
pub fn parse_toml(txt: &str) -> Result<Cfg, String> {
    let a: Arq = toml::from_str(txt).map_err(|e| format!("otimiza.toml invalido: {e}"))?;
    let tetos = match a.tetos {
        None => None,
        Some(t) => Some(limites_da_classe(classe_de_nome(&t.classe).ok_or_else(|| {
            format!("otimiza.toml: classe `{}` desconhecida (mecanica|codigo|revisao)", t.classe)
        })?)),
    };
    let perfil = a.contexto_minimo.map(|m| Perfil {
        bare: m.bare,
        mcp_configs: m.mcp_configs,
        prompt_files: m.prompt_files,
        sem_skills: m.sem_skills,
    });
    Ok(Cfg { tetos, perfil, saida_estruturada: a.saida.is_some_and(|s| s.estruturada) })
}

/// Caminho do arquivo no projeto. **Onde:** [`carregar_do_projeto`].
pub fn caminho(projeto: &Path) -> PathBuf {
    crate::paths::overdev_dir_at(projeto).join("otimiza.toml")
}

/// Carrega de `caminho`: ausente = tudo desligado; ilegível/inválido = `Err`.
/// **Onde:** quem monta a linha de comando.
pub fn carregar(caminho: &Path) -> Result<Cfg, String> {
    match std::fs::read_to_string(caminho) {
        Ok(t) => parse_toml(&t).map_err(|e| format!("{}: {e}", caminho.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Cfg::default()),
        Err(e) => Err(format!("ler {}: {e}", caminho.display())),
    }
}

/// [`carregar`] do `otimiza.toml` do projeto. **Onde:** `comando::aplicar_do_projeto`.
pub fn carregar_do_projeto(projeto: &Path) -> Result<Cfg, String> {
    carregar(&caminho(projeto))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ausente_e_desligado_e_completo_liga_tudo() {
        let d = std::env::temp_dir().join(format!("otz-cfg-{}", std::process::id()));
        assert_eq!(carregar(&d.join("nada.toml")).unwrap(), Cfg::default());
        let c = parse_toml("[tetos]\nclasse=\"Codigo\"\n[contexto_minimo]\nsem_skills=true\n[saida]\nestruturada=true\n").unwrap();
        assert_eq!(c.tetos.unwrap().max_turns, 40);
        assert!(c.perfil.unwrap().sem_skills && c.saida_estruturada);
        assert_eq!(parse_toml("").unwrap(), Cfg::default());
    }

    #[test]
    fn invalido_e_erro() {
        for ruim in [
            "[tetos]\nclasse=\"x\"",
            "[tetos]",
            "[nao_existe]\na=1",
            "[saida]\nestruturada=\"sim\"",
            "{{",
        ] {
            assert!(parse_toml(ruim).is_err(), "{ruim}");
        }
        let d = std::env::temp_dir().join(format!("otz-cfg2-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("o.toml"), "[tetos]\nclasse=\"?\"").unwrap();
        assert!(carregar(&d.join("o.toml")).unwrap_err().contains("o.toml"));
        std::fs::remove_dir_all(&d).unwrap();
    }
}
