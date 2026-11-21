//! Orçamento de contexto por unidade (IA-11 parcial) — estimativas **[DESIGN]**.
//!
//! **O quê:** estimador barato de tokens (bytes/4), limiares de handoff/compactação e plano de
//! trimming de resultados antigos. **Onde:** supervisor da unidade, a cada turno.

/// Estima tokens: `ceil(bytes / 4)`. Superestima texto multibyte (unicode) — viés seguro.
/// **Onde:** qualquer texto que entra no contexto (resultado de tool, brief, NOTES).
pub fn estimar_tokens(texto: &str) -> u64 {
    (texto.len() as u64).div_ceil(4)
}

/// Orçamento da unidade: janela e limiares em % (0..=100).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Orcamento {
    /// Tamanho da janela de contexto, em tokens.
    pub janela: u64,
    /// % a partir do qual gera handoff.
    pub handoff_pct: u8,
    /// % a partir do qual compacta/apara (>= handoff).
    pub compactar_pct: u8,
}

/// Ação recomendada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    /// Seguir.
    Seguir,
    /// Gravar handoff agora.
    Handoff,
    /// Compactar/aparar agora.
    Compactar,
}

/// Decide a ação para `usado` tokens. Janela 0 (config hostil) = `Compactar` (fail-safe).
/// Limiares > 100 são limitados a 100; `compactar_pct < handoff_pct` é elevado ao handoff.
/// **Onde:** a cada turno do supervisor.
pub fn decidir(usado: u64, o: &Orcamento) -> Acao {
    if o.janela == 0 {
        return Acao::Compactar;
    }
    let h = u128::from(o.handoff_pct.min(100));
    let c = u128::from(o.compactar_pct.min(100)).max(h);
    let pct = u128::from(usado) * 100;
    let j = u128::from(o.janela);
    if pct >= c * j {
        Acao::Compactar
    } else if pct >= h * j {
        Acao::Handoff
    } else {
        Acao::Seguir
    }
}

/// Plano de trimming: índices (dos mais antigos) a aparar até `total <= alvo`, sem tocar nos
/// `manter_recentes` últimos. `tokens[i]` = tamanho do resultado `i` (ordem cronológica).
/// **O quê:** devolve índices em ordem crescente; pode não atingir o alvo se só sobram recentes.
/// **Onde:** ação `Compactar` do supervisor.
pub fn plano_trimming(tokens: &[u64], alvo: u64, manter_recentes: usize) -> Vec<usize> {
    let mut total: u128 = tokens.iter().map(|t| u128::from(*t)).sum();
    let alvo = u128::from(alvo);
    let elegiveis = tokens.len().saturating_sub(manter_recentes);
    let mut cortes = Vec::new();
    for (i, t) in tokens.iter().enumerate().take(elegiveis) {
        if total <= alvo {
            break;
        }
        if *t > 0 {
            total = total.saturating_sub(u128::from(*t));
            cortes.push(i);
        }
    }
    cortes
}

#[cfg(test)]
mod testes {
    use super::*;

    const O: Orcamento = Orcamento { janela: 1000, handoff_pct: 70, compactar_pct: 85 };

    #[test]
    fn estimador() {
        assert_eq!(estimar_tokens(""), 0);
        assert_eq!(estimar_tokens("abcd"), 1);
        assert_eq!(estimar_tokens("abcde"), 2);
        assert_eq!(estimar_tokens("ação"), 2); // 5 bytes
    }

    #[test]
    fn limiares_fronteira_e_overflow() {
        assert_eq!(decidir(699, &O), Acao::Seguir);
        assert_eq!(decidir(700, &O), Acao::Handoff);
        assert_eq!(decidir(850, &O), Acao::Compactar);
        assert_eq!(decidir(u64::MAX, &O), Acao::Compactar);
        assert_eq!(decidir(0, &Orcamento { janela: 0, ..O }), Acao::Compactar);
        let inv = Orcamento { janela: 100, handoff_pct: 200, compactar_pct: 10 };
        assert_eq!(decidir(99, &inv), Acao::Seguir);
        assert_eq!(decidir(100, &inv), Acao::Compactar);
    }

    #[test]
    fn trimming_poupa_recentes() {
        assert_eq!(plano_trimming(&[10, 10, 10, 10], 20, 1), vec![0, 1]);
        assert_eq!(plano_trimming(&[10, 10, 10], 0, 3), Vec::<usize>::new());
        assert_eq!(plano_trimming(&[], 0, 0), Vec::<usize>::new());
        assert_eq!(plano_trimming(&[u64::MAX, u64::MAX, 5], 5, 1), vec![0, 1]);
        assert_eq!(plano_trimming(&[0, 0, 9], 5, 0), vec![2]);
    }
}
