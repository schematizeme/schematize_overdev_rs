//! Despacho de UMA unidade como processo: o invólucro com "portão", o resultado em arquivo e o
//! veredito.
//!
//! **O quê:** [`Despachante`] lança o processo e devolve um [`Filho`] (ou um [`FilhoAdotado`], que
//! reanexa a uma unidade que sobreviveu à morte do orquestrador). O invólucro `sh` espera um byte
//! no stdin (o PORTÃO) ANTES de executar o comando: o orquestrador só o libera depois de gravar
//! `Lancada` no journal, então nenhum efeito acontece sem pid registrado; se o orquestrador morre
//! antes, o stdin fecha e o invólucro sai sem rodar nada. Ao fim o invólucro grava o código de
//! saída em `resfile` (atômico): é o que permite reanexar sem re-executar.
//! **Onde:** `executor` (lança) e `reconcilia` (adota). Com `orcamento`, roda numa jaula
//! `systemd-run --user --scope` (sem `--collect`, para o `Result=oom-kill` sobreviver).

use super::modelo::Veredito;
use super::proc;
use crate::agentrun::jaulado::{comando_do_lancamento, veredito_do_exit};
use schematize_plataforma::jaula::{Orcamento, Veredito as VeredJaula};
use std::io::Write;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

/// Script do invólucro: `$1`=resfile, resto=comando. Portão, comando, resfile atômico com
/// `<código> <oom_kill>` (o `oom_kill` do cgroup da jaula só é lido com `OVERDEV_JAULA=1`: é a
/// evidência de OOM que sobrevive ao fim do scope) e o mesmo código de saída.
const INVOLUCRO: &str = "res=$1; shift; read -r _ || exit 3; \"$@\" </dev/null; c=$?; o=0; \
if [ \"$OVERDEV_JAULA\" = 1 ] && [ \"$c\" -ge 128 ]; then \
f=/sys/fs/cgroup$(sed -n 's/^0:://p' /proc/self/cgroup)/memory.events; \
o=$(sed -n 's/^oom_kill //p' \"$f\" 2>/dev/null); fi; \
printf '%s %s\\n' \"$c\" \"${o:-0}\" > \"$res.tmp\" && mv \"$res.tmp\" \"$res\"; exit \"$c\"";

/// O que lançar.
#[derive(Clone, Debug)]
pub struct Pedido {
    /// Id do item.
    pub id: String,
    /// Nome da unidade systemd (vazio = sem jaula).
    pub unidade: String,
    /// Onde o invólucro grava o código de saída.
    pub resfile: PathBuf,
    /// Onde vai a saída (stdout+stderr) da unidade.
    pub log: PathBuf,
    /// Comando já expandido (argv).
    pub argv: Vec<String>,
    /// Teto de memória (jaula); `None` = sem jaula.
    pub orcamento: Option<Orcamento>,
    /// Tempo máximo de parede.
    pub timeout: Option<Duration>,
    /// Variáveis de ambiente extras.
    pub env: Vec<(String, String)>,
}

/// Saída de uma unidade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Saida {
    /// Veredito.
    pub veredito: Veredito,
    /// Pico de memória visto (MiB).
    pub pico_mib: Option<u64>,
}

/// Fim observado de uma unidade.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fim {
    /// Terminou com veredito.
    Saida(Saida),
    /// Sumiu sem deixar resultado (morta no meio): vai ser re-despachada.
    Perdida(String),
}

/// Uma unidade viva (lançada por nós ou adotada).
pub trait Filho {
    /// Pid do líder. **Onde:** gravado em `Lancada`.
    fn pid(&self) -> Option<u32>;
    /// Abre o portão (o comando começa). **Onde:** logo DEPOIS de `Lancada` no journal.
    fn liberar(&mut self) -> Result<(), String>;
    /// Terminou? `None` = ainda roda. **Onde:** laço do executor.
    fn tentar_esperar(&mut self) -> Option<Fim>;
    /// Pede (SIGTERM) que a unidade encolha/pare. **Onde:** `PedirEncolher`/`PedirParar`.
    fn pedir_parar(&mut self);
    /// Mata só esta unidade. **Onde:** reconciliação e shutdown.
    fn matar(&mut self);
    /// Memória atual (MiB) e atualiza o pico. **Onde:** governador/monitor.
    fn atual_mib(&mut self) -> Option<u64>;
    /// Pico visto (MiB). **Onde:** histórico de picos.
    fn pico_mib(&self) -> Option<u64>;
}

/// Lança unidades. A fábrica de produção é [`DespachanteProcesso`].
pub trait Despachante {
    /// **O quê:** lança o processo (preso no portão). **Onde:** `executor`, depois de `Despachada`.
    fn lancar(&self, p: &Pedido) -> Result<Box<dyn Filho>, String>;
}

/// **O quê:** `sh -c INVOLUCRO sh <resfile> <argv...>`. **Onde:** [`DespachanteProcesso::lancar`]. PURA.
pub fn envolver(resfile: &Path, argv: &[String]) -> Vec<String> {
    let mut v: Vec<String> =
        ["sh", "-c", INVOLUCRO, "sh", &resfile.to_string_lossy()].map(String::from).to_vec();
    v.extend(argv.iter().cloned());
    v
}

/// **O quê:** prompt = prefixo estável + brief (o prefixo idêntico em todo despacho mantém o cache
/// de prompt quente). **Onde:** [`expandir`]. PURA.
pub fn montar_prompt(prefixo: &str, brief: &str) -> String {
    if prefixo.is_empty() {
        brief.to_string()
    } else {
        format!("{prefixo}\n\n{brief}")
    }
}

/// **O quê:** troca `{prompt}`, `{brief}`, `{prefixo}`, `{modelo}`, `{id}` em cada argumento.
/// Argv vazio = `claude -p` com o prompt. **Onde:** `executor`, ao montar o [`Pedido`]. PURA.
pub fn expandir(
    argv: &[String],
    id: &str,
    modelo: &str,
    prefixo: &str,
    brief: &str,
) -> Vec<String> {
    let prompt = montar_prompt(prefixo, brief);
    if argv.is_empty() {
        let base = [
            "claude",
            "-p",
            "--output-format",
            "json",
            "--model",
            modelo,
            "--dangerously-skip-permissions",
        ];
        return base.iter().map(|s| s.to_string()).chain([prompt]).collect();
    }
    let troca = |a: &String| {
        a.replace("{prompt}", &prompt)
            .replace("{brief}", brief)
            .replace("{prefixo}", prefixo)
            .replace("{modelo}", modelo)
            .replace("{id}", id)
    };
    argv.iter().map(troca).collect()
}

/// **O quê:** `resfile` de um despacho (`<dir>/unidades/<hash do id>-<n>.res`). **Onde:** executor.
pub fn resfile_de(dir: &Path, id: &str, despacho: u32) -> PathBuf {
    let h = crate::otimiza::fnv64(id.as_bytes());
    dir.join("unidades").join(format!("{h:016x}-{despacho}.res"))
}

/// **O quê:** lê `(código, oom_kill)` gravados pelo invólucro (`oom_kill` ausente = 0).
/// **Onde:** fim de unidade / adoção.
pub fn ler_resultado(resfile: &Path) -> Option<(i32, u64)> {
    let t = std::fs::read_to_string(resfile).ok()?;
    let mut it = t.split_whitespace();
    let code = it.next()?.parse().ok()?;
    Some((code, it.next().and_then(|o| o.parse().ok()).unwrap_or(0)))
}

/// **O quê:** só o código de saída. **Onde:** testes e diagnóstico.
pub fn ler_codigo(resfile: &Path) -> Option<i32> {
    ler_resultado(resfile).map(|r| r.0)
}

/// **O quê:** `Result` do scope systemd (`oom-kill`, `success`, ...). **Onde:** distinguir OOM.
fn resultado_scope(unidade: &str) -> String {
    let nome = format!("{unidade}.scope");
    let o = Command::new("systemctl")
        .args(["--user", "show", &nome, "-p", "Result", "--value"])
        .output();
    o.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

/// **O quê:** limpa o scope falho (best-effort). **Onde:** depois de ler o `Result`.
fn limpar_scope(unidade: &str) {
    let nome = format!("{unidade}.scope");
    if let Err(e) = Command::new("systemctl").args(["--user", "reset-failed", &nome]).output() {
        eprintln!("overdev: reset-failed {nome}: {e}");
    }
}

/// **O quê:** converte código + `Result` do scope no [`Veredito`]. **Onde:** `fim_de` e adoção.
fn veredito_de(code: i32, oom_kill: u64, unidade: &str, teto_mib: u64) -> Veredito {
    if oom_kill > 0 && !unidade.is_empty() {
        limpar_scope(unidade);
        return Veredito::EstourouMemoria;
    }
    if unidade.is_empty() {
        return if code == 0 { Veredito::Concluiu } else { Veredito::Falhou };
    }
    let texto = format!("{code} {}", resultado_scope(unidade));
    limpar_scope(unidade);
    match veredito_do_exit(&texto, teto_mib) {
        Some(VeredJaula::Concluiu(_)) => Veredito::Concluiu,
        Some(VeredJaula::EstourouMemoria { .. }) => Veredito::EstourouMemoria,
        Some(VeredJaula::Timeout) => Veredito::Timeout,
        _ => Veredito::Falhou,
    }
}

/// Lança processos locais (com ou sem jaula).
pub struct DespachanteProcesso;

/// Processo lançado por este orquestrador.
pub struct FilhoProcesso {
    child: Child,
    portao: Option<ChildStdin>,
    resfile: PathBuf,
    unidade: String,
    teto_mib: u64,
    inicio: Instant,
    timeout: Option<Duration>,
    pico: Option<u64>,
}

impl Despachante for DespachanteProcesso {
    /// **O quê:** monta o invólucro (jaulado se há orçamento), abre o log e lança com o stdin preso
    /// no portão, em grupo próprio. **Onde:** `executor`. Erro vira texto acionável.
    fn lancar(&self, p: &Pedido) -> Result<Box<dyn Filho>, String> {
        let mut args = envolver(&p.resfile, &p.argv);
        if let Some(o) = &p.orcamento {
            args = comando_do_lancamento(o, &args, &p.unidade);
        }
        let (prog, resto) = args.split_first().ok_or("comando vazio")?;
        let abre = |a: &Path| {
            std::fs::create_dir_all(a.parent().unwrap_or(Path::new(".")))
                .and_then(|()| std::fs::OpenOptions::new().create(true).append(true).open(a))
                .map_err(|e| format!("log da unidade {}: {e}", a.display()))
        };
        let (out, err) = (abre(&p.log)?, abre(&p.log)?);
        let _ = std::fs::remove_file(&p.resfile);
        let mut c = Command::new(prog);
        c.env("OVERDEV_JAULA", if p.orcamento.is_some() { "1" } else { "0" });
        c.args(resto).stdin(Stdio::piped()).stdout(out).stderr(err).process_group(0);
        c.envs(p.env.iter().map(|(k, v)| (k, v)));
        let mut child = c.spawn().map_err(|e| format!("lançar {prog}: {e}"))?;
        let portao = child.stdin.take();
        let teto_mib = p.orcamento.as_ref().map_or(0, |o| o.mem_max_mib);
        Ok(Box::new(FilhoProcesso {
            child,
            portao,
            resfile: p.resfile.clone(),
            unidade: if p.orcamento.is_some() { p.unidade.clone() } else { String::new() },
            teto_mib,
            inicio: Instant::now(),
            timeout: p.timeout,
            pico: None,
        }))
    }
}

impl FilhoProcesso {
    /// **O quê:** traduz o status de saída em [`Fim`]. **Onde:** `tentar_esperar`.
    fn fim_de(&self, status: std::process::ExitStatus) -> Fim {
        let por_sinal = status.signal().map(|s| 128 + s);
        let (rcode, oom) = ler_resultado(&self.resfile).map_or((None, 0), |(c, o)| (Some(c), o));
        let code = rcode.or(status.code()).or(por_sinal).unwrap_or(1);
        let veredito = veredito_de(code, oom, &self.unidade, self.teto_mib);
        Fim::Saida(Saida { veredito, pico_mib: self.pico })
    }
}

impl Filho for FilhoProcesso {
    /// **O quê:** pid do invólucro (líder do grupo). **Onde:** `Lancada`.
    fn pid(&self) -> Option<u32> {
        Some(self.child.id())
    }

    /// **O quê:** escreve o byte do portão e o fecha. **Onde:** depois de `Lancada` no journal.
    fn liberar(&mut self) -> Result<(), String> {
        let Some(mut p) = self.portao.take() else { return Ok(()) };
        match p.write_all(b"\n") {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
            Err(e) => Err(format!("abrir o portão da unidade: {e}")),
        }
    }

    /// **O quê:** `try_wait` + timeout de parede (mata a unidade, nunca o orquestrador).
    /// **Onde:** laço do executor.
    fn tentar_esperar(&mut self) -> Option<Fim> {
        match self.child.try_wait() {
            Ok(Some(st)) => Some(self.fim_de(st)),
            Ok(None) if self.timeout.is_some_and(|t| self.inicio.elapsed() > t) => {
                self.matar();
                let _ = self.child.wait();
                Some(Fim::Saida(Saida { veredito: Veredito::Timeout, pico_mib: self.pico }))
            }
            Ok(None) => None,
            Err(e) => Some(Fim::Perdida(format!("esperar o processo: {e}"))),
        }
    }

    /// **O quê:** SIGTERM ao grupo. **Onde:** governador.
    fn pedir_parar(&mut self) {
        proc::pedir_parar_unidade(self.child.id());
    }

    /// **O quê:** SIGKILL ao grupo (e parada do scope). **Onde:** timeout/shutdown.
    fn matar(&mut self) {
        proc::matar_unidade(self.child.id());
        if !self.unidade.is_empty() {
            if let Err(e) = proc::parar_scope(&self.unidade) {
                eprintln!("overdev: {e}");
            }
        }
    }

    /// **O quê:** memória atual (cgroup da jaula, senão RSS da árvore) e atualiza o pico.
    /// **Onde:** tick do governador.
    fn atual_mib(&mut self) -> Option<u64> {
        let m = memoria_mib(self.child.id(), !self.unidade.is_empty());
        self.pico = self.pico.max(m);
        m
    }

    /// **O quê:** pico visto. **Onde:** histórico de picos.
    fn pico_mib(&self) -> Option<u64> {
        self.pico
    }
}

/// **O quê:** memória (MiB) de uma unidade: `memory.current` do cgroup se jaulada, senão RSS do
/// pid + filhos diretos. **Onde:** `atual_mib` dos dois tipos de [`Filho`].
pub fn memoria_mib(pid: u32, jaulada: bool) -> Option<u64> {
    if jaulada {
        let rel = proc::cgroup_rel(pid)?;
        let t = std::fs::read_to_string(format!(
            "/sys/fs/cgroup/{}/memory.current",
            rel.trim_start_matches('/')
        ))
        .ok()?;
        return t.trim().parse::<u64>().ok().map(|b| b >> 20);
    }
    let filhos =
        std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children")).unwrap_or_default();
    let soma: u64 = filhos.split_whitespace().filter_map(|c| proc::rss_mib(c.parse().ok()?)).sum();
    proc::rss_mib(pid).map(|r| r + soma)
}

/// Unidade que sobreviveu à morte do orquestrador e foi reanexada (não é nossa filha).
pub struct FilhoAdotado {
    /// Pid gravado.
    pub pid: u32,
    /// `starttime` gravado.
    pub starttime: u64,
    /// Arquivo de resultado do invólucro.
    pub resfile: PathBuf,
    /// Unidade systemd ("" se sem jaula).
    pub unidade: String,
    /// Teto (para classificar OOM).
    pub teto_mib: u64,
    pico: Option<u64>,
}

impl FilhoAdotado {
    /// **O quê:** adota a unidade gravada no estado. **Onde:** `reconcilia`.
    pub fn novo(
        pid: u32,
        starttime: u64,
        resfile: PathBuf,
        unidade: String,
        teto_mib: u64,
    ) -> Self {
        FilhoAdotado { pid, starttime, resfile, unidade, teto_mib, pico: None }
    }
}

impl Filho for FilhoAdotado {
    /// **O quê:** pid adotado. **Onde:** diagnóstico.
    fn pid(&self) -> Option<u32> {
        Some(self.pid)
    }

    /// **O quê:** nada (o portão já foi aberto antes da morte). **Onde:** contrato.
    fn liberar(&mut self) -> Result<(), String> {
        Ok(())
    }

    /// **O quê:** ainda vivo (mesmo starttime) = `None`; terminou = código do `resfile`, ou
    /// OOM do scope, ou `Perdida`. **Onde:** laço do executor.
    fn tentar_esperar(&mut self) -> Option<Fim> {
        if proc::vivo_verificado(self.pid, self.starttime) {
            return None;
        }
        if let Some((code, oom)) = ler_resultado(&self.resfile) {
            let veredito = veredito_de(code, oom, &self.unidade, self.teto_mib);
            return Some(Fim::Saida(Saida { veredito, pico_mib: self.pico }));
        }
        if !self.unidade.is_empty() && resultado_scope(&self.unidade) == "oom-kill" {
            limpar_scope(&self.unidade);
            return Some(Fim::Saida(Saida {
                veredito: Veredito::EstourouMemoria,
                pico_mib: self.pico,
            }));
        }
        Some(Fim::Perdida(format!("pid {} sumiu sem resultado", self.pid)))
    }

    /// **O quê:** SIGTERM ao grupo adotado. **Onde:** governador.
    fn pedir_parar(&mut self) {
        proc::pedir_parar_unidade(self.pid);
    }

    /// **O quê:** mata só esta unidade (pid verificado). **Onde:** reconciliação.
    fn matar(&mut self) {
        if proc::vivo_verificado(self.pid, self.starttime) {
            proc::matar_unidade(self.pid);
        }
        if !self.unidade.is_empty() {
            if let Err(e) = proc::parar_scope(&self.unidade) {
                eprintln!("overdev: {e}");
            }
        }
    }

    /// **O quê:** memória atual. **Onde:** governador.
    fn atual_mib(&mut self) -> Option<u64> {
        let m = memoria_mib(self.pid, !self.unidade.is_empty());
        self.pico = self.pico.max(m);
        m
    }

    /// **O quê:** pico visto. **Onde:** histórico.
    fn pico_mib(&self) -> Option<u64> {
        self.pico
    }
}

#[cfg(test)]
mod testes;
