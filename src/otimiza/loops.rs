//! Detecção de loop e early-stop (IA-13, IA-12) — desenho próprio **[DESIGN]**.
//!
//! **O quê:** janela deslizante de chamadas (ferramenta+args, saída, diff); repetição
//! idêntica N vezes seguidas = loop. Leituras repetidas com o diff mudando NÃO são loop.
//! **Onde:** consumidor do stream-json do `claude -p`; o chamador extrai (ferramenta, args,
//! saída, hash do diff) — o formato exato dos eventos não foi verificado, por isso a entrada é abstrata.

use super::fnv64;
use std::collections::{BTreeMap, VecDeque};

/// Veredito após registrar uma chamada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Veredito {
    /// Segue.
    Ok,
    /// Loop: a mesma chamada/saída/diff repetida `repeticoes` vezes seguidas.
    Loop {
        /// Quantas vezes seguidas.
        repeticoes: usize,
    },
}

/// Detector com janela deslizante.
#[derive(Debug, Clone)]
pub struct Detector {
    janela: VecDeque<(u64, u64, u64)>,
    capacidade: usize,
    limite: usize,
}

impl Detector {
    /// Cria um detector; `limite` mínimo 2 (1 repetição não é loop), `capacidade >= limite`.
    /// **Onde:** uma instância por unidade.
    pub fn novo(limite: usize, capacidade: usize) -> Self {
        let limite = limite.max(2);
        Detector { janela: VecDeque::new(), capacidade: capacidade.max(limite), limite }
    }

    /// Registra uma chamada e devolve o veredito.
    /// **O quê:** chave = hash(ferramenta, args); compara chave+saída+diff com as anteriores.
    /// **Onde:** a cada `tool_use`/`tool_result` lido do stream.
    pub fn registrar(
        &mut self,
        ferramenta: &str,
        args: &str,
        saida: &str,
        diff_hash: u64,
    ) -> Veredito {
        let chave = chave_chamada(ferramenta, args);
        self.janela.push_back((chave, fnv64(saida.as_bytes()), diff_hash));
        while self.janela.len() > self.capacidade {
            self.janela.pop_front();
        }
        let ult = self.janela.back().copied();
        let reps = self.janela.iter().rev().take_while(|e| Some(**e) == ult).count();
        if reps >= self.limite {
            Veredito::Loop { repeticoes: reps }
        } else {
            Veredito::Ok
        }
    }
}

/// Chave de uma chamada: separador NUL evita colisão `("ab","c")` vs `("a","bc")`.
/// **Onde:** `Detector::registrar` e `leituras_duplicadas`.
pub fn chave_chamada(ferramenta: &str, args: &str) -> u64 {
    let mut b = Vec::with_capacity(ferramenta.len() + args.len() + 1);
    b.extend_from_slice(ferramenta.as_bytes());
    b.push(0);
    b.extend_from_slice(args.as_bytes());
    fnv64(&b)
}

/// Early-stop: os últimos `k + 1` hashes de diff iguais = `k` turnos sem mudança.
/// `k == 0` nunca para (entrada hostil); histórico curto demais não para.
/// **Onde:** fim de cada turno, antes de gastar o próximo.
pub fn early_stop(diffs_por_turno: &[u64], k: usize) -> bool {
    if k == 0 || diffs_por_turno.len() <= k {
        return false;
    }
    let cauda = &diffs_por_turno[diffs_por_turno.len() - (k + 1)..];
    cauda.windows(2).all(|w| w[0] == w[1])
}

/// Leituras duplicadas (IA-12): conta (caminho, mtime) repetidos; devolve só os com contagem >= 2.
/// **O quê:** relatório vazio quando não há duplicata.
/// **Onde:** pós-run ou injeção de dica no brief da retentativa.
pub fn leituras_duplicadas(leituras: &[(&str, u64)]) -> BTreeMap<(String, u64), usize> {
    let mut m: BTreeMap<(String, u64), usize> = BTreeMap::new();
    for (p, mt) in leituras {
        let c = m.entry(((*p).to_string(), *mt)).or_insert(0);
        *c = c.saturating_add(1);
    }
    m.retain(|_, c| *c >= 2);
    m
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn loop_encerra_em_n_chamadas() {
        let mut d = Detector::novo(3, 10);
        assert_eq!(d.registrar("Read", "a.rs", "x", 1), Veredito::Ok);
        assert_eq!(d.registrar("Read", "a.rs", "x", 1), Veredito::Ok);
        assert_eq!(d.registrar("Read", "a.rs", "x", 1), Veredito::Loop { repeticoes: 3 });
    }

    #[test]
    fn leitura_repetida_com_diff_mudando_nao_e_loop() {
        let mut d = Detector::novo(3, 10);
        for i in 0..50 {
            assert_eq!(d.registrar("Read", "a.rs", "x", i), Veredito::Ok);
        }
    }

    #[test]
    fn intercalado_quebra_sequencia_e_limite_hostil() {
        let mut d = Detector::novo(0, 0);
        assert_eq!(d.registrar("A", "", "", 0), Veredito::Ok);
        assert_eq!(d.registrar("B", "", "", 0), Veredito::Ok);
        assert_eq!(d.registrar("B", "", "", 0), Veredito::Loop { repeticoes: 2 });
    }

    #[test]
    fn chave_sem_colisao_e_unicode() {
        assert_ne!(chave_chamada("ab", "c"), chave_chamada("a", "bc"));
        let mut d = Detector::novo(2, 2);
        d.registrar("Bash", "echo ação\0\u{1F600}", "ç", 0);
        assert!(matches!(
            d.registrar("Bash", "echo ação\0\u{1F600}", "ç", 0),
            Veredito::Loop { .. }
        ));
    }

    #[test]
    fn early_stop_casos() {
        assert!(!early_stop(&[], 2));
        assert!(!early_stop(&[1, 1], 2));
        assert!(early_stop(&[9, 1, 1, 1], 2));
        assert!(!early_stop(&[1, 1, 2], 2));
        assert!(!early_stop(&[1, 1, 1], 0));
        assert!(!early_stop(&[1, 1], usize::MAX));
    }

    #[test]
    fn duplicadas_contagem_5_e_vazio() {
        let l = vec![("a.rs", 7); 5];
        assert_eq!(leituras_duplicadas(&l)[&("a.rs".to_string(), 7)], 5);
        assert!(leituras_duplicadas(&[("a", 1), ("a", 2), ("b", 1)]).is_empty());
        assert!(leituras_duplicadas(&[]).is_empty());
    }
}
