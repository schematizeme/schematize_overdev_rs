//! LANÇAMENTO JAULADO: cada `claude`/execução do overdev roda sob teto duro de memória.
//!
//! **O quê:** monta o argv jaulado (`comando_jaulado`) com unidade única por lançamento
//! (`schematize-<proj>-<seq>`), gera o wrapper do terminal, executa não-interativos
//! ([`executar_jaulado`]) e APURA as unidades que terminaram ([`apurar`]): estouro de memória é
//! a MORTE DA UNIDADE (veredito registrado em `jaulas.jsonl`), nunca da operação.
//! **Onde:** [`super::launch_prompt_in_terminal`] (lançamento) e `overdev::supervisor` (apuração
//! e prompt de re-decomposição). O supervisor NÃO roda dentro da jaula do filho e tem
//! `oom_score_adj` menor que o dos jaulados (800), então o kernel sacrifica o jaulado primeiro.
//!
//! ## Interativo vs. não-interativo
//! O `claude` do terminal é interativo (precisa de TTY): o wrapper roda `systemd-run --scope`
//! (herda o TTY) e grava `jaula-<unidade>.exit` (`<codigo> <Result>`); o scope NÃO usa `--collect`
//! para o `Result=oom-kill` sobreviver até o wrapper lê-lo. [`apurar`] converte esses arquivos
//! em registros. Backend `prlimit` (RLIMIT_AS) não serve ao `claude` (Node reserva endereço
//! virtual enorme), então só `systemd-run` conta como jaula para ele.

mod politica;
mod registro;

pub use politica::*;
pub use registro::{anexar, caminho, geracao_atual, ler, Registro, ARQUIVO};

use schematize_plataforma::jaula::{
    comando_jaulado, detectar_backend, executar,
    veredito::{classificar, Evidencia},
    Backend, Orcamento, TipoExecucao, Veredito,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Sequência de unidades DESTE processo (o pid completa a unicidade entre processos).
static UNIDADE_SEQ: AtomicU64 = AtomicU64::new(0);

/// Plano de UMA unidade jaulada: nome, orçamento e arquivos de controle.
#[derive(Debug, Clone)]
pub struct PlanoJaula {
    /// Nome único da unidade.
    pub unidade: String,
    /// Teto da unidade.
    pub orcamento: Orcamento,
    /// Geração de re-decomposição.
    pub geracao: u32,
    /// Onde o wrapper grava `<codigo> <Result>`.
    pub arq_exit: PathBuf,
    /// Metadados gravados no lançamento, casados com o `.exit` na apuração.
    pub arq_pendente: PathBuf,
}

/// Metadados do lançamento, persistidos até a apuração.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Pendente {
    unidade: String,
    classe: String,
    teto_mib: u64,
    geracao: u32,
}

/// **O quê:** monta o plano (nome `schematize-<proj>-<pid>-<n>`, orçamento por tipo, arquivos).
/// **Onde:** [`corpo_do_lancamento`] e [`executar_jaulado`]. Efeito: só incrementa o contador.
pub fn planejar(od: &Path, projeto: &Path, tipo: TipoExecucao, geracao: u32) -> PlanoJaula {
    let n = UNIDADE_SEQ.fetch_add(1, Ordering::Relaxed);
    let nome = projeto.file_name().and_then(|s| s.to_str()).unwrap_or("proj");
    let unidade = nome_da_unidade(nome, &format!("{}-{n}", std::process::id()));
    PlanoJaula {
        arq_exit: od.join(format!("jaula-{unidade}.exit")),
        arq_pendente: od.join(format!("jaula-{unidade}.pendente")),
        orcamento: orcamento_para(tipo),
        unidade,
        geracao,
    }
}

/// **O quê:** `comando_jaulado` SEM `--collect` (o scope falho precisa existir para o wrapper
/// ler `Result`). **Onde:** [`corpo_claude_jaulado`]. PURA.
pub fn comando_do_lancamento(o: &Orcamento, argv: &[String], unidade: &str) -> Vec<String> {
    comando_jaulado(o, argv, unidade).into_iter().filter(|a| a != "--collect").collect()
}

/// **O quê:** corpo bash do wrapper: jaula o `claude`, grava `<codigo> <Result>` em `arq_exit`,
/// limpa o scope falho e espera Enter. Cada argumento vai entre aspas simples; só o prompt é
/// expandido (`$(cat arquivo)`). **Onde:** [`corpo_do_lancamento`]. PURA.
pub fn corpo_claude_jaulado(
    projeto: &Path,
    claude: &Path,
    modelo: &str,
    promptfile: &Path,
    plano: &PlanoJaula,
) -> String {
    let q = crate::agentrun::aspas_sh;
    let argv = vec![
        claude.to_string_lossy().into_owned(),
        "--dangerously-skip-permissions".into(),
        "--model".into(),
        modelo.to_string(),
    ];
    let jaula: Vec<String> = comando_do_lancamento(&plano.orcamento, &argv, &plano.unidade)
        .iter()
        .map(|a| q(a))
        .collect();
    let unit = q(&format!("{}.scope", plano.unidade));
    let exit = q(&plano.arq_exit.to_string_lossy());
    format!(
        "#!/usr/bin/env bash\nexport PATH=\"$HOME/.local/bin:$HOME/.cargo/bin:$HOME/.claude/local:$PATH\"\n\
         cd {proj} || exit 1\n{jaula} \"$(cat {pf})\"\ncode=$?\n\
         res=$(systemctl --user show {unit} -p Result --value 2>/dev/null)\n\
         printf '%s %s\\n' \"$code\" \"$res\" > {exit}.tmp && mv {exit}.tmp {exit}\n\
         systemctl --user reset-failed {unit} 2>/dev/null\n\
         echo\nread -rp '[overdev encerrado — Enter para fechar] '\n",
        proj = q(&projeto.to_string_lossy()),
        jaula = jaula.join(" "),
        pf = q(&promptfile.to_string_lossy()),
    )
}

/// **O quê:** `true` se o usuário optou por NÃO jaular (`SCHEMATIZE_SEM_JAULA=1`).
/// **Onde:** [`corpo_do_lancamento`], [`executar_jaulado`].
pub fn sem_jaula_por_env() -> bool {
    sem_jaula_optado(std::env::var(ENV_SEM_JAULA).ok().as_deref())
}

/// **O quê:** decide o corpo do wrapper: jaulado (systemd) -> `Ok(Some(corpo))`; sem backend e
/// usuário optou por `SCHEMATIZE_SEM_JAULA=1` -> aviso no jsonl e `Ok(None)` (corpo legado);
/// sem backend e sem opt-in -> `Err` acionável (NUNCA roda sem teto em silêncio).
/// **Onde:** `launch_prompt_in_terminal`. **Efeitos:** grava `.pendente` (ou o aviso).
pub fn corpo_do_lancamento(
    od: &Path,
    projeto: &Path,
    claude: &Path,
    modelo: &str,
    promptfile: &Path,
) -> Result<Option<String>, String> {
    corpo_com(detectar_backend(), sem_jaula_por_env(), od, projeto, claude, modelo, promptfile)
}

/// **O quê:** [`corpo_do_lancamento`] com backend e opt-in injetados (testável sem systemd).
/// **Onde:** a função acima e os testes.
pub fn corpo_com(
    backend: Backend,
    sem_jaula: bool,
    od: &Path,
    projeto: &Path,
    claude: &Path,
    modelo: &str,
    promptfile: &Path,
) -> Result<Option<String>, String> {
    let geracao = geracao_atual(&ler(od).0);
    let plano = planejar(od, projeto, TipoExecucao::AgenteLlm, geracao);
    if backend != Backend::SystemdScope {
        if !sem_jaula {
            return Err(mensagem_sem_backend(
                "não há `systemd-run --user` com o controller `memory` delegado.",
            ));
        }
        avisar_sem_jaula(od, &plano);
        return Ok(None);
    }
    let pend = Pendente {
        unidade: plano.unidade.clone(),
        classe: classe_nome(classe_do_teto(plano.orcamento.mem_max_mib)).into(),
        teto_mib: plano.orcamento.mem_max_mib,
        geracao,
    };
    let json = serde_json::to_string(&pend).map_err(|e| format!("serializar pendente: {e}"))?;
    std::fs::write(&plano.arq_pendente, json).map_err(|e| format!("gravar pendente: {e}"))?;
    Ok(Some(corpo_claude_jaulado(projeto, claude, modelo, promptfile, &plano)))
}

/// **O quê:** registra o aviso de execução SEM jaula (escolha explícita do usuário).
/// **Onde:** [`corpo_com`] e [`executar_jaulado`]. Falha de escrita vai ao stderr, nunca engolida.
fn avisar_sem_jaula(od: &Path, plano: &PlanoJaula) {
    let r = Registro {
        unidade: plano.unidade.clone(),
        classe: classe_nome(classe_do_teto(plano.orcamento.mem_max_mib)).into(),
        teto_mib: plano.orcamento.mem_max_mib,
        pico_mib: None,
        veredito: "aviso".into(),
        geracao: plano.geracao,
        timestamp: crate::nucleo::util::now_unix(),
        aviso: Some(format!(
            "{ENV_SEM_JAULA}=1: executando SEM teto de memória por escolha do usuário"
        )),
    };
    if let Err(e) = anexar(od, &r) {
        eprintln!("jaulas.jsonl: não consegui registrar o aviso: {e}");
    }
}

/// **O quê:** tag do veredito + pico para o jsonl. **Onde:** [`registro_de`].
fn tag_do_veredito(v: &Veredito) -> (&'static str, Option<u64>) {
    match v {
        Veredito::Concluiu(_) => ("concluiu", None),
        Veredito::Falhou(_) => ("falhou", None),
        Veredito::EstourouMemoria { pico_mib, .. } => ("oom", *pico_mib),
        Veredito::Timeout => ("timeout", None),
        Veredito::SemBackend(_) => ("sem_backend", None),
    }
}

/// **O quê:** monta o [`Registro`] de uma unidade terminada. **Onde:** [`executar_jaulado`] e [`apurar`]. PURA.
fn registro_de(
    unidade: &str,
    classe: &str,
    teto: u64,
    geracao: u32,
    v: &Veredito,
    ts: u64,
) -> Registro {
    let (tag, pico) = tag_do_veredito(v);
    let aviso = match v {
        Veredito::SemBackend(m) => Some(m.clone()),
        _ => None,
    };
    Registro {
        unidade: unidade.into(),
        classe: classe.into(),
        teto_mib: teto,
        pico_mib: pico,
        veredito: tag.into(),
        geracao,
        timestamp: ts,
        aviso,
    }
}

/// **O quê:** executa `argv` jaulado (não-interativo), REGISTRA o veredito e o devolve como VALOR.
/// OOM mata só esta unidade; quem chama segue. Sem backend e sem opt-in: `SemBackend` (nada rodou);
/// com `SCHEMATIZE_SEM_JAULA=1`: roda sem teto, com aviso no jsonl.
/// **Onde:** execuções avulsas do overdev (scripts/tools/testes) e o teste de integração.
pub fn executar_jaulado(
    od: &Path,
    projeto: &Path,
    o: &Orcamento,
    argv: &[String],
    geracao: u32,
) -> Veredito {
    let mut plano = planejar(od, projeto, TipoExecucao::Tool, geracao);
    plano.orcamento = o.clone();
    let classe = classe_nome(classe_do_teto(o.mem_max_mib));
    let v = if detectar_backend() == Backend::Nenhum && sem_jaula_por_env() {
        avisar_sem_jaula(od, &plano);
        rodar_sem_jaula(argv)
    } else {
        executar(o, argv, &plano.unidade)
    };
    let r = registro_de(
        &plano.unidade,
        classe,
        o.mem_max_mib,
        geracao,
        &v,
        crate::nucleo::util::now_unix(),
    );
    if let Err(e) = anexar(od, &r) {
        eprintln!("jaulas.jsonl: não consegui registrar {}: {e}", r.unidade);
    }
    v
}

/// **O quê:** roda `argv` sem jaula (só com opt-in explícito). **Onde:** [`executar_jaulado`].
fn rodar_sem_jaula(argv: &[String]) -> Veredito {
    let Some((prog, args)) = argv.split_first() else { return Veredito::Falhou(127) };
    match std::process::Command::new(prog).args(args).status() {
        Ok(s) if s.success() => Veredito::Concluiu(0),
        Ok(s) => Veredito::Falhou(s.code().unwrap_or(1)),
        Err(_) => Veredito::Falhou(127),
    }
}

/// **O quê:** `<codigo> <Result>` do `.exit` -> [`Veredito`] (SIGKILL/`oom-kill` = estouro). PURA.
/// **Onde:** [`apurar`].
pub fn veredito_do_exit(texto: &str, teto_mib: u64) -> Option<Veredito> {
    let mut it = texto.split_whitespace();
    let code: i32 = it.next()?.parse().ok()?;
    let ev = Evidencia { result_oom: it.next() == Some("oom-kill"), ..Evidencia::default() };
    Some(classificar(Some(code), None, &ev, teto_mib))
}

/// **O quê:** transforma cada `jaula-*.exit` com `.pendente` correspondente em linha do jsonl,
/// remove os dois arquivos e devolve os registros novos. Uma unidade morta por OOM é só um
/// registro: não derruba nem reinicia o supervisor nem as outras unidades.
/// **Onde:** supervisor, a cada ronda. **Efeitos:** lê/remove arquivos, anexa ao jsonl.
pub fn apurar(od: &Path) -> Vec<Registro> {
    let mut novos = Vec::new();
    for e in std::fs::read_dir(od).into_iter().flatten().flatten() {
        let p = e.path();
        let Some(nome) = p.file_name().and_then(|n| n.to_str()) else { continue };
        let Some(unidade) = nome.strip_prefix("jaula-").and_then(|n| n.strip_suffix(".exit"))
        else {
            continue;
        };
        let pend_arq = od.join(format!("jaula-{unidade}.pendente"));
        let pend: Option<Pendente> =
            std::fs::read_to_string(&pend_arq).ok().and_then(|t| serde_json::from_str(&t).ok());
        let Some(pend) = pend else { continue };
        let Some(v) =
            std::fs::read_to_string(&p).ok().and_then(|t| veredito_do_exit(&t, pend.teto_mib))
        else {
            continue;
        };
        let r = registro_de(
            &pend.unidade,
            &pend.classe,
            pend.teto_mib,
            pend.geracao,
            &v,
            crate::nucleo::util::now_unix(),
        );
        match anexar(od, &r) {
            Ok(()) => {
                for f in [&p, &pend_arq] {
                    if let Err(e) = std::fs::remove_file(f) {
                        eprintln!("jaula: não removi {}: {e}", f.display());
                    }
                }
                novos.push(r);
            }
            Err(e) => eprintln!("jaulas.jsonl: não consegui registrar {}: {e}", r.unidade),
        }
    }
    novos
}

/// **O quê:** prompt do relançamento conforme o histórico: OOM/timeout -> re-decomposição;
/// gerações esgotadas -> parkear (e grava o marcador `parkeado`, zerando a contagem); senão `None`
/// (vale a retomada comum). **Onde:** supervisor, no `Ronda::Relancar`.
pub fn prompt_de_relancamento(od: &Path) -> Option<String> {
    let (regs, _) = ler(od);
    let ultimo = regs.iter().rev().find(|r| r.veredito != "aviso")?;
    let v = match ultimo.veredito.as_str() {
        "oom" => Veredito::EstourouMemoria { pico_mib: ultimo.pico_mib, teto_mib: ultimo.teto_mib },
        "timeout" => Veredito::Timeout,
        _ => return None,
    };
    let geracao = geracao_atual(&regs);
    match reagir(&v, geracao.saturating_sub(1)) {
        ReacaoAoVeredito::Redecompor { geracao } => {
            Some(prompt_redecomposicao(geracao, &ultimo.unidade, ultimo.pico_mib, ultimo.teto_mib))
        }
        ReacaoAoVeredito::Parkear => {
            let mut marca = ultimo.clone();
            marca.veredito = "parkeado".into();
            marca.timestamp = crate::nucleo::util::now_unix();
            if let Err(e) = anexar(od, &marca) {
                eprintln!("jaulas.jsonl: não consegui registrar o parkeamento: {e}");
            }
            Some(prompt_parkear(&ultimo.unidade, ultimo.teto_mib))
        }
        _ => None,
    }
}

#[cfg(test)]
mod testes;
