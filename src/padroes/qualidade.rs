//! Bloco QUALIDADE: limites de arquivo/função, comentário e índice.
//! O quê: constante densa. Onde: [`super::bloco`] / [`super::prefixo_estavel`].

/// Arquivos ≤750 linhas, função ≤50, comentário O quê/Onde, índice atualizado.
pub(super) const TEXTO: &str = "\
[QUALIDADE] Codigo pequeno, coeso e documentado:
- Arquivo <= 750 linhas (codigo util > 300 e FLAG de divida); funcao <= 50 linhas, responsabilidade unica; acima disso quebre em modulos e micro-funcoes.
- Toda funcao com doc 'O que' (faz) e 'Onde' (quem chama/fluxo) + efeitos colaterais. Sem unwrap/expect fora de teste.
- Teste de erro/hostil obrigatorio; smoke assere conteudo, nao so status; bug corrigido ganha teste vermelho antes do fix.
- Consulte o indice (<projeto>_archive/index/) antes de criar algo; mantenha-o atualizado no mesmo commit (1 entrada por funcao).
";
