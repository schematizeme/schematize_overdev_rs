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
use overdev_casa::overdev::pergunta::Pergunta;

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
    println!("{}", documento_progresso(&raiz.display().to_string(), &p));
    Ok(())
}

/// **O quê:** o documento `progresso --json` — função pura, para o teste travar o shape.
///
/// **Onde:** [`progresso`].
///
/// **Sem run, `modo` e `teto` são `null`, nunca `""`/`0`:** o `""` obrigaria o leitor a
/// adivinhar que string vazia significa "ausente", e o `0` de teto seria lido como "teto zero".
/// `iteracoes` fica número: contagem de ciclos já gastos, e zero é verdade.
pub fn documento_progresso(raiz: &str, p: &overdev::progresso::Progress) -> String {
    let sem_run = p.mode.is_empty();
    format!(
        "{{\"raiz\":\"{}\",\"modo\":{},\"feitos\":{},\"abertos\":{},\"on_hold\":{},\
         \"humanos\":{},\"recusados\":{},\"iteracoes\":{},\"teto\":{}}}",
        esc(raiz),
        texto_ou_null(&p.mode),
        p.done,
        p.open,
        p.hold,
        p.human,
        p.recusado,
        p.iterations,
        if sem_run { "null".to_string() } else { p.max_iters.to_string() }
    )
}

/// **O quê:** string JSON, ou `null` quando vazia.
///
/// **Onde:** todo campo textual opcional dos documentos deste módulo.
pub fn texto_ou_null(s: &str) -> String {
    if s.is_empty() {
        "null".to_string()
    } else {
        format!("\"{}\"", esc(s))
    }
}

/// **O quê:** a lista de strings JSON.
///
/// **Onde:** `linka` e `escolhas` no documento da fila.
fn lista_de_textos(v: &[String]) -> String {
    format!("[{}]", v.iter().map(|s| texto_json(s)).collect::<Vec<_>>().join(","))
}

/// **O quê:** string JSON SEMPRE (vazia vira `""`) — item de lista não pode virar `null`.
///
/// **Onde:** [`lista_de_textos`].
fn texto_json(s: &str) -> String {
    format!("\"{}\"", esc(s))
}

/// **O quê:** uma pergunta como objeto JSON, com o `estado` calculado.
///
/// **Onde:** [`documento_fila`]. `estado` é slug: `aberta`, `aguardando_revisao`, `revisada`.
fn pergunta_json(p: &Pergunta) -> String {
    let estado = if p.aberta() {
        "aberta"
    } else if p.aguarda_revisao() {
        "aguardando_revisao"
    } else {
        "revisada"
    };
    let opcoes = p
        .opcoes
        .iter()
        .map(|o| {
            format!(
                "{{\"id\":\"{}\",\"rotulo\":\"{}\",\"detalhe\":{}}}",
                esc(&o.id),
                esc(&o.rotulo),
                texto_ou_null(&o.detalhe)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let resposta = match &p.resposta {
        None => "null".to_string(),
        Some(r) => format!(
            "{{\"em\":{},\"por\":\"{}\",\"escolhas\":{},\"texto\":{},\"revisada\":{},\
             \"revisao\":{}}}",
            r.em,
            esc(&r.por),
            lista_de_textos(&r.escolhas),
            texto_ou_null(&r.texto),
            r.revisada,
            texto_ou_null(&r.revisao)
        ),
    };
    format!(
        "{{\"id\":\"{}\",\"estado\":\"{}\",\"kind\":\"{}\",\"criada_em\":{},\
         \"fase\":{},\"titulo\":\"{}\",\"contexto\":{},\"opcoes\":[{}],\"auto\":{},\
         \"linka\":{},\"resposta\":{}}}",
        esc(&p.id),
        estado,
        p.kind.slug(),
        p.criada_em,
        texto_ou_null(&p.fase),
        esc(&p.titulo),
        texto_ou_null(&p.contexto),
        opcoes,
        texto_ou_null(&p.auto),
        lista_de_textos(&p.linka),
        resposta
    )
}

/// **O quê:** o documento `fila --json` — as perguntas em três grupos + os arquivos ilegíveis.
///
/// **Onde:** `cli::perguntas::questions` (o comando `fila`, com `questions` de apelido). A
/// janela lê os três grupos; cada pergunta traz também o `estado` por extenso.
///
/// **Os nomes dos grupos e dos campos são os que a janela já lê** (`abertas`,
/// `aguardando_revisao`, `revisadas`): trocar a escrita sem trocar os nomes é o que mantém a
/// janela funcionando. Ausente é `null`, nunca `""`.
pub fn documento_fila(todas: &[Pergunta], ilegiveis: &[String]) -> String {
    let grupo = |f: &dyn Fn(&Pergunta) -> bool| {
        format!(
            "[{}]",
            todas.iter().filter(|p| f(p)).map(pergunta_json).collect::<Vec<_>>().join(",")
        )
    };
    format!(
        "{{\"abertas\":{},\"aguardando_revisao\":{},\"revisadas\":{},\"ilegiveis\":{}}}",
        grupo(&|p| p.aberta()),
        grupo(&|p| p.aguarda_revisao()),
        grupo(&|p| p.resposta.as_ref().is_some_and(|r| r.revisada)),
        lista_de_textos(ilegiveis)
    )
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
