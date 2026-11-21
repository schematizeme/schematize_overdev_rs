//! Bloco RUNTIME: eficiência de runtime, orçamento de memória e decomposição de etapas.
//! O quê: constante densa. Onde: [`super::bloco`] / [`super::prefixo_estavel`].

/// Orçamento de memória, OOM=bug, poucas libs medidas, decomposição máxima.
pub(super) const TEXTO: &str = "\
[RUNTIME] Eficiencia e requisito, nao otimizacao tardia:
- Todo servico/operacao declara orcamento (rss idle/nominal/max, p99, payload max) e MEDE no gate local; estourou = corrige o codigo ou ADR com medicao.
- OOM e BUG de design, nunca falta de RAM: proibido subir limite, ligar swap ou maquina maior como solucao. CRUD Rust/Go > ~256 MiB RSS sob carga nominal e DEFEITO.
- Distinga RSS estavel (retencao do alocador/pool) de RSS crescente (leak/acumulo). Meca antes de opinar (heaptrack, dhat, cargo-bloat, memory.peak do cgroup).
- Tudo com limite: pool, canal bounded, cache com TTL/teto, paginacao com page_size maximo, stream em vez de buffer inteiro; emprestar (&str, Cow, Bytes) em vez de clonar.
- Poucas libs: nova so com justificativa (o que std/lib presente nao resolve), default-features=false, versao pinada, --locked, peso medido (cargo tree -d, udeps, bloat).
- Release com lto/strip em prd; debug build em prd e vetado.
- Decomposicao maxima: quebre em etapas pequenas, cada uma com contrato tipado persistido e idempotente, orcamento, medicao (span OTel, RSS pico) e otimizacao propria; etapa que estoura subdivide ou vira stream. Retry so da etapa, nunca do pipeline todo.
";
