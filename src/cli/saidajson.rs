//! A saída de MÁQUINA — o contrato que a janela e o hub leem.
//!
//! **O quê:** `fila --json` (a fila de perguntas do quiz) e `progresso --json` (em que pé está
//! o run).
//!
//! **Onde:** a janela deste app, e o hub quando delega a aba.
//!
//! ## Escrito à MÃO, como nos irmãos
//!
//! O documento é um CONTRATO com outro processo. Derivar faria o nome de um campo mudar quando
//! alguém renomeia um campo do domínio, em silêncio e sem passar por revisão; escrito à mão, a
//! mudança de forma aparece no diff — e há teste que fixa cada nome.

use overdev_casa::overdev;

/// **O quê:** escapa uma string para dentro de JSON.
///
/// **Onde:** todo campo de texto deste módulo. Um item de checklist é texto livre escrito por
/// quem quiser: quebra de linha e aspas são o caso normal, não a exceção.
pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o
}

/// **O quê:** o progresso do run neste diretório.
///
/// **Onde:** a janela e o hub.
///
/// **As quatro classes saem SEPARADAS**, e não só o total: "17 abertos" não diz se são de
/// máquina (que o agente fecha sozinho) ou humanos (que só a pessoa decide). Somá-las faria a
/// tela pedir ação a quem não tem nada a fazer.
pub fn progresso(json: bool) -> Result<(), String> {
    let raiz = crate::cli::comandos::cwd_project()?;
    let p = overdev::progresso::progress_at(&raiz);
    if !json {
        println!(
            "{} · {} feito(s) · {} aberto(s) · {} on-hold · {} humano(s) · {} recusado(s)",
            if p.mode.is_empty() { "sem run" } else { &p.mode },
            p.done,
            p.open,
            p.hold,
            p.human,
            p.recusado
        );
        return Ok(());
    }
    // **`modo` vazio e `modo: "stopped"` são coisas diferentes**, e o documento diz as duas:
    // "não há run neste diretório" e "há um run, parado" pedem telas diferentes.
    println!(
        "{{\"raiz\":\"{}\",\"modo\":\"{}\",\"feitos\":{},\"abertos\":{},\"on_hold\":{},\
         \"humanos\":{},\"recusados\":{},\"iteracoes\":{},\"teto\":{}}}",
        esc(&raiz.display().to_string()),
        esc(&p.mode),
        p.done,
        p.open,
        p.hold,
        p.human,
        p.recusado,
        p.iterations,
        p.max_iters
    );
    Ok(())
}

/// **O quê:** o estado dos HOOKS do Claude Code: registrados? apontando para este binário?
///
/// **Onde:** o `doctor` do hub, e a mão de quem quer conferir.
///
/// **`--repair` é o conserto, e ele existe porque o hook APODRECE sozinho.** O comando gravado
/// no `settings.json` procura o binário numa lista de caminhos; se a instalação mudar de
/// lugar, ou se a forma do comando mudar (como mudou na E6, para procurar este app antes do
/// hub), o hook antigo continua lá, válido e desatualizado. Reescrever é barato; descobrir
/// tarde que o `guard` não roda há semanas, não.
pub fn hooks(repair: bool, json: bool) -> Result<(), String> {
    use overdev_casa::settings;
    let exe = overdev_casa::nucleo::util::self_exe();
    if repair {
        settings::refresh_hooks(&exe)?;
    }
    let ligados = settings::overdev_enabled();
    let em_dia = settings::hooks_atualizados(&exe);
    let valido = settings::settings_valid();
    // **`None` é AUSENTE, e não inválido.** Um `settings.json` que não existe é o caso de quem
    // nunca ligou os hooks; dizer "INVÁLIDO" ali mandaria a pessoa consertar um arquivo que
    // não tem nada de errado — ele só não existe ainda.
    if !json {
        println!(
            "settings.json : {}",
            match valido {
                Some(true) => "válido",
                Some(false) => "INVÁLIDO",
                None => "ausente",
            }
        );
        println!("hooks         : {}", if ligados { "ligados" } else { "desligados" });
        println!("em dia        : {}", if em_dia { "sim" } else { "NÃO — rode `hooks --repair`" });
        return Ok(());
    }
    println!(
        "{{\"settings\":{},\"ligados\":{},\"em_dia\":{}}}",
        match valido {
            Some(true) => "\"valido\"",
            Some(false) => "\"invalido\"",
            None => "\"ausente\"",
        },
        ligados,
        em_dia
    );
    Ok(())
}
