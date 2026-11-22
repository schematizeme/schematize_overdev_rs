//! Ledger de custo/tokens por UNIDADE (IA-01, IA-04, IA-20), em `.overdev/ledger.jsonl`.
//!
//! **O quê:** registro append-only (uma linha JSON por tentativa), custo estimado em
//! micro-USD (inteiros, sem float) com tabela de preços versionada, e agregação por onda/modelo.
//! **Onde:** orquestrador grava ao fim de cada tentativa; relatório de 30 min agrega.
//! Formato do usage: o mesmo objeto `usage` da API que `src/usage.rs` já lê do transcript
//! (`input_tokens`, `output_tokens`, `cache_read_input_tokens`, `cache_creation_input_tokens`).
//! TODO: parser do `result` do `claude -p --output-format json` (`total_cost_usd`) não é
//! observável neste repo; não inventado aqui.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

/// Versão da tabela de preços. ESTIMATIVA: conferir na página de pricing antes de metas de custo.
pub const PRECOS_VERSAO: &str = "2026-11-20-estimativa";

/// Tokens de uma tentativa.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Uso {
    /// Entrada sem cache.
    pub input: u64,
    /// Saída.
    pub output: u64,
    /// Leitura de cache.
    pub cache_read: u64,
    /// Escrita de cache.
    pub cache_write: u64,
}

impl Uso {
    /// Soma saturando. **Onde:** agregação.
    pub fn somar(&mut self, o: &Uso) {
        self.input = self.input.saturating_add(o.input);
        self.output = self.output.saturating_add(o.output);
        self.cache_read = self.cache_read.saturating_add(o.cache_read);
        self.cache_write = self.cache_write.saturating_add(o.cache_write);
    }

    /// Taxa de cache `read / (read + write + input)`; `None` se o denominador é 0 (IA-04).
    /// **Onde:** alerta de regressão de cache.
    pub fn taxa_cache(&self) -> Option<f64> {
        let den = self.cache_read.saturating_add(self.cache_write).saturating_add(self.input);
        (den > 0).then(|| self.cache_read as f64 / den as f64)
    }
}

/// Lê um objeto `usage`. Campo ausente, negativo, float ou de tipo errado vira 0 (nunca derruba).
/// **Onde:** ao registrar a tentativa a partir de JSON de resultado/transcript.
pub fn uso_de_json(v: &Value) -> Uso {
    let g = |k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
    Uso {
        input: g("input_tokens"),
        output: g("output_tokens"),
        cache_read: g("cache_read_input_tokens"),
        cache_write: g("cache_creation_input_tokens"),
    }
}

/// Preço em USD por milhão de tokens (= micro-USD por token).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preco {
    /// Entrada.
    pub input: u64,
    /// Saída.
    pub output: u64,
}

/// Preço por família, casando por substring do id (haiku/sonnet/opus). Desconhecido = `None`.
/// **Onde:** `custo_micro_usd`.
pub fn preco_do_modelo(modelo: &str) -> Option<Preco> {
    let m = modelo.to_ascii_lowercase();
    if m.contains("haiku") {
        Some(Preco { input: 1, output: 5 })
    } else if m.contains("sonnet") {
        Some(Preco { input: 3, output: 15 })
    } else if m.contains("opus") {
        Some(Preco { input: 5, output: 25 })
    } else {
        None
    }
}

/// Custo estimado em micro-USD: leitura de cache 0,1x e escrita 1,25x do preço de entrada.
/// `None` para modelo desconhecido (não chuta custo). Satura em `u64::MAX`.
/// **Onde:** ao montar o `Registro`.
pub fn custo_micro_usd(modelo: &str, u: &Uso) -> Option<u64> {
    preco_do_modelo(modelo).map(|p| custo_com_preco(p, u))
}

/// Custo em micro-USD para um preço dado (mesma fórmula de [`custo_micro_usd`]); satura em `u64::MAX`.
/// **Onde:** `custo_micro_usd` (tabela embutida) e `precos::TabelaPrecos` (tabela do arquivo).
pub fn custo_com_preco(p: Preco, u: &Uso) -> u64 {
    let (i, o) = (u128::from(p.input), u128::from(p.output));
    let tot = u128::from(u.input) * i * 100
        + u128::from(u.output) * o * 100
        + u128::from(u.cache_read) * i * 10
        + u128::from(u.cache_write) * i * 125;
    u64::try_from(tot / 100).unwrap_or(u64::MAX)
}

/// Uma linha do ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registro {
    /// Id da unidade.
    pub unidade: String,
    /// Número da onda.
    pub onda: u32,
    /// Modelo.
    pub modelo: String,
    /// Estado final (`done`, `failed:loop`, `killed:oom`...).
    pub estado: String,
    /// Tokens.
    pub uso: Uso,
    /// Custo estimado (micro-USD); `None` se modelo sem preço.
    pub custo_micro_usd: Option<u64>,
}

/// Anexa um registro (uma linha). Cria o diretório pai se faltar. **Onde:** fim de cada tentativa.
pub fn anexar(caminho: &Path, r: &Registro) -> Result<(), String> {
    if let Some(dir) = caminho.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("criar {}: {e}", dir.display()))?;
    }
    let mut linha = serde_json::to_string(r).map_err(|e| e.to_string())?;
    linha.push('\n');
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(caminho)
        .map_err(|e| format!("abrir {}: {e}", caminho.display()))?;
    f.write_all(linha.as_bytes()).map_err(|e| format!("gravar ledger: {e}"))
}

/// Lê o ledger tolerando linhas corrompidas/truncadas: devolve (válidos, nº de linhas ruins).
/// Arquivo ausente = vazio. **Onde:** relatório, retomada.
pub fn ler(caminho: &Path) -> Result<(Vec<Registro>, usize), String> {
    let bytes = match std::fs::read(caminho) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((vec![], 0)),
        Err(e) => return Err(format!("ler {}: {e}", caminho.display())),
    };
    Ok(parse_linhas(&String::from_utf8_lossy(&bytes)))
}

/// Parse puro das linhas do ledger (linha vazia ignorada; inválida conta como ruim).
/// **Onde:** `ler` e testes.
pub fn parse_linhas(texto: &str) -> (Vec<Registro>, usize) {
    let mut ok = Vec::new();
    let mut ruins = 0usize;
    for l in texto.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Registro>(l) {
            Ok(r) => ok.push(r),
            Err(_) => ruins = ruins.saturating_add(1),
        }
    }
    (ok, ruins)
}

/// Totais agregados.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Agregado {
    /// Tentativas.
    pub tentativas: u64,
    /// Tokens somados.
    pub uso: Uso,
    /// Custo somado (micro-USD, só os com preço).
    pub custo_micro_usd: u64,
    /// Custo gasto em tentativas não `done` (desperdício, IA-20).
    pub custo_desperdicado: u64,
}

fn agregar_por<K: Ord>(rs: &[Registro], chave: impl Fn(&Registro) -> K) -> BTreeMap<K, Agregado> {
    let mut m: BTreeMap<K, Agregado> = BTreeMap::new();
    for r in rs {
        let a = m.entry(chave(r)).or_default();
        let c = r.custo_micro_usd.unwrap_or(0);
        a.tentativas = a.tentativas.saturating_add(1);
        a.uso.somar(&r.uso);
        a.custo_micro_usd = a.custo_micro_usd.saturating_add(c);
        if r.estado != "done" {
            a.custo_desperdicado = a.custo_desperdicado.saturating_add(c);
        }
    }
    m
}

/// Agrega por onda. **Onde:** painel por onda (IA-20).
pub fn por_onda(rs: &[Registro]) -> BTreeMap<u32, Agregado> {
    agregar_por(rs, |r| r.onda)
}

/// Agrega por modelo. **Onde:** painel por modelo (IA-20).
pub fn por_modelo(rs: &[Registro]) -> BTreeMap<String, Agregado> {
    agregar_por(rs, |r| r.modelo.clone())
}

#[cfg(test)]
mod testes {
    use super::*;

    fn reg(u: &str, onda: u32, modelo: &str, estado: &str, input: u64) -> Registro {
        let uso = Uso { input, ..Uso::default() };
        Registro {
            unidade: u.into(),
            onda,
            modelo: modelo.into(),
            estado: estado.into(),
            uso,
            custo_micro_usd: custo_micro_usd(modelo, &uso),
        }
    }

    #[test]
    fn usage_hostil_vira_zero() {
        let v: Value = serde_json::from_str(
            r#"{"input_tokens":-5,"output_tokens":"x","cache_read_input_tokens":1.5}"#,
        )
        .unwrap();
        assert_eq!(uso_de_json(&v), Uso::default());
        assert_eq!(uso_de_json(&Value::Null), Uso::default());
        let v: Value = serde_json::from_str(r#"{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":100,"cache_creation_input_tokens":7}"#).unwrap();
        assert_eq!(uso_de_json(&v), Uso { input: 10, output: 5, cache_read: 100, cache_write: 7 });
    }

    #[test]
    fn custo_e_overflow() {
        let u = Uso {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 1_000_000,
            cache_write: 1_000_000,
        };
        // sonnet: 3 + 15 + 0,3 + 3,75 USD = 22,05 USD
        assert_eq!(custo_micro_usd("claude-sonnet-5", &u), Some(22_050_000));
        assert_eq!(custo_micro_usd("modelo-x", &u), None);
        let m =
            Uso { input: u64::MAX, output: u64::MAX, cache_read: u64::MAX, cache_write: u64::MAX };
        assert_eq!(custo_micro_usd("OPUS", &m), Some(u64::MAX));
        let mut a = m;
        a.somar(&m);
        assert_eq!(a.input, u64::MAX);
    }

    #[test]
    fn taxa_cache() {
        assert_eq!(Uso::default().taxa_cache(), None);
        let u = Uso { input: 10, cache_read: 80, cache_write: 10, output: 999 };
        assert_eq!(u.taxa_cache(), Some(0.8));
    }

    #[test]
    fn jsonl_corrompido_linha_a_linha() {
        let ok = serde_json::to_string(&reg("u1", 1, "haiku", "done", 5)).unwrap();
        let txt = format!("{ok}\nlixo\n\n{{\"unidade\":\"trunc\n{ok}\n{}", &ok[..ok.len() / 2]);
        let (rs, ruins) = parse_linhas(&txt);
        assert_eq!(rs.len(), 2);
        assert_eq!(ruins, 3);
        assert_eq!(parse_linhas(""), (vec![], 0));
    }

    #[test]
    fn agregacao_e_desperdicio() {
        let rs = vec![
            reg("a", 1, "claude-haiku-5", "done", 1_000_000),
            reg("b", 1, "claude-sonnet-5", "killed:oom", 1_000_000),
            reg("b", 2, "claude-sonnet-5", "done", 1_000_000),
            reg("ü", 2, "desconhecido", "done", 7),
        ];
        let o = por_onda(&rs);
        assert_eq!(o[&1].custo_micro_usd, 4_000_000);
        assert_eq!(o[&1].custo_desperdicado, 3_000_000);
        assert_eq!(o[&2].tentativas, 2);
        assert_eq!(por_modelo(&rs)["desconhecido"].custo_micro_usd, 0);
        assert!(por_onda(&[]).is_empty());
    }

    #[test]
    fn anexar_e_ler_roundtrip() {
        let dir = std::env::temp_dir().join(format!("otz-led-{}", std::process::id()));
        let p = dir.join(".overdev/ledger.jsonl");
        assert_eq!(ler(&p).unwrap(), (vec![], 0));
        let r = reg("ação", 3, "claude-opus-5", "failed:loop", 42);
        anexar(&p, &r).unwrap();
        anexar(&p, &r).unwrap();
        assert_eq!(ler(&p).unwrap().0, vec![r.clone(), r]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
