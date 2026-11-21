//! Testes dos padrões hardcoded (D1): estabilidade, marcadores, orçamento e injeção nos prompts.
//!
//! **O quê:** garante prefixo byte-estável, blocos não vazios/≤750 linhas, tamanho ≤ teto e
//! prompts começando pelo prefixo. **Onde:** CI local (`cargo test --test padroes_prefixo`).

use overdev_casa::agentes::Agentes;
use overdev_casa::agentrun::{
    archive_todos_prompt, brief_subagente, overdev_prompt_com, reindex_prompt,
};
use overdev_casa::padroes::{bloco, hash_prefixo, prefixo_estavel, Nome, ORDEM, PADROES_VERSAO};
use std::path::Path;

/// Teto de bytes do prefixo: padrões densos, não um livro.
const TETO_BYTES: usize = 6 * 1024;

/// Duas chamadas produzem bytes idênticos e o mesmo hash.
#[test]
fn prefixo_byte_identico() {
    assert_eq!(prefixo_estavel().as_bytes(), prefixo_estavel().as_bytes());
    assert_eq!(hash_prefixo(), hash_prefixo());
    assert_eq!(hash_prefixo().len(), 16);
    assert!(prefixo_estavel().contains(PADROES_VERSAO));
}

/// Cada bloco traz seus marcadores-chave.
#[test]
fn marcadores_de_cada_bloco() {
    let casos = [
        (Nome::Nucleo, "SQL sempre parametrizado"),
        (Nome::Runtime, "OOM e BUG"),
        (Nome::Runtime, "256 MiB"),
        (Nome::Jaula, "500 MiB"),
        (Nome::Jaula, "NUNCA reexecutar igual"),
        (Nome::Orquestracao, "nao escreve codigo de entrega"),
        (Nome::Qualidade, "750 linhas"),
        (Nome::Qualidade, "<= 50 linhas"),
        (Nome::Seguranca, "Supply chain"),
    ];
    for (n, m) in casos {
        assert!(bloco(n).contains(m), "{n:?} sem `{m}`");
        assert!(prefixo_estavel().contains(m));
    }
}

/// Orçamento de tamanho; bloco nunca vazio nem gigante.
#[test]
fn tamanho_e_blocos_validos() {
    let p = prefixo_estavel();
    assert!(p.len() <= TETO_BYTES, "prefixo com {} bytes > {TETO_BYTES}", p.len());
    for n in ORDEM {
        let b = bloco(n);
        assert!(b.trim().len() > 100, "bloco {n:?} vazio/curto");
        assert!(b.lines().count() <= 750, "bloco {n:?} > 750 linhas");
    }
}

/// Todo prompt de overdev/agente abre pelo prefixo estável.
#[test]
fn prompts_comecam_pelo_prefixo() {
    let p = prefixo_estavel();
    let od = overdev_prompt_com("objetivo X", &Agentes::padrao());
    assert!(od.starts_with(&p));
    assert!(od.contains("objetivo X"));
    assert!(brief_subagente(Path::new("checklist/p0-001-a.md")).starts_with(&p));
    assert!(archive_todos_prompt().starts_with(&p));
    assert!(reindex_prompt().starts_with(&p));
}
