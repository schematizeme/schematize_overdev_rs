//! Tabela de preços do ledger configurável por arquivo (`.overdev/precos.toml`).
//!
//! **O quê:** `TabelaPrecos` com família (substring do id do modelo), preço de entrada e saída em
//! USD por milhão de tokens (inteiros). O PADRÃO é a estimativa embutida de `ledger`
//! (`estimativa = true`, versão `PRECOS_VERSAO`); arquivo ausente = padrão; arquivo inválido = ERRO
//! (nunca cai em silêncio no padrão, para não esconder preço errado).
//! **Onde:** quem calcula custo para o ledger usa [`TabelaPrecos::custo_micro_usd`] em vez da
//! função com tabela fixa quando o projeto tem `precos.toml`.
//!
//! Formato:
//! ```toml
//! versao = "2026-11-22"
//! [[modelo]]
//! familia = "haiku"
//! input = 1
//! output = 5
//! ```

use super::ledger::{custo_com_preco, Preco, Uso, PRECOS_VERSAO};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Teto de sanidade por preço (USD por milhão de tokens); evita overflow e typo de unidade.
const PRECO_MAX: u64 = 100_000;

/// Tabela de preços.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabelaPrecos {
    /// Versão da tabela (vai ao relatório).
    pub versao: String,
    /// `true` = valores embutidos, sem conferência com a página de preços.
    pub estimativa: bool,
    /// (família em minúsculas, preço), na ordem do arquivo (primeira que casar vence).
    pub entradas: Vec<(String, Preco)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arquivo {
    versao: String,
    #[serde(default)]
    modelo: Vec<Modelo>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Modelo {
    familia: String,
    input: u64,
    output: u64,
}

/// Tabela padrão (estimativa embutida). **Onde:** sem arquivo de preços.
pub fn padrao() -> TabelaPrecos {
    let e = |f: &str, input, output| (f.to_string(), Preco { input, output });
    TabelaPrecos {
        versao: PRECOS_VERSAO.to_string(),
        estimativa: true,
        entradas: vec![e("haiku", 1, 5), e("sonnet", 3, 15), e("opus", 5, 25)],
    }
}

/// Lê a tabela de um texto TOML. **O quê:** família vazia, campo desconhecido, número negativo
/// ou tabela sem modelo = `Err`. **Onde:** [`carregar`].
pub fn parse_toml(txt: &str) -> Result<TabelaPrecos, String> {
    let a: Arquivo = toml::from_str(txt).map_err(|e| format!("precos.toml invalido: {e}"))?;
    if a.modelo.is_empty() {
        return Err("precos.toml sem nenhum [[modelo]]".into());
    }
    let mut entradas = Vec::new();
    for m in a.modelo {
        let f = m.familia.trim().to_ascii_lowercase();
        if f.is_empty() {
            return Err("precos.toml: `familia` vazia".into());
        }
        if m.input > PRECO_MAX || m.output > PRECO_MAX {
            return Err(format!("precos.toml: preco de `{f}` acima de {PRECO_MAX} USD/Mtok"));
        }
        entradas.push((f, Preco { input: m.input, output: m.output }));
    }
    Ok(TabelaPrecos { versao: a.versao, estimativa: false, entradas })
}

/// Caminho do arquivo de preços do projeto. **Onde:** [`carregar_do_projeto`].
pub fn caminho(projeto: &Path) -> PathBuf {
    crate::paths::overdev_dir_at(projeto).join("precos.toml")
}

/// Carrega de `caminho`: ausente = padrão; ilegível ou inválido = `Err`. **Onde:** orquestrador.
pub fn carregar(caminho: &Path) -> Result<TabelaPrecos, String> {
    match std::fs::read_to_string(caminho) {
        Ok(t) => parse_toml(&t).map_err(|e| format!("{}: {e}", caminho.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(padrao()),
        Err(e) => Err(format!("ler {}: {e}", caminho.display())),
    }
}

/// [`carregar`] no `precos.toml` do projeto. **Onde:** início do run.
pub fn carregar_do_projeto(projeto: &Path) -> Result<TabelaPrecos, String> {
    carregar(&caminho(projeto))
}

impl TabelaPrecos {
    /// Preço do modelo pela primeira família que casa por substring (sem caixa). Desconhecido = `None`.
    pub fn preco(&self, modelo: &str) -> Option<Preco> {
        let m = modelo.to_ascii_lowercase();
        self.entradas.iter().find(|(f, _)| m.contains(f.as_str())).map(|(_, p)| *p)
    }

    /// Custo em micro-USD com esta tabela (mesma fórmula do ledger). `None` = modelo sem preço.
    pub fn custo_micro_usd(&self, modelo: &str, u: &Uso) -> Option<u64> {
        self.preco(modelo).map(|p| custo_com_preco(p, u))
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::otimiza::ledger::custo_micro_usd;

    #[test]
    fn padrao_e_estimativa_e_bate_com_o_ledger() {
        let t = padrao();
        assert!(t.estimativa && t.versao == PRECOS_VERSAO);
        let u = Uso {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 1_000_000,
            cache_write: 1_000_000,
        };
        for m in ["claude-haiku-5", "claude-sonnet-5", "OPUS"] {
            assert_eq!(t.custo_micro_usd(m, &u), custo_micro_usd(m, &u), "{m}");
        }
        assert_eq!(t.custo_micro_usd("x", &u), None);
    }

    #[test]
    fn arquivo_sobrescreve_e_ordem_vale() {
        let t = parse_toml("versao=\"v2\"\n[[modelo]]\nfamilia=\"Sonnet-5\"\ninput=2\noutput=10\n[[modelo]]\nfamilia=\"sonnet\"\ninput=9\noutput=9\n").unwrap();
        assert!(!t.estimativa);
        let u = Uso { input: 1_000_000, ..Uso::default() };
        assert_eq!(t.custo_micro_usd("claude-sonnet-5", &u), Some(2_000_000));
        assert_eq!(t.custo_micro_usd("claude-sonnet-4", &u), Some(9_000_000));
    }

    #[test]
    fn invalido_nunca_vira_padrao_em_silencio() {
        for ruim in [
            "",
            "versao=\"x\"",
            "versao=\"x\"\n[[modelo]]\nfamilia=\"\"\ninput=1\noutput=1",
            "versao=\"x\"\n[[modelo]]\nfamilia=\"a\"\ninput=-1\noutput=1",
            "versao=\"x\"\n[[modelo]]\nfamilia=\"a\"\ninput=1.5\noutput=1",
            "versao=\"x\"\nextra=1\n[[modelo]]\nfamilia=\"a\"\ninput=1\noutput=1",
            "versao=\"x\"\n[[modelo]]\nfamilia=\"a\"\ninput=18446744073709551615\noutput=1",
            "\u{0}lixo",
        ] {
            assert!(parse_toml(ruim).is_err(), "{ruim:?}");
        }
        let d = std::env::temp_dir().join(format!("otz-precos-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(carregar(&d.join("nao-existe.toml")).unwrap(), padrao());
        std::fs::write(d.join("p.toml"), "quebrado =").unwrap();
        assert!(carregar(&d.join("p.toml")).unwrap_err().contains("p.toml"));
        std::fs::remove_dir_all(&d).unwrap();
    }
}
