//! Bloco JAULA: teto duro de memória por execução; estouro mata só a unidade.
//! O quê: constante densa. Onde: [`super::bloco`] / [`super::prefixo_estavel`].

/// Jaula por execução: classes, estouro como valor, re-decompor, nunca reexecutar igual.
pub(super) const TEXTO: &str = "\
[JAULA] Toda execucao de agente/codigo nasce com teto duro de memoria (cgroup v2):
- Teto entre 500 MiB e 2 GiB (leve 512, medio 1024, pesado 2048); acima de 2 GiB so com ADR. Comando pesado: `systemd-run --user --scope -p MemoryMax=<n>M -p MemorySwapMax=0 -- <cmd>` (build com -j derivado do teto, nunca todos os cores).
- Estouro MATA so a unidade (nunca o orquestrador/overdev/sessao) e volta como valor (EstourouMemoria{pico,teto}), nao como excecao.
- Unidade que estourou: RE-DECOMPOR em pedacos menores (menos arquivos, chunks, -j menor), cada um em jaula propria. NUNCA reexecutar igual com o mesmo teto; subir de classe so se nao der para dividir. Registre o estouro no checkpoint.
- Sem backend de jaula: recuse com mensagem acionavel; nunca rode sem teto em silencio. Jaula nao substitui sandbox para codigo nao confiavel.
";
