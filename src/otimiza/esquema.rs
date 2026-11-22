//! Validador mínimo de JSON Schema (subconjunto), sem dependência nova.
//!
//! **O quê:** confere um `serde_json::Value` contra `type`, `required`, `properties`,
//! `additionalProperties:false`, `enum`, `minLength/maxLength`, `maxItems`, `items`,
//! `minimum/maximum`. Schema desconhecido = palavra ignorada (nunca aceita por engano um tipo
//! errado: `type` ausente no schema só deixa de checar o tipo).
//! **Onde:** `saida` (IA-14) e `notes` (IA-11): validação da saída estruturada da unidade.

use serde_json::Value;

/// Profundidade máxima de recursão (entrada hostil não estoura a pilha).
const PROFUNDIDADE_MAX: usize = 32;

/// Valida `v` contra `schema`. `Err` traz TODAS as violações (caminho + motivo), em ordem.
///
/// **O quê:** ponto de entrada. **Onde:** `saida::extrair`, `notes::validar`.
pub fn validar(schema: &Value, v: &Value) -> Result<(), Vec<String>> {
    let mut erros = Vec::new();
    visitar(schema, v, "$", 0, &mut erros);
    if erros.is_empty() {
        Ok(())
    } else {
        Err(erros)
    }
}

/// Confere o `type` declarado. **O quê:** `integer` exige inteiro (não float). **Onde:** [`visitar`].
fn tipo_ok(tipo: &str, v: &Value) -> bool {
    match tipo {
        "string" => v.is_string(),
        "integer" => v.is_i64() || v.is_u64(),
        "number" => v.is_number(),
        "boolean" => v.is_boolean(),
        "array" => v.is_array(),
        "object" => v.is_object(),
        "null" => v.is_null(),
        _ => false,
    }
}

/// Visita um nó, acumulando erros. **O quê:** despacha para as checagens por tipo de valor.
/// **Onde:** [`validar`] (recursivo).
fn visitar(s: &Value, v: &Value, p: &str, d: usize, e: &mut Vec<String>) {
    if d > PROFUNDIDADE_MAX {
        e.push(format!("{p}: profundidade acima de {PROFUNDIDADE_MAX}"));
        return;
    }
    if let Some(t) = s.get("type").and_then(Value::as_str) {
        if !tipo_ok(t, v) {
            e.push(format!("{p}: esperado {t}"));
            return;
        }
    }
    if let Some(en) = s.get("enum").and_then(Value::as_array) {
        if !en.contains(v) {
            e.push(format!("{p}: fora do enum"));
        }
    }
    checar_texto(s, v, p, e);
    checar_numero(s, v, p, e);
    if let Some(o) = v.as_object() {
        checar_objeto(s, o, p, d, e);
    }
    if let Some(a) = v.as_array() {
        checar_array(s, a, p, d, e);
    }
}

/// Limites de string. **O quê:** conta caracteres (não bytes). **Onde:** [`visitar`].
fn checar_texto(s: &Value, v: &Value, p: &str, e: &mut Vec<String>) {
    let Some(t) = v.as_str() else { return };
    let n = t.chars().count() as u64;
    if s.get("maxLength").and_then(Value::as_u64).is_some_and(|m| n > m) {
        e.push(format!("{p}: texto com {n} caracteres acima do máximo"));
    }
    if s.get("minLength").and_then(Value::as_u64).is_some_and(|m| n < m) {
        e.push(format!("{p}: texto abaixo do mínimo"));
    }
}

/// Limites numéricos. **O quê:** `minimum`/`maximum` inclusivos. **Onde:** [`visitar`].
fn checar_numero(s: &Value, v: &Value, p: &str, e: &mut Vec<String>) {
    let Some(x) = v.as_f64() else { return };
    if s.get("minimum").and_then(Value::as_f64).is_some_and(|m| x < m) {
        e.push(format!("{p}: abaixo do mínimo"));
    }
    if s.get("maximum").and_then(Value::as_f64).is_some_and(|m| x > m) {
        e.push(format!("{p}: acima do máximo"));
    }
}

/// Objeto: `required`, `properties` e `additionalProperties:false`. **Onde:** [`visitar`].
fn checar_objeto(
    s: &Value,
    o: &serde_json::Map<String, Value>,
    p: &str,
    d: usize,
    e: &mut Vec<String>,
) {
    let props = s.get("properties").and_then(Value::as_object);
    for r in s.get("required").and_then(Value::as_array).into_iter().flatten() {
        if let Some(k) = r.as_str().filter(|k| !o.contains_key(*k)) {
            e.push(format!("{p}: falta o campo `{k}`"));
        }
    }
    for (k, val) in o {
        match props.and_then(|pr| pr.get(k)) {
            Some(sub) => visitar(sub, val, &format!("{p}.{k}"), d + 1, e),
            None if s.get("additionalProperties") == Some(&Value::Bool(false)) => {
                e.push(format!("{p}: campo `{k}` não previsto"));
            }
            None => {}
        }
    }
}

/// Array: `maxItems` e `items`. **Onde:** [`visitar`].
fn checar_array(s: &Value, a: &[Value], p: &str, d: usize, e: &mut Vec<String>) {
    if s.get("maxItems").and_then(Value::as_u64).is_some_and(|m| a.len() as u64 > m) {
        e.push(format!("{p}: {} itens acima do máximo", a.len()));
    }
    if let Some(it) = s.get("items") {
        for (i, x) in a.iter().enumerate() {
            visitar(it, x, &format!("{p}[{i}]"), d + 1, e);
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;

    fn s() -> Value {
        json!({"type":"object","required":["a"],"additionalProperties":false,
          "properties":{"a":{"type":"string","maxLength":3},
                        "n":{"type":"integer","minimum":0,"maximum":10},
                        "l":{"type":"array","maxItems":2,"items":{"enum":["x","y"]}}}})
    }

    #[test]
    fn aceita_valido_e_lista_todas_as_violacoes() {
        assert!(validar(&s(), &json!({"a":"abc","n":10,"l":["x"]})).is_ok());
        let e = validar(&s(), &json!({"a":"abcd","n":11,"l":["x","y","z"],"z":1})).unwrap_err();
        assert!(e.len() >= 5, "{e:?}");
        assert!(validar(&s(), &json!({})).unwrap_err()[0].contains("falta"));
    }

    #[test]
    fn tipos_hostis_e_unicode() {
        assert!(validar(&s(), &json!({"a":1})).is_err());
        assert!(validar(&s(), &json!({"a":"x","n":1.5})).is_err(), "float não é integer");
        assert!(validar(&s(), &json!({"a":"ççç"})).is_ok(), "3 caracteres, 6 bytes");
        assert!(validar(&s(), &json!([])).is_err());
        assert!(validar(&s(), &Value::Null).is_err());
    }

    #[test]
    fn aninhamento_profundo_nao_estoura() {
        let mut sc = json!({"type":"string"});
        let mut v = json!("x");
        for _ in 0..200 {
            sc = json!({"type":"array","items":sc});
            v = json!([v]);
        }
        assert!(validar(&sc, &v).unwrap_err()[0].contains("profundidade"));
    }
}
