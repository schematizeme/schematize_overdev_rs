//! Saída estruturada obrigatória e curta da unidade (IA-14).
//!
//! **O quê:** schema `{status, resumo, arquivos, gate}`, flag `--json-schema` e extração do
//! campo `structured_output` do JSON de resultado do `claude -p`. Sem `structured_output`
//! válido a unidade FALHOU (`failed:schema`) — texto livre nunca é promovido a sucesso.
//! **Onde:** gerador de comando (`comando::aplicar`) e parser do resultado; o principal só
//! vê o JSON curto de [`Saida::para_principal`].

use super::esquema;
use serde_json::{json, Value};

/// Limite de caracteres do resumo (protege o contexto do principal).
pub const RESUMO_MAX: u64 = 500;

/// Status declarado pela unidade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Concluiu o contrato.
    Done,
    /// Falhou.
    Failed,
    /// Bloqueada (precisa de decisão/dependência).
    Blocked,
}

/// Saída validada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saida {
    /// Status.
    pub status: Status,
    /// Resumo curto.
    pub resumo: String,
    /// Arquivos tocados.
    pub arquivos: Vec<String>,
    /// Gate passou?
    pub gate_ok: bool,
}

/// Por que a saída não vale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FalhaSchema {
    /// O resultado não trouxe `structured_output`.
    Ausente,
    /// Trouxe, mas viola o schema (todas as violações).
    Invalido(Vec<String>),
}

/// O schema (JSON Schema, subconjunto de [`esquema`]). **Onde:** `--json-schema` e validação.
pub fn schema() -> Value {
    json!({
        "type": "object",
        "required": ["status", "resumo", "arquivos", "gate"],
        "additionalProperties": false,
        "properties": {
            "status": {"type": "string", "enum": ["done", "failed", "blocked"]},
            "resumo": {"type": "string", "maxLength": RESUMO_MAX},
            "arquivos": {"type": "array", "maxItems": 50,
                         "items": {"type": "string", "maxLength": 300}},
            "gate": {"type": "object", "required": ["ok"], "additionalProperties": false,
                     "properties": {"ok": {"type": "boolean"},
                                    "comando": {"type": "string", "maxLength": 200}}}
        }
    })
}

/// `["--json-schema", <schema>]`. **Onde:** `comando::aplicar`.
pub fn flags() -> Vec<String> {
    vec!["--json-schema".into(), schema().to_string()]
}

/// Extrai e valida a saída estruturada do JSON de resultado.
///
/// **O quê:** `Ausente` se não há `structured_output` (ou é `null`); `Invalido` se viola o schema.
/// **Onde:** fim de tentativa; `Err` vira `failed:schema` ([`estado_de`]).
pub fn extrair(resultado: &Value) -> Result<Saida, FalhaSchema> {
    let so = match resultado.get("structured_output") {
        None | Some(Value::Null) => return Err(FalhaSchema::Ausente),
        Some(v) => v,
    };
    esquema::validar(&schema(), so).map_err(FalhaSchema::Invalido)?;
    let status = match so.get("status").and_then(Value::as_str) {
        Some("done") => Status::Done,
        Some("blocked") => Status::Blocked,
        _ => Status::Failed,
    };
    let texto = |k: &str| so.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let arquivos = so.get("arquivos").and_then(Value::as_array).into_iter().flatten();
    Ok(Saida {
        status,
        resumo: texto("resumo"),
        arquivos: arquivos.filter_map(|a| a.as_str().map(String::from)).collect(),
        gate_ok: so.pointer("/gate/ok").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// Estado do ledger para o desfecho da extração (`done`, `failed`, `blocked` ou `failed:schema`).
/// **Onde:** `ledger::Registro::estado`.
pub fn estado_de(r: &Result<Saida, FalhaSchema>) -> &'static str {
    match r {
        Err(_) => "failed:schema",
        Ok(s) if s.status == Status::Done && s.gate_ok => "done",
        Ok(s) if s.status == Status::Blocked => "blocked",
        Ok(_) => "failed",
    }
}

impl Saida {
    /// O JSON curto que o principal vê (nunca o transcript). **Onde:** relatório ao principal.
    pub fn para_principal(&self) -> String {
        let st = match self.status {
            Status::Done => "done",
            Status::Failed => "failed",
            Status::Blocked => "blocked",
        };
        json!({"status": st, "resumo": self.resumo, "arquivos": self.arquivos,
               "gate_ok": self.gate_ok})
        .to_string()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn res(so: Value) -> Value {
        json!({"type":"result","subtype":"success","result":"texto livre","structured_output":so})
    }

    #[test]
    fn valida_e_mapeia() {
        let r = res(json!({"status":"done","resumo":"ok","arquivos":["a.rs"],"gate":{"ok":true}}));
        let s = extrair(&r);
        assert_eq!(estado_de(&s), "done");
        let s = s.unwrap();
        assert_eq!(s.arquivos, ["a.rs"]);
        assert!(s.para_principal().contains("\"gate_ok\":true"));
        let g = res(json!({"status":"done","resumo":"x","arquivos":[],"gate":{"ok":false}}));
        assert_eq!(estado_de(&extrair(&g)), "failed", "done sem gate verde nao e done");
    }

    #[test]
    fn invalido_ou_ausente_e_failed_schema() {
        let ruim = res(json!({"status":"talvez","resumo":"x".repeat(501),"arquivos":[]}));
        let e = extrair(&ruim);
        assert!(matches!(&e, Err(FalhaSchema::Invalido(v)) if v.len() >= 3));
        assert_eq!(estado_de(&e), "failed:schema");
        for r in [json!({"result":"so texto"}), res(Value::Null), json!(null), json!([])] {
            assert_eq!(extrair(&r), Err(FalhaSchema::Ausente));
        }
        let extra =
            res(json!({"status":"done","resumo":"x","arquivos":[],"gate":{"ok":true},"x":1}));
        assert_eq!(estado_de(&extrair(&extra)), "failed:schema");
    }

    #[test]
    fn flags_trazem_schema_parseavel() {
        let f = flags();
        assert_eq!(f[0], "--json-schema");
        assert!(serde_json::from_str::<Value>(&f[1]).is_ok());
    }
}
