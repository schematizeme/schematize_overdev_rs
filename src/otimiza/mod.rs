//! Otimização de uso de IA do overdev (pesquisa 2026-11-20, IA-01/04/07/12/13/16/20).
//!
//! **O quê:** lógica PURA e testável (std + serde): ledger de custo, cascata de modelos,
//! detecção de loop, orçamento de contexto, retomada idempotente e TTL de cache.
//! **Onde:** o orquestrador chama `claude -p` como processo e consulta estas decisões antes de
//! (re)lançar uma unidade. Nada aqui faz I/O de rede nem lança processo.
//!
//! FORA (exigem API Messages, inexistentes em `claude -p`): IA-18 (Messages API/Batch,
//! programmatic tool calling, context editing). Também fora por serem I/O/processo:
//! IA-02 supervisor/backoff, IA-05 flags, IA-10 cgroup, IA-19 worktree.

pub mod cache;
pub mod cascata;
pub mod contexto;
pub mod ledger;
pub mod loops;
pub mod retomada;

/// Hash FNV-1a de 64 bits, determinístico entre versões do Rust (ao contrário de `DefaultHasher`).
///
/// **O quê:** impressão digital barata (NÃO criptográfica) de texto/bytes.
/// **Onde:** chave de repetição em `loops` e hash do brief em `retomada`.
pub fn fnv64(dados: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in dados {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod testes {
    use super::fnv64;

    #[test]
    fn fnv_vetores_conhecidos() {
        assert_eq!(fnv64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_ne!(fnv64("ação".as_bytes()), fnv64("acao".as_bytes()));
    }
}
