//! Subcomandos do OVERDEV: rodar, dividir em K agentes, snapshot/histórico/restore
//! do DB local e o log de conclusões.

use crate::cli::args::Cmd;
use crate::cli::prompt::confirm;
use overdev_casa::agentrun;
use overdev_casa::agentrun::AgentRunner;
use overdev_casa::{overdev, overdevdb};

/// `schematize overdev run [--max N] [--yes]` — dispara o `claude` acoplado no
/// diretório atual e monitora (auto-continue). Guardrail: mostra o comando do
/// agente e confirma antes (o agente MEXE no projeto), a menos de `--yes`.
/// `schematize overdev split K` — divide o checklist em K parts e (com --dispatch) lança K claudes,
/// tudo dentro do teto seguro do governador (`schematize agents`).
pub fn overdev_split(k: usize, dispatch: bool, force: bool) -> Result<(), String> {
    let project = std::env::current_dir().map_err(|e| format!("cwd inacessível: {e}"))?;
    let b = overdev_casa::agents::budget();
    let plan = b.split_plan(k);

    println!("Governador de concorrência (máquina inteira):");
    println!(
        "  teto seguro: {} · rodando agora: {} · disponível: {}",
        b.total_cap, b.snap.running_claudes, b.available
    );
    println!(
        "  split em {} claude(s): {} subagents cada (total {}/{} do teto)",
        plan.mains, plan.subagents_each, plan.total_used, plan.cap
    );

    if k > b.total_cap && !force {
        return Err(format!(
            "{k} claudes principais passa do teto seguro ({}). Reduza o K ou use --force (pode travar a máquina).",
            b.total_cap
        ));
    }
    if dispatch && b.available < k && !force {
        return Err(format!(
            "só há {} slot(s) livre(s) na máquina agora (teto {} − {} rodando); lançar {k} travaria. Espere liberar ou use --force.",
            b.available, b.total_cap, b.snap.running_claudes
        ));
    }

    let res = overdev::split(&project, k)?;
    println!("\n✓ dividido: {} itens em {} parte(s):", res.moved, res.parts.len());
    for (i, (f, n)) in res.parts.iter().zip(&res.per_part).enumerate() {
        println!("  part {:>2}: {n:>3} item(ns)  → {}", i + 1, f.display());
    }

    if !dispatch {
        println!("\nRevise os parts e rode com --dispatch pra lançar os {k} claudes (ou abra cada um você mesmo).");
        return Ok(());
    }

    println!("\nLançando {k} claude(s) — um por part…");
    for (i, f) in res.parts.iter().enumerate() {
        let rel = f.strip_prefix(&project).unwrap_or(f);
        let prompt = format!(
            "Rode o overdev deste projeto cuidando APENAS do arquivo `{}` (sua fatia do split). Feche \
             TODOS os itens `- [ ]` dele com prova, seguindo a disciplina do overdev. Você pode usar até \
             {} subagents em paralelo — NÃO ultrapasse, pra não travar a máquina (há outros claudes \
             rodando as outras fatias). Não toque nos outros part-*.md.",
            rel.display(),
            plan.subagents_each
        );
        match agentrun::launch_prompt_in_terminal(&project, &prompt) {
            Ok(_) => println!("  ✓ claude {} lançado ({})", i + 1, rel.display()),
            Err(e) => println!("  ✗ claude {} falhou: {e}", i + 1),
        }
    }
    Ok(())
}

pub fn overdev_run(max: Option<u64>, yes: bool) -> Result<(), String> {
    let project = std::env::current_dir().map_err(|e| format!("cwd inacessível: {e}"))?;
    let max = max.unwrap_or(agentrun::DEFAULT_MAX_NUDGES);
    let objetivo = overdev::objetivo_at(&project).unwrap_or_default();
    let runner = agentrun::ClaudeRunner;
    println!("Vai disparar o agente acoplado neste projeto:");
    println!("  projeto: {}", project.display());
    println!("  comando: {}", runner.command_line(&objetivo));
    println!("  auto-continue: até {max} nudge(s) quando o agente ficar ocioso com item aberto.");
    if !yes && !confirm("Disparar o agente `claude` acoplado neste projeto? [s/N]") {
        return Err("cancelado.".to_string());
    }
    agentrun::run_attached(&project, &runner, max)
}

/// cwd como raiz do projeto (erro claro se inacessível).
pub fn cwd_project() -> Result<std::path::PathBuf, String> {
    std::env::current_dir().map_err(|e| format!("cwd inacessível: {e}"))
}

/// `schematize overdev snapshot` — grava no DB local as versões novas dos artefatos.
pub fn overdev_snapshot() -> Result<(), String> {
    let project = cwd_project()?;
    let n = overdevdb::snapshot(&project)?;
    if n == 0 {
        println!("nenhuma mudança — nada novo pra versionar.");
    } else {
        println!("{n} snapshot(s) novo(s) gravado(s) no DB local.");
    }
    Ok(())
}

/// `schematize overdev history [--limit N]` — tabela do histórico do projeto.
pub fn overdev_history(limit: usize) -> Result<(), String> {
    let project = cwd_project()?;
    let hist = overdevdb::history(&project, limit)?;
    if hist.is_empty() {
        println!("sem snapshots pra este projeto ainda (rode `schematize overdev snapshot`).");
        return Ok(());
    }
    println!("{:>6}  {:<19}  {:>8}  arquivo", "id", "quando", "bytes");
    for m in hist {
        println!("{:>6}  {:<19}  {:>8}  {}", m.id, fmt_ts(m.ts), m.size, m.file);
    }
    println!("(veja um: `schematize overdev restore <id>`)");
    Ok(())
}

/// `schematize overdev restore <id>` — regrava o snapshot no caminho original.
pub fn overdev_restore(id: i64) -> Result<(), String> {
    let project = cwd_project()?;
    let dest = overdevdb::restore(id, &project)?;
    println!("snapshot {id} restaurado em {}", dest.display());
    Ok(())
}

/// Formata um epoch secs em `AAAA-MM-DD HH:MM` (UTC, sem crate de data).
pub fn fmt_ts(ts: i64) -> String {
    // Cálculo civil a partir do epoch (algoritmo de Howard Hinnant), UTC.
    let days = ts.div_euclid(86_400);
    let secs = ts.rem_euclid(86_400);
    let (h, mi) = (secs / 3600, (secs % 3600) / 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}")
}

/// Offset local em segundos (via `date +%z`, ex.: `-0300`). Fallback 0 (UTC) se
/// o `date` falhar ou vier num formato inesperado.
pub fn local_offset_secs() -> i64 {
    let out = match std::process::Command::new("date").arg("+%z").output() {
        Ok(o) if o.status.success() => o.stdout,
        _ => return 0,
    };
    let s = String::from_utf8_lossy(&out);
    let s = s.trim();
    // Formato esperado: sinal + HHMM (ex.: "-0300", "+0530").
    if s.len() < 5 {
        return 0;
    }
    let sign: i64 = if s.starts_with('-') { -1 } else { 1 };
    let digits: Vec<char> = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 4 {
        return 0;
    }
    let hh: i64 = format!("{}{}", digits[0], digits[1]).parse().unwrap_or(0);
    let mm: i64 = format!("{}{}", digits[2], digits[3]).parse().unwrap_or(0);
    sign * (hh * 3600 + mm * 60)
}

/// Hora local `HH:MM:SS` de um epoch (secs), dado o offset local. PURO/testável.
pub fn fmt_hms(ts: i64, offset: i64) -> String {
    let secs = (ts + offset).rem_euclid(86_400);
    format!("{:02}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60)
}

/// `schematize overdev log` — lista as conclusões (`- [x]`) do CHECKLIST com a hora
/// local em que foram detectadas (lê `.overdev/completions.json`, sem gravar).
pub fn overdev_log() {
    let project = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let comps = overdev::completions(&project);
    if comps.is_empty() {
        println!("sem conclusões registradas ainda (.schematize/overdev/completions.json).");
        return;
    }
    let off = local_offset_secs();
    println!("conclusões ({}):", comps.len());
    for c in &comps {
        println!("  {}  {}", fmt_hms(c.ts, off), c.text);
    }
}

/// `schematize overdev load|index` — dispara uma sessão `claude` one-shot no cwd
/// com o comando dado. Se `claude` não estiver no PATH, só imprime a dica.
pub fn overdev_agent_cmd(cmd: &str) -> Result<(), String> {
    if !agentrun::claude_in_path() {
        println!("`claude` não está no PATH. Rode manualmente na pasta do projeto: claude {cmd}");
        return Ok(());
    }
    println!("disparando sessão `claude` no diretório atual com: {cmd}");
    match std::process::Command::new("claude").arg(cmd).status() {
        Ok(st) if st.success() => Ok(()),
        Ok(st) => {
            println!("a sessão `claude` saiu com {st}. Se preciso, rode à mão: claude {cmd}");
            Ok(())
        }
        Err(e) => {
            println!("não consegui disparar `claude` ({e}). Rode à mão: claude {cmd}");
            Ok(())
        }
    }
}

/// **O quê:** despacha o subcomando.
///
/// **Onde:** `main`. Devolve `Result` em vez de imprimir e sair aqui dentro, porque o código de
/// saída é parte do contrato — e no caso do `guard` ele é MAIS que isso: o hook `PreToolUse` do
/// Claude Code usa o código de saída para vetar uma chamada de ferramenta.
pub fn executar(sub: Cmd) -> Result<(), String> {
    match sub {

        Cmd::Enable => overdev_casa::settings::enable(&overdev_casa::nucleo::util::self_exe()),
        Cmd::Disable => overdev_casa::settings::disable(),
        Cmd::Start { objetivo, max } => overdev_casa::overdev::start(&objetivo.join(" "), max),
        Cmd::Terminal => crate::cli::comandos::cwd_project().and_then(|p| {
            overdev_casa::agentrun::abrir_terminal_no_projeto(&p)
                .map(|t| println!("terminal `{t}` aberto em {} — o claude sobe com o bypass ligado.", p.display()))
        }),
        Cmd::Supervise { max } => crate::cli::comandos::cwd_project().map(|projeto| {
            let teto = max.unwrap_or(overdev_casa::overdev::supervisor::MAX_RELANCAMENTOS);
            println!(
                "Supervisionando {} — relanço o agente se ele morrer com item de máquina aberto (teto {teto}).",
                projeto.display()
            );
            let n = overdev_casa::overdev::supervisor::supervise(&projeto, teto);
            println!("supervisor encerrado após {n} relançamento(s).");
        }),
        Cmd::Split { k, dispatch, force } => crate::cli::comandos::overdev_split(k, dispatch, force),
        Cmd::Check => {
            overdev_casa::overdev::check();
            Ok(())
        }
        Cmd::Guard => {
            overdev_casa::overdev::guard();
            Ok(())
        }
        Cmd::Agents { json, split } => agents_cmd(json, split),
        Cmd::Archive => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            overdev_casa::archive::sync(&cwd).map(|msg| println!("{msg}"))
        }
        Cmd::Panel => overdev_casa::panel::open(),
        Cmd::Graph { sub } => match sub {
            crate::cli::args::GraphCmd::Obsidian { out } => {
                overdev_casa::panel::export_obsidian(out)
            }
        },
        Cmd::Progresso { json } => crate::cli::saidajson::progresso(json),
        Cmd::Hooks { repair, json } => crate::cli::saidajson::hooks(repair, json),
        Cmd::Status => {
            overdev_casa::overdev::status();
            Ok(())
        }
        Cmd::Hold { texto } => overdev_casa::overdev::hold(&texto.join(" ")),
        Cmd::Park { item, pergunta } => overdev_casa::overdev::park(&item, &pergunta.join(" ")),
        Cmd::Human { texto, done } => {
            let t = texto.join(" ");
            let sub = if t.trim().is_empty() { None } else { Some(t.as_str()) };
            overdev_casa::overdev::human_done(sub, done)
        }
        Cmd::Note { texto, kind } => overdev_casa::overdev::note(&kind, &texto.join(" ")),
        Cmd::Answer { alvo, texto } => {
            overdev_casa::overdev::resolver(alvo_humano(&alvo), overdev_casa::overdev::resposta::Acao::Responder, &texto.join(" "))
        }
        Cmd::Refuse { alvo, texto } => {
            overdev_casa::overdev::resolver(alvo_humano(&alvo), overdev_casa::overdev::resposta::Acao::Recusar, &texto.join(" "))
        }
        Cmd::Ask { titulo, kind, opcoes, contexto, auto, linka } => crate::cli::perguntas::ask(
            &titulo.join(" "),
            &kind,
            &opcoes,
            &contexto,
            &auto,
            &linka,
        ),
        Cmd::Questions { json } => crate::cli::perguntas::questions(json),
        Cmd::Reply { alvo, escolhas, texto } => {
            crate::cli::perguntas::reply(&alvo, &escolhas, &texto)
        }
        Cmd::Review { alvo, texto } => crate::cli::perguntas::review(&alvo, &texto.join(" ")),
        Cmd::Add { texto } => crate::cli::caixa::caixa_add(&texto.join(" ")),
        Cmd::Caixa { sub } => crate::cli::caixa::caixa_cmd(sub),
        Cmd::Stop => overdev_casa::overdev::stop(),
        Cmd::Run { max, yes } => overdev_run(max, yes),
        Cmd::Snapshot => overdev_snapshot(),
        Cmd::History { limit } => overdev_history(limit),
        Cmd::Restore { id } => overdev_restore(id),
        Cmd::Load => overdev_agent_cmd(overdev_casa::overdev::load_cmd()),
        Cmd::Index => overdev_agent_cmd(overdev_casa::overdev::index_cmd()),
        Cmd::Log => {
            overdev_log();
            Ok(())
        }
    }
}

/// `schematize lang [code] [--list]`.
/// `schematize agents` — imprime o orçamento de concorrência e persiste ~/.schematize/agents.json.
pub fn agents_cmd(json: bool, split: Option<usize>) -> Result<(), String> {
    let b = overdev_casa::agents::budget();
    let _ = overdev_casa::agents::persist(&b); // best-effort: outros (Claude/overdev/GUI) leem daqui.

    if json {
        let plan = split.map(|k| b.split_plan(k));
        let mut v = serde_json::json!({
            "total_cap": b.total_cap, "available": b.available,
            "cpu_cap": b.cpu_cap, "ram_cap": b.ram_cap, "load_cap": b.load_cap,
            "threads": b.snap.threads, "mem_available_mb": b.snap.mem_available_mb,
            "load1": b.snap.load1, "running_claudes": b.snap.running_claudes,
            "ram_tight": b.ram_tight,
        });
        if let Some(p) = plan {
            v["split"] = serde_json::json!({
                "mains": p.mains, "subagents_each": p.subagents_each, "total_used": p.total_used
            });
        }
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
        return Ok(());
    }

    let gb = |mb: u64| format!("{:.1} GB", mb as f64 / 1024.0);
    println!("\x1b[1mOrçamento de concorrência do Claude (máquina inteira)\x1b[0m");
    println!("  threads lógicos      : {}", b.snap.threads);
    println!("  reserva (respiro)    : {}", b.params.reserve);
    println!(
        "  RAM disponível       : {}  (≈{} por agent, −{:.0}% de margem)",
        gb(b.snap.mem_available_mb),
        gb(b.params.mb_per_agent),
        b.params.ram_margin * 100.0
    );
    println!("  load atual (1min)    : {:.2}", b.snap.load1);
    println!(
        "  claudes rodando AGORA: {}  (esta janela + outras + subagents)",
        b.snap.running_claudes
    );
    println!("  ─────────────────────");
    println!("  teto por CPU         : {}", b.cpu_cap);
    println!(
        "  teto por RAM         : {}{}",
        b.ram_cap,
        if b.ram_tight { "  \x1b[33m(RAM apertada — cuidado com swap)\x1b[0m" } else { "" }
    );
    println!("  teto por load        : {}", b.load_cap);
    println!("  \x1b[1mTETO TOTAL seguro    : {}\x1b[0m  (o menor dos três)", b.total_cap);
    println!("  \x1b[1;32mDISPONÍVEL p/ lançar : {}\x1b[0m  (teto − já rodando)", b.available);
    if let Some(k) = split {
        let p = b.split_plan(k);
        println!("\n  \x1b[1mSplit em {} claude(s) principal(is):\x1b[0m {} subagents cada  (total {}/{} do teto)", p.mains, p.subagents_each, p.total_used, p.cap);
    }
    Ok(())
}

/// **O quê:** interpreta o alvo de um item humano: número vira índice, resto vira texto.
///
/// **Onde:** os comandos que fecham, respondem ou recusam um item humano.
///
/// **Veio do `main.rs` do hub** e é PURA, então ganhou teste ao mudar de casa — lá ela não
/// tinha nenhum, porque estava no meio do despacho.
fn alvo_humano(s: &str) -> overdev_casa::overdev::resposta::Alvo {
    use overdev_casa::overdev::resposta::Alvo;
    match s.trim().parse::<usize>() {
        Ok(n) if n > 0 => Alvo::Indice(n),
        _ => Alvo::Texto(s.trim().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::alvo_humano;
    use overdev_casa::overdev::resposta::Alvo;

    /// **Número vira ÍNDICE e o resto vira TEXTO — e o zero não é índice.**
    ///
    /// Os itens humanos são contados a partir de 1 na tela. Um `Alvo::Indice(0)` não casaria
    /// com nada e o comando diria "não achei" sobre uma lista cheia, sem dizer por quê.
    #[test]
    fn o_alvo_distingue_indice_de_texto() {
        assert_eq!(alvo_humano("3"), Alvo::Indice(3));
        assert_eq!(alvo_humano("  2  "), Alvo::Indice(2), "espaço em volta não muda a leitura");
        assert_eq!(alvo_humano("0"), Alvo::Texto("0".into()), "zero não é índice");
        assert_eq!(alvo_humano("-1"), Alvo::Texto("-1".into()));
        assert_eq!(alvo_humano("tags"), Alvo::Texto("tags".into()));
        // Texto que COMEÇA com número continua sendo texto: "2 fatores" é um item, não o
        // segundo da lista.
        assert_eq!(alvo_humano("2 fatores"), Alvo::Texto("2 fatores".into()));
    }
}
