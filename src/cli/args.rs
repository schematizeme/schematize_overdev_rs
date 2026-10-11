//! A superfície da CLI.
//!
//! **Onde:** `main`, na raiz da árvore do clap.
//!
//! **Os nomes vieram do `schematize overdev <sub>` do hub, de propósito** (E6 do ADR-0018):
//! quem já digitava `overdev start "…"` digita `schematize-overdev start "…"`. E
//! `schematize overdev …` continua funcionando, encaminhado pelo hub — **inclusive os HOOKS
//! do Claude Code**, que chamam `schematize overdev guard` a cada chamada de ferramenta e
//! `schematize overdev check` a cada parada. Mudar a forma desses dois comandos no mesmo dia
//! em que eles mudam de dono travaria toda sessão aberta.

use clap::{Parser, Subcommand};

/// `schematize-overdev` — fila, trava, supervisor e gate de um run.
#[derive(Parser)]
#[command(
    name = "schematize-overdev",
    version = overdev_casa::nucleo::procedencia::rotulo_versao(),
    about = "schematize overdev — the engine of a run: queue, lock, supervisor and gate",
    long_about = "Run, supervise and gate an overdev in a project.\n\
                  Works standalone; the schematize hub delegates its Overdev tab to it, and \
                  the Claude Code hooks call `guard` and `check` through it."
)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Register the hooks (Stop + AskUserQuestion veto) in settings.json.
    Enable,
    /// Remove the overdev hooks from settings.json.
    Disable,
    /// Start a run in the current directory: `schematize overdev start "<goal>"`.
    Start {
        objetivo: Vec<String>,
        #[arg(long)]
        max: Option<u64>,
    },
    /// Abre um TERMINAL interativo já neste projeto, com o `claude` pronto e o bypass de
    /// permissões ligado. Quando o claude sai, o shell continua aberto na pasta certa.
    Terminal,
    /// SUPERVISIONA o run deste diretório: se o agente morrer (contexto/crash/janela fechada)
    /// com item de máquina aberto, RELANÇA. É a rede que o Stop hook não cobre — ele só age
    /// quando o agente TENTA encerrar o turno, não quando o processo simplesmente acaba.
    Supervise {
        /// Teto de relançamentos (guardrail anti-loop).
        #[arg(long)]
        max: Option<u32>,
    },
    /// SPLIT do checklist em K arquivos `checklist/part-N.md` (pastas multi-arquivo) pra rodar
    /// multiagents. Respeita o governador (`schematize agents`): mostra quantos subagents por claude
    /// e recusa passar do teto seguro. `--dispatch` lança os K claudes (cada um no seu part).
    Split {
        /// Em quantos claudes principais dividir (2, 4, …).
        k: usize,
        /// Lança os K claudes em terminais externos, cada um no seu part-N.md.
        #[arg(long)]
        dispatch: bool,
        /// Ignora o teto do governador (perigoso — pode travar a máquina).
        #[arg(long)]
        force: bool,
    },
    /// (Stop hook) reject stopping while there is an open item.
    Check,
    /// (PreToolUse hook) veto AskUserQuestion during overdev.
    Guard,
    /// The Claude Code HOOKS: are they registered, and are they up to date?
    ///
    /// **Existe porque o `doctor` do hub precisa dessa resposta**, e depois da E6 quem sabe
    /// dela é este app. Sem um subcomando, o hub teria de reimplementar a leitura do
    /// `settings.json` — a segunda lei para o mesmo fato.
    Hooks {
        /// Repair the hooks (re-register them pointing at the current binary).
        #[arg(long)]
        repair: bool,
        /// Machine-readable output (the hub's `doctor` reads this).
        #[arg(long)]
        json: bool,
    },
    /// The concurrency budget: how many agents fit in PARALLEL on this machine.
    ///
    /// **Veio junto com o overdev na E6**, porque é ele quem divide o trabalho em K agentes e
    /// precisa saber o teto antes de lançar. O `schematize agents` continua funcionando,
    /// encaminhado pelo hub.
    Agents {
        /// Machine-readable output.
        #[arg(long)]
        json: bool,
        /// Plan a split into K main agents and show how many subagents fit in each.
        #[arg(long)]
        split: Option<usize>,
    },
    /// Sync the project archive (`<projeto>_archive`).
    ///
    /// **Veio junto com o overdev (E6)** porque é lá que ele grava handoff, contexto, planos e
    /// índice — o archive é a memória do run, e a memória vai com o motor.
    Archive,
    /// Open the auxiliary HTML panel (overdev + index graph) in the browser.
    Panel,
    /// Graph tools (export the index graph, Obsidian-style).
    Graph {
        #[command(subcommand)]
        sub: GraphCmd,
    },
    /// Show the run state.
    Status,
    /// Where the run stands: done, open, on-hold and human items.
    ///
    /// **Separado do `status`**, que é a tela humana com objetivo e tabela. Este é o número —
    /// e a janela e o hub leem o número, não a tela.
    Progresso {
        /// Machine-readable output (the window and the hub read this).
        #[arg(long)]
        json: bool,
    },
    /// The checklist, item by item: type, text, machine/human (the window reads this).
    Checklist {
        /// Machine-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Mark the first open item matching the text as on-hold.
    Hold { texto: Vec<String> },
    /// Park a question (log it in the base txt) and mark the item on-hold.
    Park { item: String, pergunta: Vec<String> },
    /// Human closes a `- [H ]` item → `- [H x]`: by text and/or `--done <n>`.
    Human {
        /// Text that the human item must contain (optional if --done is given).
        texto: Vec<String>,
        /// Close the Nth open human item (1-based) instead of matching text.
        #[arg(long)]
        done: Option<usize>,
    },
    /// Attach a human note (correction prompt / per-task point) in .schematize/overdev/NOTAS.md.
    Note {
        texto: Vec<String>,
        /// Note kind: correcao (correction prompt) | task (per-task point). Default: correcao.
        #[arg(long, default_value = "correcao")]
        kind: String,
    },
    /// Answer a human item with text — this RELEASES the machine item it was blocking.
    Answer {
        /// Nth open human item (1-based), or a text fragment of it.
        alvo: String,
        /// The answer / decision. Goes to the checklist and to DECISOES.md.
        texto: Vec<String>,
    },
    /// Refuse a human item — the machine item it blocked is CANCELLED, not resumed.
    Refuse {
        /// Nth open human item (1-based), or a text fragment of it.
        alvo: String,
        /// Why it is not applicable.
        texto: Vec<String>,
    },
    /// Enqueue a QUIZ question for the human: clickable options, context, and what it unblocks.
    ///
    /// Use this instead of `park` whenever the question has a closed set of answers. A question
    /// whose text contains "or" MUST be `--kind escolha`: asked as free text, it gets answered
    /// with one word that does not resolve the "or" (it happened, 2026-09-24).
    Ask {
        /// The question, in one line.
        titulo: Vec<String>,
        /// aprovar | escolha | multipla | sim_nao_outro | livre
        #[arg(long, default_value = "livre")]
        kind: String,
        /// An option, as `id:label` or `id:label:detail`. Repeat once per option.
        #[arg(long = "opcao")]
        opcoes: Vec<String>,
        /// WHY you are asking, and what changes in each path. The human reads this hours later.
        #[arg(long, default_value = "")]
        contexto: String,
        /// Suggested option id. A suggestion — never applied on its own.
        #[arg(long, default_value = "")]
        auto: String,
        /// Checklist item id this answer unblocks. Repeat.
        #[arg(long = "linka")]
        linka: Vec<String>,
    },
    /// The question queue: open, answered-awaiting-review, and reviewed (`questions` still works).
    #[command(alias = "questions")]
    Fila {
        /// Machine-readable output (the GUI reads this).
        #[arg(long)]
        json: bool,
    },
    /// Answer a queued question by id: option ids and/or free text. Used by the GUI.
    Reply {
        /// Question id, or a text fragment of its title.
        alvo: String,
        /// Chosen option id. Repeat for `multipla`.
        #[arg(long = "escolha")]
        escolhas: Vec<String>,
        /// Free text (for `livre`, or the "other" of `sim_nao_outro`).
        #[arg(long, default_value = "")]
        texto: String,
    },
    /// Record the MACHINE's review of an answer — this, and not the answer, is what releases.
    ///
    /// The human asked for it in these words: "vc tem que revisar minha resposta, não passar
    /// direto". An answer that does not resolve the question asked must be re-asked, not guessed.
    Review {
        /// Question id, or a text fragment of its title.
        alvo: String,
        /// What you concluded by READING the answer.
        texto: Vec<String>,
    },
    /// Add a demand to the inbox WITHOUT touching the checklist (safe while an agent runs).
    Add { texto: Vec<String> },
    /// Inbox: list, organize a demand into items, or merge them into the checklist.
    Caixa {
        #[command(subcommand)]
        sub: CaixaCmd,
    },
    /// End the run (hooks become inert again).
    Stop,
    /// Run overdev with an attached `claude` agent in a PTY (monitors + auto-continue).
    Run {
        /// Max number of `continue` nudges injected when the agent goes idle.
        #[arg(long)]
        max: Option<u64>,
        /// Skip the confirmation prompt (the agent WILL touch this project).
        #[arg(long)]
        yes: bool,
    },
    /// Snapshot the `.schematize/overdev/` artifacts into the local DB (versioned backup).
    Snapshot,
    /// List the local DB snapshot history for this project (newest first).
    History {
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Machine-readable output (the window reads this).
        #[arg(long)]
        json: bool,
    },
    /// Restore a snapshot (by id) back to its original path.
    Restore { id: i64 },
    /// Inject `/eng-load` into a `claude` session to load the engineering precepts.
    Load,
    /// Inject `/eng-index` into a `claude` session to (re)index the project.
    Index,
    /// Desktop integration: put this app in the applications menu, or take it out.
    ///
    /// **O app instala a PRÓPRIA integração** (ADR-0019): quem baixa o binário do release ou
    /// compila do fonte tem de conseguir o ícone também, sem passar pelo hub.
    Desktop {
        /// Write the icon and the .desktop entry.
        #[arg(long)]
        install: bool,
        /// Remove the .desktop entry (the icons stay — they are inert and shared).
        #[arg(long)]
        remover: bool,
    },
    /// Print the completion log (HH:MM:SS local + item) from .schematize/overdev/completions.json.
    Log,
    /// The run monitor: open machine items and the recent completions (cheap; the window polls it).
    Monitor {
        /// Machine-readable output (the window reads this).
        #[arg(long)]
        json: bool,
    },
    /// Tokens and model of the project, from the Claude transcripts (HEAVY: do not poll it).
    Tokens {
        /// Machine-readable output (the window reads this).
        #[arg(long)]
        json: bool,
    },
    /// Where PLAN.md, CHECKLIST.md and NOTAS.md of this run live (the window opens them in the editor).
    Arquivos {
        /// Machine-readable output (the window reads this).
        #[arg(long)]
        json: bool,
    },
    /// The human notes (NOTAS.md) as a document.
    Notas {
        /// Machine-readable output (the window reads this).
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum CaixaCmd {
    /// Show what is captured and not yet in the checklist.
    List {
        /// Machine-readable output (the window reads this).
        #[arg(long)]
        json: bool,
    },
    /// Record the items an agent extracted from a demand (moves it to `processado`).
    Organizar {
        /// Demand id (see `caixa list`).
        id: String,
        /// One checklist item; repeat the flag for each.
        #[arg(long = "item", required = true)]
        itens: Vec<String>,
    },
    /// Merge every organized demand into the checklist (atomic, under lock).
    Merge,
    /// Open an agent in a terminal to organize the pending demands.
    Agente,
}

#[derive(Subcommand)]
pub enum ProjectsCmd {
    /// List every project (pinned + detected from the dev dirs), flagging the pinned.
    List,
    /// Pin a folder as a first-class project (always listed, even without a git marker).
    Add { path: String },
    /// Unpin a previously pinned folder.
    Remove { path: String },
    /// Drop a `.schematize` marker file in the folder (default: cwd) → a project that
    /// stops the scan from descending (turns an umbrella into ONE project).
    Mark { path: Option<String> },
    /// Remove the `.schematize` marker from the folder (default: cwd).
    Unmark { path: Option<String> },
}

#[derive(Subcommand)]
pub enum Auto {
    /// Enable and start the agent (systemd --user + XDG autostart).
    Enable,
    /// Disable and remove the autostart.
    Disable,
}

/// Ferramentas do GRAFO do índice.
///
/// **O exportador vive no `panel/`**, que é o mesmo parser que o painel HTML usa. Ter dois
/// leitores do formato do índice é o que esta extradição desfaz — e por isso o `graph` veio
/// para cá junto com o painel, mesmo o índice sendo conceito do hub.
#[derive(Subcommand)]
pub enum GraphCmd {
    /// Export the index as an Obsidian vault (markdown + [[wikilinks]]).
    Obsidian {
        /// Output directory (default: <project>_archive/obsidian).
        #[arg(long)]
        out: Option<String>,
    },
}
