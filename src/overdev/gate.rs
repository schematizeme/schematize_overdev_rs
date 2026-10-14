//! O GATE do overdev — a peça que decide se o agente PODE parar. `check`/`guard`
//! são o que o Stop hook do Claude Code chama; enquanto sobrar item aberto sem
//! pergunta parkeada, a parada é rejeitada.

use super::*;

/// Roda o gate opcional (.overdev/gate.sh). true se não há gate ou se passa.
pub(crate) fn gate_ok() -> bool {
    let g = dir().join("gate.sh");
    if !g.exists() {
        return true;
    }
    // `to_str()` devolve None em caminho não-UTF-8. Com `unwrap()` isso derrubava o STOP
    // HOOK — o processo que decide se o agente pode parar — em vez de decidir. Um projeto
    // numa pasta com byte inválido travava o overdev com pânico, não com veredito.
    // Sem gate legível, o veredito seguro é "não bloqueia": o gate é OPCIONAL, e um erro
    // de leitura nosso não pode virar prisão pro usuário.
    match g.to_str() {
        Some(caminho) => gate_roda(&*crate::ambiente::plat().shell, caminho),
        None => true,
    }
}

/// **O quê:** roda `bash <caminho>` pelo seam `Shell` e diz se saiu 0.
///
/// **Onde:** [`gate_ok`] e os testes (com um `Shell` falso). O caminho vai entre aspas simples
/// (`'` vira `'\''`): ele passa por UM shell e um nome com espaço ou `;` não pode virar comando.
/// Falha ao lançar conta como gate vermelho, igual a antes.
pub(crate) fn gate_roda(shell: &dyn schematize_plataforma::seams::Shell, caminho: &str) -> bool {
    let cmd = format!("bash {}", crate::agentrun::aspas_sh(caminho));
    shell.run(&cmd).is_ok_and(|o| o.status.success())
}

pub(crate) fn drain_stdin() {
    let mut buf = String::new();
    let _ = std::io::stdin().read_to_string(&mut buf);
}

pub(crate) fn print_block(reason: &str) {
    let v = serde_json::json!({ "decision": "block", "reason": reason });
    println!("{v}");
}

/// Veredito do Stop hook, decidido de forma PURA (testável) a partir das contagens.
/// "Concluído de verdade" = zero `- [ ]` E zero `- [H ]`. On-hold nunca bloqueia.
#[derive(PartialEq, Eq, Debug)]
pub(crate) enum StopDecision {
    /// Parada legítima e silenciosa (nada aberto, gate verde).
    Permit,
    /// Máquina terminou o dela, mas restam itens HUMANOS. Permite parar,
    /// porém informa (não bloqueia) — o humano fecha pela GUI/CLI.
    PermitWithHuman(String),
    /// Rejeita a parada com a mensagem de bloqueio.
    Block(String),
}

/// Regra do check: enquanto houver `- [ ]` (máquina) → BLOCK. Se não houver `- [ ]`
/// mas houver `- [H ]` (humano) → PERMITE parar e lista os pendentes humanos.
/// Gate falhando com tudo de máquina feito também BLOCK (a máquina conserta).
pub(crate) fn decide_stop(
    c: &Counts,
    gate_ok: bool,
    open_items: &[String],
    human_items: &[String],
    it: u64,
    max: u64,
) -> StopDecision {
    if c.open > 0 {
        let items = open_items.join("\\n");
        return StopDecision::Block(format!(
            "MODO OVERDEV — NÃO PARE E NÃO DIGA QUE TERMINOU. Faltam {} item(ns) de MÁQUINA abertos ({} on-hold · {} humanos, ciclo {it}/{max}). NÃO fale com o usuário ainda. Pegue o PRÓXIMO '- [ ]', implemente, VERIFIQUE (gate/teste), marque '- [x]'. Se travar por dúvida: NÃO use AskUserQuestion — escreva a pergunta em ./{QUESTIONS_FILE}, marque o item '- [~]' (on-hold) e SIGA. Itens de máquina abertos:\\n{items}\\nOs '- [H ]' (humanos) NÃO são seus — deixe pro humano. Só pare quando não sobrar '- [ ]' e o gate passar.",
            c.open, c.hold, c.human
        ));
    }
    // Sem trabalho de máquina em aberto. Gate ainda manda na máquina.
    if !gate_ok {
        return StopDecision::Block(format!(
            "MODO OVERDEV — NÃO PARE. O checklist de máquina está todo marcado mas o gate (.schematize/overdev/gate.sh) FALHOU (ciclo {it}/{max}). Conserte o que o gate acusa e rode de novo. Só pare com o gate verde."
        ));
    }
    if c.human > 0 {
        let items = human_items.join("\n");
        return StopDecision::PermitWithHuman(format!(
            "MODO OVERDEV — a parte da MÁQUINA está concluída (gate verde). Faltam {} item(ns) HUMANOS — feche pela GUI ou por `schematize overdev human \"<texto>\"`:\n{}",
            c.human, items
        ));
    }
    StopDecision::Permit
}

/// Stop hook: rejeita a parada enquanto houver `- [ ]` (trabalho de máquina).
/// Quando só restam `- [H ]` (humanos), PERMITE parar e informa os pendentes.
pub fn check() {
    drain_stdin();
    // AUTO-CURA do hook quebrado, ANTES do retorno de inerte (§ UX de massa: o software
    // se conserta, não manda o usuário editar settings.json).
    //
    // Por que aqui: o estrago mora no settings.json de QUEM LIGOU o overdev — muitas vezes
    // o do projeto, com o hook shell legado em caminho RELATIVO (`bash
    // .claude/hooks/overdev-stop.sh`). Esse falha em toda parada, com o overdev ativo ou
    // não, e o `schematize` só é executado ali por este próprio hook: era o único ponto que
    // roda no projeto certo, na hora certa, e ninguém aproveitava. O `doctor` reparava, mas
    // exigia o usuário saber que existe e rodar à mão.
    //
    // Seguro por construção: `repara_hooks_em` só toca comando que é NOSSO (cita `overdev
    // check`/`overdev guard` ou o `overdev-stop.sh`) E está quebrado; escreve só nesse caso,
    // e é idempotente. Best-effort — falhar o reparo nunca pode alterar a decisão de parar.
    reparar_hooks_do_projeto();
    let st = match load() {
        Some(s) if s.mode == "active" => s,
        _ => return, // inerte
    };
    // LOG de conclusões (best-effort): detecta os `- [x]` novos por DIFF e registra
    // a hora. Fica na trilha do Stop hook, então também pega run EXTERNO (fora do app).
    // Não altera a decisão de parar.
    let _ = record_completions(Path::new("."));
    // Archive obrigatório em dia: re-espelha os artefatos (o agente vai escrevendo PLAN/DECISOES/etc).
    ensure_archive_mirror(Path::new("."));
    // budget
    let mut it: u64 =
        fs::read_to_string(iters_file()).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
    it += 1;
    let _ = fs::write(iters_file(), it.to_string());
    if it > st.max_iters {
        return; // guardrail anti-loop: permite parar
    }
    let c = counts();
    match decide_stop(&c, gate_ok(), &open_items(), &human_items(), it, st.max_iters) {
        StopDecision::Permit => {}
        // Permite a parada (não bloqueia), mas mostra os itens humanos pendentes.
        StopDecision::PermitWithHuman(msg) => println!("{msg}"),
        StopDecision::Block(reason) => print_block(&reason),
    }
}

/// Regrava os hooks do overdev que estiverem quebrados, no settings do usuário e no do
/// projeto corrente. Chamada pelo Stop hook (ver [`check`]).
///
/// **Entrada:** nenhuma — usa o cwd como projeto (é onde o Claude Code roda o hook).
/// **Saída:** nenhuma. **Efeitos:** pode reescrever `.claude/settings.json` (do projeto) e
/// o settings do usuário; erro é engolido de propósito — um reparo que falha não pode
/// derrubar o gate nem virar ruído na parada do agente.
fn reparar_hooks_do_projeto() {
    let projeto = std::env::current_dir().ok();
    for arquivo in crate::settings::arquivos_de_settings(projeto.as_deref()) {
        let _ = crate::settings::repara_hooks_em(crate::ambiente::hooks(), &arquivo);
    }
}

/// PreToolUse hook (matcher AskUserQuestion): veta a pergunta bloqueante em overdev.
pub fn guard() {
    drain_stdin();
    let active = matches!(load(), Some(s) if s.mode == "active");
    if !active {
        return; // fora de overdev: libera normalmente
    }
    let reason = format!(
        "VETADO em OVERDEV: nada de parar pra perguntar com pool bloqueante. Escreva a pergunta em ./{QUESTIONS_FILE} (na base do projeto), marque o item correspondente como '- [~]' (on-hold) no .schematize/overdev/CHECKLIST.md com `schematize overdev hold`, e CONTINUE os outros itens. As perguntas serão respondidas quando o usuário voltar."
    );
    let v = serde_json::json!({
        "decision": "block",
        "reason": reason,
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason
        }
    });
    println!("{v}");
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use schematize_plataforma::seams::Shell;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{ExitStatus, Output};
    use std::sync::Mutex;

    /// `Shell` falso: devolve o código dado e guarda o comando recebido.
    struct Falso {
        codigo: i32,
        visto: Mutex<Vec<String>>,
    }
    impl Shell for Falso {
        /// **O quê:** registra `cmd` e devolve saída com o código programado. **Onde:** testes.
        fn run(&self, cmd: &str) -> std::io::Result<Output> {
            if let Ok(mut v) = self.visto.lock() {
                v.push(cmd.to_string());
            }
            Ok(Output {
                status: ExitStatus::from_raw(self.codigo << 8),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        }
        /// **O quê:** sempre existe. **Onde:** testes.
        fn existe(&self, _bin: &str) -> bool {
            true
        }
    }

    /// O gate roda `bash '<caminho>'` pelo seam, e o código de saída decide o veredito.
    #[test]
    fn o_gate_passa_pelo_shell_da_plataforma() {
        let verde = Falso { codigo: 0, visto: Mutex::new(vec![]) };
        assert!(gate_roda(&verde, "/p/meu projeto/gate.sh"));
        assert_eq!(*verde.visto.lock().unwrap(), ["bash '/p/meu projeto/gate.sh'"]);
        assert!(!gate_roda(&Falso { codigo: 1, visto: Mutex::new(vec![]) }, "/p/gate.sh"));
    }

    /// Aspa simples no caminho não escapa do quoting (não vira comando).
    #[test]
    fn caminho_hostil_fica_entre_aspas() {
        let f = Falso { codigo: 0, visto: Mutex::new(vec![]) };
        assert!(gate_roda(&f, "/p/a'; rm -rf ~; '/gate.sh"));
        assert_eq!(*f.visto.lock().unwrap(), ["bash '/p/a'\\''; rm -rf ~; '\\''/gate.sh'"]);
    }
}
