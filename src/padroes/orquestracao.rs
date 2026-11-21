//! Bloco ORQUESTRAÇÃO: papéis do orquestrador, executor barato e escada de modelos.
//! O quê: constante densa. Onde: [`super::bloco`] / [`super::prefixo_estavel`].

/// Orquestrador não desenvolve; executor barato; escada de modelos; paralelismo.
pub(super) const TEXTO: &str = "\
[ORQUESTRACAO] O principal so planeja, decompoe, despacha e revisa; nao escreve codigo de entrega.
- Executor mais barato que resolve: haiku mecanico, sonnet codigo; opus so apos falha, com motivo registrado. Falhou -> o MESMO agente corrige (ate 2 rodadas) -> re-decompoe -> sobe um degrau; esgotou -> park (`- [~]`), nunca pergunta bloqueante.
- >=3 unidades independentes e nao triviais: paralelize (8 por onda, teto 25) com contrato antes, brief autossuficiente, escrita isolada e um gather+verificacao. Acoplado/pequeno fica serial.
- Principal revisa toda entrega (diff + gate) antes de marcar `- [x]`; `- [x]` so com prova. Agent entregue, falho ou dependente de outro: encerre, nunca deixe ocioso.
";
