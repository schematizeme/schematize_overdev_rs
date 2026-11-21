//! Renderização do `context.md` e do `checklist.md` a partir da [`Coleta`] (e das notas do Claude).
//!
//! **O quê:** texto Markdown no formato de handoff da casa. O corpo ESTÁVEL (sem hora) é o que
//! entra no hash: só há reescrita quando ele muda. A primeira linha (`> Atualizado: ...`) é
//! acrescentada na hora de gravar por [`com_hora`].
//! **Onde:** [`super::ciclo::Motor`].

use super::coleta::Coleta;
use serde::{Deserialize, Serialize};

/// Notas do Claude persistidas no estado e reimpressas em todo handoff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notas {
    /// `YYYY-MM-DD HH:MM` local de quando foram geradas.
    pub quando: String,
    /// Texto já validado e redigido.
    pub texto: String,
}

/// **O quê:** `## Título` + linhas em lista (ou `_nenhum_`). **Onde:** [`contexto`], [`checklist`].
fn secao(t: &mut String, titulo: &str, itens: &[String]) {
    t.push_str(&format!("\n## {titulo}\n\n"));
    if itens.is_empty() {
        t.push_str("_nenhum_\n");
    }
    for i in itens {
        t.push_str(&format!("- {i}\n"));
    }
}

/// **O quê:** corpo estável do `context.md`. **Onde:** tick mecânico (hash e gravação). PURA.
pub fn contexto(c: &Coleta, notas: Option<&Notas>) -> String {
    let mut t = format!("# Handoff vivo — {}\n\n", c.projeto);
    t.push_str("> Gerado pela camada mecânica (sem Claude): estado determinístico do workspace.\n");
    t.push_str("> Retomada: leia `ULTIMO-checklist.md`, depois os repos sujos abaixo.\n");
    let repos: Vec<String> = c
        .repos
        .iter()
        .map(|r| {
            let sujo =
                if r.n_sujos == 0 { "limpo".to_string() } else { format!("{} sujo(s)", r.n_sujos) };
            format!(
                "`{}` — {} @ {} — {sujo}",
                r.nome,
                r.branch,
                if r.head.is_empty() { "(sem commit)" } else { &r.head }
            )
        })
        .collect();
    secao(&mut t, "Repos do workspace (git)", &repos);
    let sujos: Vec<String> = c
        .repos
        .iter()
        .flat_map(|r| r.sujos.iter().map(move |s| format!("`{}`: `{s}`", r.nome)))
        .collect();
    secao(&mut t, "Mudanças não commitadas", &sujos);
    secao(
        &mut t,
        "Arquivos tocados recentemente",
        &c.tocados.iter().map(|s| format!("`{s}`")).collect::<Vec<_>>(),
    );
    secao(&mut t, "Unidades em voo (`.overdev/estado/`)", &c.em_voo);
    secao(&mut t, "Vereditos recentes das jaulas", &c.vereditos);
    secao(&mut t, "Último relatório", &c.relatorio.iter().cloned().collect::<Vec<_>>());
    if !c.avisos.is_empty() {
        secao(&mut t, "Avisos da coleta", &c.avisos);
    }
    if let Some(n) = notas {
        t.push_str(&format!("\n## Notas do Claude ({})\n\n{}\n", n.quando, n.texto));
    }
    t
}

/// **O quê:** corpo estável do `checklist.md` (FEITO vs EM ABERTO, `[~]` parkeados). **Onde:**
/// tick mecânico. PURA.
pub fn checklist(c: &Coleta) -> String {
    let k = &c.checklist;
    let mut t = format!("# Checklist vivo — {}\n\n", c.projeto);
    if !k.existe {
        t.push_str("_Sem CHECKLIST do overdev neste workspace._\n");
        return t;
    }
    t.push_str(&format!(
        "Resumo: {} feito(s), {} em aberto, {} parkeado(s) `[~]`.\n",
        k.feitos, k.n_abertos, k.n_parkeados
    ));
    let marca = |v: &[String], m: &str| v.iter().map(|x| format!("{m} {x}")).collect::<Vec<_>>();
    secao(&mut t, "FEITO (últimos)", &marca(&k.ultimos_feitos, "[x]"));
    secao(&mut t, "EM ABERTO", &marca(&k.abertos, "[ ]"));
    secao(&mut t, "PARKEADOS", &marca(&k.parkeados, "[~]"));
    t
}

/// **O quê:** prefixa a linha de hora ao corpo estável (não entra no hash). **Onde:** gravação.
pub fn com_hora(quando: &str, estavel: &str) -> String {
    format!("> Atualizado: {quando}\n\n{estavel}")
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn contexto_e_estavel_e_inclui_notas() {
        let c = Coleta { projeto: "p".into(), ..Coleta::default() };
        assert_eq!(contexto(&c, None), contexto(&c, None));
        let n = Notas { quando: "2026-11-21 10:00".into(), texto: "decidi X".into() };
        let t = contexto(&c, Some(&n));
        assert!(t.contains("## Notas do Claude (2026-11-21 10:00)") && t.contains("decidi X"));
        assert!(checklist(&c).contains("Sem CHECKLIST"));
    }
}
