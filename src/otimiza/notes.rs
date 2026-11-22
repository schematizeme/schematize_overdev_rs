//! NOTES/handoff estruturado por unidade, <= 2.000 tokens (IA-11).
//!
//! **O quê:** schema fixo (feito, decisões, arquivos tocados, próximo passo) validado por
//! [`esquema`], limite de tokens, persistência em `<dir>/<id>.notes.json` e o brief da unidade
//! filha montado SÓ com contrato + NOTES (sem reler histórico).
//! **Onde:** fim de uma unidade (grava NOTES) e início da unidade retomada (`brief_retomada`).
//! Não substitui o handoff vivo do projeto (`handoff_vivo`); é por unidade.

use super::contexto::estimar_tokens;
use super::esquema;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Teto de tokens (estimador `contexto::estimar_tokens`, viés seguro).
pub const NOTES_MAX_TOKENS: u64 = 2_000;

/// NOTES de uma unidade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notes {
    /// O que foi feito.
    pub feito: Vec<String>,
    /// Decisões tomadas.
    pub decisoes: Vec<String>,
    /// Arquivos tocados.
    pub arquivos: Vec<String>,
    /// Próximo passo concreto.
    pub proximo_passo: String,
}

/// Schema das NOTES. **Onde:** `--json-schema` da unidade e [`validar`].
pub fn schema() -> Value {
    let lista = json!({"type":"array","maxItems":30,"items":{"type":"string","maxLength":300}});
    json!({"type":"object","additionalProperties":false,
           "required":["feito","decisoes","arquivos","proximo_passo"],
           "properties":{"feito":lista,"decisoes":lista,"arquivos":lista,
                         "proximo_passo":{"type":"string","maxLength":500}}})
}

/// Valida NOTES: schema + teto de tokens.
///
/// **O quê:** `Err` lista as violações; passa de [`NOTES_MAX_TOKENS`] = erro (não trunca em silêncio).
/// **Onde:** ao ler o que a unidade gravou, antes de entregar à filha.
pub fn validar(v: &Value) -> Result<Notes, Vec<String>> {
    esquema::validar(&schema(), v)?;
    let tok = estimar_tokens(&v.to_string());
    if tok > NOTES_MAX_TOKENS {
        return Err(vec![format!("NOTES com ~{tok} tokens acima de {NOTES_MAX_TOKENS}")]);
    }
    let lista = |k: &str| -> Vec<String> {
        v.get(k)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|x| x.as_str().map(String::from))
            .collect()
    };
    Ok(Notes {
        feito: lista("feito"),
        decisoes: lista("decisoes"),
        arquivos: lista("arquivos"),
        proximo_passo: v.get("proximo_passo").and_then(Value::as_str).unwrap_or_default().into(),
    })
}

impl Notes {
    /// JSON canônico das NOTES. **Onde:** gravação e brief.
    pub fn para_json(&self) -> Value {
        json!({"feito":self.feito,"decisoes":self.decisoes,"arquivos":self.arquivos,
               "proximo_passo":self.proximo_passo})
    }
}

/// Caminho das NOTES da unidade. `id` fora de `[A-Za-z0-9_-]{1,64}` é recusado (sem `..`/`/`).
/// **Onde:** [`salvar`] e [`carregar`].
pub fn caminho(dir: &Path, id: &str) -> Result<PathBuf, String> {
    let ok = !id.is_empty()
        && id.len() <= 64
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !ok {
        return Err(format!("id de unidade invalido para NOTES: `{id}`"));
    }
    Ok(dir.join(format!("{id}.notes.json")))
}

/// Grava as NOTES (valida antes; escrita atômica tmp+rename). **Onde:** fim de unidade.
pub fn salvar(dir: &Path, id: &str, n: &Notes) -> Result<PathBuf, String> {
    let v = n.para_json();
    validar(&v).map_err(|e| e.join("; "))?;
    let alvo = caminho(dir, id)?;
    std::fs::create_dir_all(dir).map_err(|e| format!("criar {}: {e}", dir.display()))?;
    let tmp = alvo.with_extension("json.tmp");
    std::fs::write(&tmp, v.to_string()).map_err(|e| format!("gravar {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &alvo).map_err(|e| format!("renomear {}: {e}", alvo.display()))?;
    Ok(alvo)
}

/// Lê e valida as NOTES gravadas. **Onde:** início da unidade retomada.
pub fn carregar(dir: &Path, id: &str) -> Result<Notes, String> {
    let p = caminho(dir, id)?;
    let t = std::fs::read_to_string(&p).map_err(|e| format!("ler {}: {e}", p.display()))?;
    let v: Value =
        serde_json::from_str(&t).map_err(|e| format!("{}: JSON invalido: {e}", p.display()))?;
    validar(&v).map_err(|e| format!("{}: {}", p.display(), e.join("; ")))
}

/// Brief da unidade filha: SÓ contrato + NOTES (nada de histórico do pai).
///
/// **O quê:** texto determinístico. **Onde:** retomada de unidade (`retomada::Checkpoint`).
pub fn brief_retomada(contrato: &str, n: &Notes) -> String {
    format!(
        "CONTRATO:\n{contrato}\n\nNOTES DA TENTATIVA ANTERIOR (continue daqui, nao releia o historico):\n{}\n",
        n.para_json()
    )
}

#[cfg(test)]
mod testes {
    use super::*;

    fn n() -> Notes {
        Notes {
            feito: vec!["A".into()],
            decisoes: vec!["usar X".into()],
            arquivos: vec!["src/a.rs".into()],
            proximo_passo: "rodar o gate".into(),
        }
    }

    #[test]
    fn roundtrip_e_brief_so_com_contrato_e_notes() {
        let d = std::env::temp_dir().join(format!("otz-notes-{}", std::process::id()));
        let p = salvar(&d, "u-1", &n()).unwrap();
        assert!(p.ends_with("u-1.notes.json"));
        let lida = carregar(&d, "u-1").unwrap();
        assert_eq!(lida, n());
        let b = brief_retomada("contrato Z", &lida);
        assert!(b.contains("contrato Z") && b.contains("rodar o gate"));
        assert!(!b.contains("transcript"));
        assert!(estimar_tokens(&b) < NOTES_MAX_TOKENS);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn hostis_schema_tokens_e_id() {
        assert!(validar(&json!({"feito":[]})).is_err());
        assert!(
            validar(&json!({"feito":[],"decisoes":[],"arquivos":[],"proximo_passo":1})).is_err()
        );
        let gordo = Notes { feito: vec!["x".repeat(300); 30], ..n() };
        let gordo2 = Notes { decisoes: vec!["y".repeat(300); 30], ..gordo.clone() };
        let e = validar(&gordo2.para_json()).unwrap_err();
        assert!(e[0].contains("tokens"), "{e:?}");
        for id in ["", "../x", "a/b", "ação", &"a".repeat(65)] {
            assert!(caminho(Path::new("/x"), id).is_err(), "{id}");
        }
        let d = std::env::temp_dir().join(format!("otz-notes2-{}", std::process::id()));
        assert!(carregar(&d, "nao-existe").is_err());
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("lixo.notes.json"), "{nao json").unwrap();
        assert!(carregar(&d, "lixo").unwrap_err().contains("JSON invalido"));
        std::fs::remove_dir_all(&d).unwrap();
    }
}
