//! **Padrões de engenharia hardcoded** no binário do overdev (D1).
//!
//! **O quê:** blocos nomeados e versionados de padrões densos e imperativos (`&'static str`
//! compiladas, sem ler arquivo em runtime) e o [`prefixo_estavel`] que abre todo prompt.
//! **Onde:** `agentrun::prompts` e `agentrun::prompt_agentes` prefixam cada prompt de overdev e de
//! agente com ele — a parte ESTÁTICA vem primeiro (prefixo de prompt cache), a dinâmica depois.
//!
//! Fontes: pesquisa 2026-11-20 (ENG-01..25), `eficiencia-runtime.md`, herdados da engineering.
//! Números de RSS são política da casa [rec], não fato externo.

mod jaula;
mod nucleo;
mod orquestracao;
mod qualidade;
mod runtime;
mod seguranca;

/// Versão do conjunto de padrões; mude junto com qualquer edição de bloco.
pub const PADROES_VERSAO: &str = "2026.11.20-1";

/// Os blocos nomeados, na ORDEM FIXA em que entram no prefixo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nome {
    /// Pisos inegociáveis.
    Nucleo,
    /// Eficiência de runtime e orçamento de memória.
    Runtime,
    /// Jaula de memória por execução.
    Jaula,
    /// Orquestração e escada de modelos.
    Orquestracao,
    /// Qualidade de código.
    Qualidade,
    /// Segurança e supply chain.
    Seguranca,
}

/// Todos os blocos, na ordem estável de concatenação (array, nunca HashMap).
pub const ORDEM: [Nome; 6] = [
    Nome::Nucleo,
    Nome::Runtime,
    Nome::Jaula,
    Nome::Orquestracao,
    Nome::Qualidade,
    Nome::Seguranca,
];

/// **O quê:** o texto compilado de um bloco. **Onde:** testes e [`prefixo_estavel`].
pub fn bloco(n: Nome) -> &'static str {
    match n {
        Nome::Nucleo => nucleo::TEXTO,
        Nome::Runtime => runtime::TEXTO,
        Nome::Jaula => jaula::TEXTO,
        Nome::Orquestracao => orquestracao::TEXTO,
        Nome::Qualidade => qualidade::TEXTO,
        Nome::Seguranca => seguranca::TEXTO,
    }
}

/// **O quê:** cabeçalho versionado + blocos em [`ORDEM`], byte-estável (sem timestamp nem ordem
/// de mapa). **Onde:** início de TODO prompt de overdev/agente; é o prefixo do prompt cache.
pub fn prefixo_estavel() -> String {
    let mut s = format!("PADROES DE ENGENHARIA DA CASA v{PADROES_VERSAO} (vinculantes)\n\n");
    for n in ORDEM {
        s.push_str(bloco(n));
        s.push('\n');
    }
    s.push_str("--- fim dos padroes; a tarefa vem abaixo ---\n\n");
    s
}

/// **O quê:** FNV-1a 64 (std-only) em hex de 16 dígitos sobre `dados`. **Onde:** [`hash_prefixo`].
fn fnv1a64(dados: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in dados {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// **O quê:** hash FNV-1a 64 em hex do [`prefixo_estavel`]; muda se o prefixo mudar (invalida
/// cache). **Onde:** diagnóstico/telemetria e testes de estabilidade.
pub fn hash_prefixo() -> String {
    format!("{:016x}", fnv1a64(prefixo_estavel().as_bytes()))
}
