//! Eval dourado e sweep de modelo/effort (IA-17) — relatório e roteamento determinísticos.
//!
//! **O quê:** agrega amostras (unidade, classe, modelo, effort, aprovada, custo) em US$ por
//! unidade aprovada e taxa de aprovação, e DERIVA a tabela classe -> (modelo, effort) do relatório
//! (nunca à mão). A varredura real (rodar variações) passa por [`ExecutorVariacao`]: o padrão
//! [`ExecutorNaoConfigurado`] recusa, porque rodar unidades de verdade gasta assinatura.
//! **Onde:** ferramenta offline sobre o ledger; a tabela alimenta o roteador (`cascata`).
//! Toda aritmética é inteira (micro-USD, pontos-base) e a ordenação é total: mesma entrada,
//! mesmo relatório, byte a byte.

use super::ledger::Registro;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Uma unidade histórica rodada com (modelo, effort).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Amostra {
    /// Id da unidade.
    pub unidade: String,
    /// Classe (`mecanica`, `codigo`, ...).
    pub classe: String,
    /// Modelo.
    pub modelo: String,
    /// Effort.
    pub effort: String,
    /// Passou no gate?
    pub aprovada: bool,
    /// Custo da tentativa em micro-USD.
    pub custo_micro_usd: u64,
}

/// Linha do relatório: um (classe, modelo, effort).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Linha {
    /// Classe.
    pub classe: String,
    /// Modelo.
    pub modelo: String,
    /// Effort.
    pub effort: String,
    /// Amostras.
    pub n: u64,
    /// Aprovadas.
    pub aprovadas: u64,
    /// Taxa de aprovação em pontos-base (10000 = 100%).
    pub taxa_bp: u64,
    /// Custo total (micro-USD).
    pub custo_total: u64,
    /// Custo total / aprovadas; `None` se nenhuma aprovada.
    pub custo_por_aprovada: Option<u64>,
}

/// Lê amostras JSONL tolerando linhas ruins: devolve (válidas, nº de ruins). **Onde:** fixtures.
pub fn ler_jsonl(texto: &str) -> (Vec<Amostra>, usize) {
    let mut ok = Vec::new();
    let mut ruins = 0usize;
    for l in texto.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Amostra>(l) {
            Ok(a) => ok.push(a),
            Err(_) => ruins += 1,
        }
    }
    (ok, ruins)
}

/// Converte registros do ledger em amostras. `done` = aprovada; `classe_de(unidade)` vem de fora
/// (o ledger não guarda classe); effort do ledger é `"-"` (não registrado).
/// **Onde:** montagem do dataset a partir de unidades históricas.
pub fn de_ledger(rs: &[Registro], classe_de: impl Fn(&str) -> String) -> Vec<Amostra> {
    rs.iter()
        .map(|r| Amostra {
            unidade: r.unidade.clone(),
            classe: classe_de(&r.unidade),
            modelo: r.modelo.clone(),
            effort: "-".into(),
            aprovada: r.estado == "done",
            custo_micro_usd: r.custo_micro_usd.unwrap_or(0),
        })
        .collect()
}

/// Agrega as amostras. **O quê:** ordem final: classe, custo/aprovada crescente (sem aprovada
/// por último), modelo, effort. **Onde:** relatório e [`tabela_roteamento`].
pub fn relatorio(amostras: &[Amostra]) -> Vec<Linha> {
    let mut m: BTreeMap<(String, String, String), (u64, u64, u64)> = BTreeMap::new();
    for a in amostras {
        let e = m.entry((a.classe.clone(), a.modelo.clone(), a.effort.clone())).or_default();
        e.0 += 1;
        e.1 += u64::from(a.aprovada);
        e.2 = e.2.saturating_add(a.custo_micro_usd);
    }
    let mut ls: Vec<Linha> = m
        .into_iter()
        .map(|((classe, modelo, effort), (n, ap, c))| Linha {
            classe,
            modelo,
            effort,
            n,
            aprovadas: ap,
            taxa_bp: ap * 10_000 / n.max(1),
            custo_total: c,
            custo_por_aprovada: (ap > 0).then(|| c / ap),
        })
        .collect();
    ls.sort_by(|a, b| {
        let k = |l: &Linha| (l.classe.clone(), l.custo_por_aprovada.unwrap_or(u64::MAX));
        k(a).cmp(&k(b)).then_with(|| (&a.modelo, &a.effort).cmp(&(&b.modelo, &b.effort)))
    });
    ls
}

/// Deriva classe -> (modelo, effort): o de menor US$/aprovada entre os que têm `n >= n_min` e
/// taxa >= `taxa_min_bp`. Classe sem candidato fica FORA (o roteador mantém o padrão da casa).
/// **Onde:** alimenta o roteador; derivada do relatório, nunca à mão.
pub fn tabela_roteamento(
    ls: &[Linha],
    taxa_min_bp: u64,
    n_min: u64,
) -> BTreeMap<String, (String, String)> {
    let mut t = BTreeMap::new();
    for l in ls {
        let elegivel = l.n >= n_min && l.taxa_bp >= taxa_min_bp && l.custo_por_aprovada.is_some();
        if elegivel && !t.contains_key(&l.classe) {
            t.insert(l.classe.clone(), (l.modelo.clone(), l.effort.clone()));
        }
    }
    t
}

/// Renderiza relatório + roteamento em Markdown determinístico. **Onde:** arquivo do archive.
pub fn render_md(ls: &[Linha], rot: &BTreeMap<String, (String, String)>) -> String {
    let mut s = String::from("| classe | modelo | effort | n | aprovadas | taxa % | custo total (micro-USD) | por aprovada |\n|---|---|---|---|---|---|---|---|\n");
    for l in ls {
        let p = l.custo_por_aprovada.map_or("-".to_string(), |c| c.to_string());
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {}.{:02} | {} | {} |\n",
            l.classe,
            l.modelo,
            l.effort,
            l.n,
            l.aprovadas,
            l.taxa_bp / 100,
            l.taxa_bp % 100,
            l.custo_total,
            p
        ));
    }
    s.push_str("\nRoteamento derivado:\n");
    for (c, (m, e)) in rot {
        s.push_str(&format!("- {c} -> {m} / {e}\n"));
    }
    s
}

/// Quem roda uma unidade do dataset com (modelo, effort) e devolve a amostra medida.
/// **Onde:** a varredura; a implementação real (lançador `claude -p` + gate) é decisão do operador.
pub trait ExecutorVariacao {
    /// Roda `base` com a variação e devolve a amostra resultante.
    fn rodar(&mut self, base: &Amostra, modelo: &str, effort: &str) -> Result<Amostra, String>;
}

/// Executor padrão: recusa. **O quê:** a varredura real roda unidades de verdade (custo de
/// assinatura), então só acontece com um executor ligado de propósito. **Onde:** padrão de [`varrer`].
#[derive(Debug, Default, Clone, Copy)]
pub struct ExecutorNaoConfigurado;

impl ExecutorVariacao for ExecutorNaoConfigurado {
    fn rodar(&mut self, _b: &Amostra, _m: &str, _e: &str) -> Result<Amostra, String> {
        Err("varredura real nao configurada: ligue um ExecutorVariacao explicito (roda unidades de verdade e gasta assinatura); o relatorio offline funciona sem ele".into())
    }
}

/// Roda cada unidade do dataset em cada variação (modelo, effort). Para no primeiro erro.
///
/// **O quê:** ordem estável (unidade, depois variação). **Onde:** ferramenta de sweep.
pub fn varrer(
    dataset: &[Amostra],
    variacoes: &[(String, String)],
    ex: &mut dyn ExecutorVariacao,
) -> Result<Vec<Amostra>, String> {
    let mut out = Vec::new();
    for base in dataset {
        for (m, e) in variacoes {
            out.push(ex.rodar(base, m, e).map_err(|x| format!("{}: {x}", base.unidade))?);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn a(u: &str, c: &str, m: &str, e: &str, ok: bool, custo: u64) -> Amostra {
        Amostra {
            unidade: u.into(),
            classe: c.into(),
            modelo: m.into(),
            effort: e.into(),
            aprovada: ok,
            custo_micro_usd: custo,
        }
    }

    fn fixture() -> Vec<Amostra> {
        vec![
            a("u1", "codigo", "haiku", "low", false, 100),
            a("u2", "codigo", "haiku", "low", true, 100),
            a("u1", "codigo", "sonnet", "low", true, 500),
            a("u2", "codigo", "sonnet", "low", true, 500),
            a("u1", "codigo", "opus", "high", true, 2500),
            a("u3", "mecanica", "haiku", "low", true, 50),
        ]
    }

    #[test]
    fn relatorio_deterministico_e_roteamento_derivado() {
        let r = relatorio(&fixture());
        assert_eq!(r, relatorio(&fixture()));
        let h = r.iter().find(|l| l.modelo == "haiku" && l.classe == "codigo").unwrap();
        assert_eq!(
            (h.n, h.aprovadas, h.taxa_bp, h.custo_total, h.custo_por_aprovada),
            (2, 1, 5000, 200, Some(200))
        );
        let t = tabela_roteamento(&r, 9000, 2);
        assert_eq!(t["codigo"], ("sonnet".to_string(), "low".to_string()), "haiku nao atinge 90%");
        assert!(!t.contains_key("mecanica"), "n=1 < n_min: sem candidato");
        let t2 = tabela_roteamento(&r, 5000, 1);
        assert_eq!(t2["mecanica"].0, "haiku");
        assert_eq!(t2["codigo"].0, "haiku", "com taxa menor exigida, o mais barato por aprovada");
        let md = render_md(&r, &t);
        assert_eq!(md, render_md(&relatorio(&fixture()), &t));
        assert!(md.contains("- codigo -> sonnet / low") && md.contains("50.00"));
    }

    #[test]
    fn jsonl_hostil_ledger_e_variacao_recusada() {
        let (ok, ruins) = ler_jsonl("{\"x\":1}\n\nlixo\n");
        assert_eq!((ok.len(), ruins), (0, 2));
        let j = serde_json::to_string(&fixture()[0]).unwrap();
        assert_eq!(ler_jsonl(&format!("{j}\n{j}")).0.len(), 2);
        assert!(relatorio(&[]).is_empty());
        let nada = a("u", "c", "m", "e", false, 9);
        assert_eq!(relatorio(&[nada])[0].custo_por_aprovada, None);
        let r = Registro {
            unidade: "ü".into(),
            onda: 1,
            modelo: "haiku".into(),
            estado: "done".into(),
            uso: Default::default(),
            custo_micro_usd: None,
        };
        let am = de_ledger(&[r], |_| "codigo".into());
        assert!(am[0].aprovada && am[0].custo_micro_usd == 0);
        let v = varrer(&fixture(), &[("haiku".into(), "low".into())], &mut ExecutorNaoConfigurado);
        assert!(v.unwrap_err().contains("nao configurada"));
        assert!(varrer(&[], &[("m".into(), "e".into())], &mut ExecutorNaoConfigurado)
            .unwrap()
            .is_empty());
    }
}
