//! Política PURA do lançamento jaulado: classe, nome de unidade, reação ao veredito e prompts.
//!
//! **O quê:** tudo aqui é função pura (sem systemd, sem disco): testável em qualquer máquina.
//! **Onde:** [`super`] (lançamento/registro) e o supervisor (`overdev::supervisor`).

use schematize_plataforma::jaula::{
    estimar, sanitizar_unidade, Classe, Orcamento, TipoExecucao, Veredito,
};

/// Máximo de gerações de re-decomposição antes de parkear a unidade (`- [~]`).
pub const MAX_GERACOES: u32 = 3;

/// Variável de ambiente que o USUÁRIO liga para abrir mão da jaula (escolha explícita).
pub const ENV_SEM_JAULA: &str = "SCHEMATIZE_SEM_JAULA";

/// O que fazer depois de uma unidade jaulada terminar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReacaoAoVeredito {
    /// Concluiu: nada a reagir.
    Seguir,
    /// Morta por OOM/timeout: NÃO reexecutar igual — re-decompor; `geracao` é a da próxima unidade.
    Redecompor {
        /// Geração da unidade re-decomposta (1..=[`MAX_GERACOES`]).
        geracao: u32,
    },
    /// Esgotou as gerações: marcar o item `- [~]` e seguir para o próximo.
    Parkear,
    /// Falha comum: vale a política atual do supervisor (relançar com a retomada).
    PoliticaAtual,
    /// Sem como criar a jaula: recusa acionável e sem culpa (nada rodou).
    Recusar(String),
}

/// **O quê:** orçamento da unidade por tipo de execução (via `estimar`: agente LLM = 1 GiB).
/// **Onde:** [`super::planejar`] e testes.
pub fn orcamento_para(tipo: TipoExecucao) -> Orcamento {
    estimar(tipo)
}

/// **O quê:** a [`Classe`] correspondente a um teto (o menor degrau que o comporta).
/// **Onde:** registro em `jaulas.jsonl` ([`classe_nome`]).
pub fn classe_do_teto(mem_max_mib: u64) -> Classe {
    if mem_max_mib <= Classe::Leve.mem_mib() {
        Classe::Leve
    } else if mem_max_mib <= Classe::Medio.mem_mib() {
        Classe::Medio
    } else {
        Classe::Pesado
    }
}

/// **O quê:** classe do lançamento de um tipo de execução (`AgenteLlm` -> `Medio`).
/// **Onde:** [`super::planejar`].
pub fn classe_do_lancamento(tipo: TipoExecucao) -> Classe {
    classe_do_teto(orcamento_para(tipo).mem_max_mib)
}

/// **O quê:** nome estável da classe para o JSON (`leve`/`medio`/`pesado`).
/// **Onde:** [`super::registro::Registro`].
pub fn classe_nome(c: Classe) -> &'static str {
    match c {
        Classe::Leve => "leve",
        Classe::Medio => "medio",
        Classe::Pesado => "pesado",
    }
}

/// **O quê:** slug do projeto para o nome da unidade: minúsculas `[a-z0-9-]`, até 24 chars;
/// vazio vira `proj`. **Onde:** [`nome_da_unidade`].
pub fn slug_do_projeto(nome: &str) -> String {
    let s: String = nome
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .take(24)
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "proj".to_string()
    } else {
        s
    }
}

/// **O quê:** `schematize-<slug>-<seq>` já sanitizado (nome hostil nunca vira opção/injeção).
/// **Onde:** [`super::planejar`], um nome único por lançamento.
pub fn nome_da_unidade(projeto: &str, seq: &str) -> String {
    sanitizar_unidade(&format!("schematize-{}-{}", slug_do_projeto(projeto), seq))
}

/// **O quê:** `true` só para o valor exato `1` — jaula desligada por escolha explícita.
/// **Onde:** [`super::sem_jaula_por_env`].
pub fn sem_jaula_optado(valor: Option<&str>) -> bool {
    valor.is_some_and(|v| v.trim() == "1")
}

/// **O quê:** mensagem de recusa: o que não deu, por quê, e o que EU faço por você.
/// **Onde:** [`reagir`] e o lançador quando não há backend.
pub fn mensagem_sem_backend(motivo: &str) -> String {
    format!(
        "Não consegui criar a jaula de memória porque {motivo} Para proteger o resto do \
         sistema não rodei nada sem teto. Eu preparo isso por você: instale/ative o \
         `systemd-run --user` (cgroup v2 com o controller `memory` delegado); se preferir \
         seguir SEM proteção, diga isso explicitamente com {ENV_SEM_JAULA}=1 e eu registro o aviso."
    )
}

/// **O quê:** a decisão: veredito + geração corrente (0 = original) -> [`ReacaoAoVeredito`].
/// `EstourouMemoria`/`Timeout` re-decompõem até [`MAX_GERACOES`], depois parkeiam.
/// **Onde:** supervisor, ao apurar uma unidade morta.
pub fn reagir(v: &Veredito, geracao_atual: u32) -> ReacaoAoVeredito {
    match v {
        Veredito::Concluiu(_) => ReacaoAoVeredito::Seguir,
        Veredito::Falhou(_) => ReacaoAoVeredito::PoliticaAtual,
        Veredito::SemBackend(m) => ReacaoAoVeredito::Recusar(mensagem_sem_backend(m)),
        Veredito::EstourouMemoria { .. } | Veredito::Timeout => {
            if geracao_atual >= MAX_GERACOES {
                ReacaoAoVeredito::Parkear
            } else {
                ReacaoAoVeredito::Redecompor { geracao: geracao_atual + 1 }
            }
        }
    }
}

/// **O quê:** prompt de RE-DECOMPOSIÇÃO: a unidade foi MORTA-POR-OOM; quebrar em pedaços
/// menores e reduzir o working set — nunca repetir igual. **Onde:** supervisor, no relançamento.
pub fn prompt_redecomposicao(
    geracao: u32,
    unidade: &str,
    pico_mib: Option<u64>,
    teto_mib: u64,
) -> String {
    let pico = pico_mib.map_or("desconhecido".to_string(), |p| format!("{p} MiB"));
    format!(
        "UNIDADE MORTA-POR-OOM (geração {geracao}/{MAX_GERACOES}). A unidade `{unidade}` estourou \
         o teto de {teto_mib} MiB (pico {pico}) e o kernel matou SÓ ela; o overdev segue. NÃO \
         reexecute igual. Releia `.schematize/overdev/CHECKLIST.md`, confie no que está `- [x]` e \
         RE-DECOMPONHA o item em curso em pedaços MENORES (um alvo de escrita por pedaço), \
         reduza o working set (leia só os trechos necessários, sem carregar arquivos/saídas \
         gigantes, streaming/paginação, `cargo -j1`/`-j2` sob teto) e execute um pedaço por vez. \
         Se após {MAX_GERACOES} gerações ainda estourar, o item será parkeado `- [~]`."
    )
}

/// **O quê:** prompt de PARKEAR: esgotou as gerações; marcar `- [~]` com o motivo e seguir.
/// **Onde:** supervisor, quando [`reagir`] devolve [`ReacaoAoVeredito::Parkear`].
pub fn prompt_parkear(unidade: &str, teto_mib: u64) -> String {
    format!(
        "UNIDADE `{unidade}` ESGOTOU {MAX_GERACOES} gerações de re-decomposição (teto {teto_mib} \
         MiB). PARKEIE o item em curso no `CHECKLIST.md` como `- [~]` com uma linha de motivo \
         (OOM recorrente = indício de código mal otimizado: anote a causa provável) e SIGA para \
         o próximo `- [ ]`. Não pergunte; não reexecute o item parkeado."
    )
}
