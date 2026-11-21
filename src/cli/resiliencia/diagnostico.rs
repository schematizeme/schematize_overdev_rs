//! `overdev sistema diag` e `overdev memoria status`: leitura pura, NUNCA altera o sistema.
//!
//! **O quê:** `sistema diag` imprime o `Perfil` do host com os achados acionáveis (texto em pt/en
//! ou JSON); `memoria status` mostra a leitura atual do host, a decisão do governador agora e os
//! tetos recomendados por tipo de tarefa (do `picos.hist`).
//! **Onde:** `cli::resiliencia::{sistema, memoria}`.

use crate::resiliencia::controle::{politica_para, Controle};
use schematize_plataforma::memoria::{Decisao, FonteDir, PedidoAdmissao};
use schematize_plataforma::sistema::{identificar_host, Idioma};
use std::path::PathBuf;
use std::sync::Arc;

/// **O quê:** idioma do texto: a flag, senão `SCHEMATIZE_LANG`/`LC_ALL`/`LANG` (`pt*` = português),
/// senão inglês. **Onde:** [`sistema_diag`]. PURA com o ambiente injetado.
pub fn idioma(flag: Option<&str>, ambiente: Option<&str>) -> Idioma {
    let txt = flag.or(ambiente).unwrap_or("").to_lowercase();
    if txt.starts_with("pt") {
        Idioma::Pt
    } else {
        Idioma::En
    }
}

/// **O quê:** o comando `sistema diag`. **Onde:** `cli::resiliencia::sistema`.
pub fn sistema_diag(json: bool, lang: Option<&str>) -> Result<(), String> {
    let p = identificar_host();
    if json {
        println!("{}", p.to_json());
        return Ok(());
    }
    let env = ["SCHEMATIZE_LANG", "LC_ALL", "LANG"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()));
    println!("{}", p.resumo(idioma(lang, env.as_deref())));
    Ok(())
}

/// **O quê:** o comando `memoria status`. **Onde:** `cli::resiliencia::memoria`.
pub fn memoria_status(json: bool, estado: Option<PathBuf>) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| format!("cwd inacessível: {e}"))?;
    let od = estado
        .and_then(|e| e.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| crate::paths::overdev_dir_at(&cwd));
    let perfil = identificar_host();
    let m = perfil.margem_segura();
    let pol = politica_para(8, m.piso_critico_mib);
    let mut ctl = Controle::novo(
        pol,
        Arc::new(FonteDir::real()),
        Some(od.join("picos.hist")),
        m.folga_cgroup_mib,
    );
    let fila: Vec<PedidoAdmissao> =
        (0..8).map(|i| PedidoAdmissao { id: format!("sonda{i}"), teto_mib: 500 }).collect();
    let (decisoes, h) = ctl.decidir(0, &[], &fila);
    let admitiria = decisoes.iter().find_map(|d| match d {
        Decisao::Admitir { quantas } => Some(*quantas),
        _ => None,
    });
    let recs = ctl.recomendacoes(ctl.max_host_mib(0));
    if json {
        let tetos: Vec<_> = recs
            .iter()
            .map(|r| serde_json::json!({"tipo": r.tipo, "picos_mib": r.picos, "recomendado_mib": r.recomendado_mib}))
            .collect();
        let v = serde_json::json!({
            "host": {"total_mib": h.mem_total_mib, "disponivel_mib": h.mem_disponivel_mib, "psi_some_avg10": h.psi_some_avg10},
            "margem": {"piso_critico_mib": m.piso_critico_mib, "reservavel_mib": m.reservavel_mib, "folga_cgroup_mib": m.folga_cgroup_mib},
            "governador": {"limite_paralelo": ctl.limite_paralelo(), "admitiria_de_8x500mib": admitiria.unwrap_or(0)},
            "tetos_recomendados": tetos, "linhas_rejeitadas": ctl.linhas_rejeitadas,
        });
        println!("{v}");
        return Ok(());
    }
    imprimir(&h, &m, &ctl, admitiria, &recs);
    Ok(())
}

/// **O quê:** texto de `memoria status`. **Onde:** [`memoria_status`].
fn imprimir(
    h: &schematize_plataforma::memoria::EstadoHost,
    m: &schematize_plataforma::sistema::MargemMemoria,
    ctl: &Controle,
    admitiria: Option<usize>,
    recs: &[crate::resiliencia::controle::Recomendacao],
) {
    println!(
        "Host: {} MiB total, {} MiB disponível, PSI memória some avg10 {:.2}%",
        h.mem_total_mib, h.mem_disponivel_mib, h.psi_some_avg10
    );
    println!(
        "Margem: piso crítico {} MiB, reservável {} MiB (limitado por {:?})",
        m.piso_critico_mib, m.reservavel_mib, m.limitado_por
    );
    println!(
        "Governador: limite de paralelismo {}, admitiria {} de 8 unidades de 500 MiB agora",
        ctl.limite_paralelo(),
        admitiria.unwrap_or(0)
    );
    if recs.is_empty() {
        println!(
            "Tetos recomendados: sem histórico ainda (padrão {} MiB)",
            crate::resiliencia::controle::TETO_PADRAO_MIB
        );
    }
    for r in recs {
        println!(
            "Teto recomendado [{}]: {:?} MiB (picos recentes: {:?})",
            r.tipo, r.recomendado_mib, r.picos
        );
    }
    if ctl.linhas_rejeitadas > 0 {
        println!(
            "Aviso: {} linha(s) ilegível(is) em picos.hist foram ignoradas",
            ctl.linhas_rejeitadas
        );
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Flag vence o ambiente; `pt*` é português; o resto é inglês.
    #[test]
    fn idioma_da_flag_e_do_ambiente() {
        assert_eq!(idioma(Some("en"), Some("pt_BR.UTF-8")), Idioma::En);
        assert_eq!(idioma(Some("pt"), None), Idioma::Pt);
        assert_eq!(idioma(None, Some("PT_br")), Idioma::Pt);
        assert_eq!(idioma(None, Some("de_DE")), Idioma::En);
        assert_eq!(idioma(None, None), Idioma::En);
    }
}
