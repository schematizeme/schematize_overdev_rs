//! `oom_score_adj` do próprio orquestrador: baixo se permitido, sem erro se não.
//!
//! **O quê:** tenta gravar um valor baixo em `/proc/self/oom_score_adj` para o kernel sacrificar
//! as jaulas (800) antes do orquestrador. Sem `CAP_SYS_RESOURCE` valor negativo é recusado: é o
//! caso normal em usuário comum e vira só um texto informativo (nunca um erro).
//! **Onde:** boot de `executar`/`retomar` e do supervisor.

use schematize_plataforma::persistencia::vigia::ajustar_oom;

/// Valor desejado para o orquestrador (as jaulas usam 800).
pub const OOM_ADJ_DESEJADO: i32 = -500;

/// **O quê:** `oom_score_adj` atual deste processo (`None` se ilegível). **Onde:** diagnóstico.
pub fn oom_adj_atual() -> Option<i32> {
    std::fs::read_to_string("/proc/self/oom_score_adj").ok()?.trim().parse().ok()
}

/// **O quê:** tenta baixar o `oom_score_adj` próprio; devolve um texto do resultado (aplicado ou
/// "não permitido, segue com N"). **Onde:** boot; o texto vai para o journal como observação.
/// **Efeitos:** escreve em `/proc/self/oom_score_adj` (best-effort).
pub fn proteger_orquestrador() -> String {
    let antes = oom_adj_atual();
    let aplicado = ajustar_oom(std::process::id(), OOM_ADJ_DESEJADO);
    let depois = oom_adj_atual();
    if aplicado {
        format!("oom_score_adj {antes:?} -> {depois:?} (aplicado)")
    } else {
        format!("oom_score_adj {OOM_ADJ_DESEJADO} não permitido sem CAP_SYS_RESOURCE; segue em {depois:?}")
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Sem privilégio não dá erro nem panic; o texto sempre existe.
    #[test]
    fn proteger_nunca_falha() {
        let t = proteger_orquestrador();
        assert!(t.contains("oom_score_adj"), "{t}");
    }
}
