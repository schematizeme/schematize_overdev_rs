//! TTL do cache de prompt (IA-16): 5 min por padrão.
//!
//! **O quê:** decide se o prefixo ainda está quente e agrupa relançamentos dentro do TTL.
//! **Onde:** scheduler, antes de relançar unidade (ex.: após OOM). Tempos em segundos Unix.

/// TTL padrão do cache de prompt (5 min).
pub const TTL_PADRAO_SEG: u64 = 300;
/// Margem de segurança: relançar a `ttl - margem` já conta como "quase frio".
pub const MARGEM_PADRAO_SEG: u64 = 15;

/// O cache de prompt ainda está quente?
///
/// **O quê:** `agora - ultimo_uso < ttl`. Relógio voltando (`agora < ultimo_uso`) conta como
/// quente (decorrido 0). `ttl == 0` nunca é quente.
/// **Onde:** scheduler antes de decidir esperar ou relançar.
pub fn cache_ainda_quente(ultimo_uso: u64, agora: u64, ttl: u64) -> bool {
    ttl > 0 && agora.saturating_sub(ultimo_uso) < ttl
}

/// Agrupa relançamentos: cada pedido cujo intervalo ao anterior do grupo é `< ttl - margem`
/// entra no mesmo grupo (cada uso renova o TTL).
///
/// **O quê:** devolve grupos de ids ordenados por instante; pedidos com mesmo instante mantêm a
/// ordem de entrada. `margem >= ttl` isola cada pedido.
/// **Onde:** scheduler de ondas: lançar o grupo junto mantém o cache quente (leitura 0,1x).
pub fn agrupar_relancamentos(pedidos: &[(String, u64)], ttl: u64, margem: u64) -> Vec<Vec<String>> {
    let mut ord: Vec<&(String, u64)> = pedidos.iter().collect();
    ord.sort_by_key(|p| p.1);
    let janela = ttl.saturating_sub(margem);
    let mut grupos: Vec<Vec<String>> = Vec::new();
    let mut ultimo: Option<u64> = None;
    for (id, t) in ord {
        let junta = matches!(ultimo, Some(u) if t.saturating_sub(u) < janela);
        if junta {
            if let Some(g) = grupos.last_mut() {
                g.push(id.clone());
            }
        } else {
            grupos.push(vec![id.clone()]);
        }
        ultimo = Some(*t);
    }
    grupos
}

#[cfg(test)]
mod testes {
    use super::*;

    fn p(id: &str, t: u64) -> (String, u64) {
        (id.to_string(), t)
    }

    #[test]
    fn quente_fronteiras() {
        assert!(cache_ainda_quente(100, 399, 300));
        assert!(!cache_ainda_quente(100, 400, 300));
        assert!(cache_ainda_quente(100, 50, 300), "relógio voltou");
        assert!(!cache_ainda_quente(0, 0, 0));
        assert!(!cache_ainda_quente(0, u64::MAX, 300));
    }

    #[test]
    fn agrupa_dentro_do_ttl_e_encadeia() {
        let g = agrupar_relancamentos(&[p("c", 400), p("a", 0), p("b", 200)], 300, 15);
        assert_eq!(g, vec![vec!["a", "b", "c"]]);
        let g = agrupar_relancamentos(&[p("a", 0), p("b", 290)], 300, 15);
        assert_eq!(g.len(), 2, "290 >= 285 -> separa");
    }

    #[test]
    fn vazio_e_margem_hostil() {
        assert!(agrupar_relancamentos(&[], 300, 15).is_empty());
        let g = agrupar_relancamentos(&[p("a", 1), p("b", 1)], 300, u64::MAX);
        assert_eq!(g.len(), 2);
        let g = agrupar_relancamentos(&[p("é", u64::MAX), p("ü", 0)], 300, 0);
        assert_eq!(g.len(), 2);
    }
}
