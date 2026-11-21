//! Execução de subprocesso com timeout, saída limitada e grupo de processos próprio.
//!
//! **O quê:** [`rodar`] spawna `argv` (stdin opcional), drena stdout (até um teto) e stderr (cauda)
//! em threads, espera com `try_wait` e, no timeout, mata o GRUPO inteiro. Opcionalmente aplica
//! `RLIMIT_AS` ao filho (rede de segurança para stubs/ferramentas fora da jaula do systemd).
//! **Onde:** [`super::coleta`] (`git`) e [`super::claude`] (`claude -p`). Nunca bloqueia além do timeout.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Resultado de uma execução.
#[derive(Debug, Default)]
pub struct Saida {
    /// Código de saída (`None` se morto por sinal).
    pub codigo: Option<i32>,
    /// Sinal que matou o filho, se houve.
    pub sinal: Option<i32>,
    /// stdout capturado (até o teto).
    pub stdout: Vec<u8>,
    /// O stdout passou do teto (o excedente foi descartado).
    pub truncada: bool,
    /// Foi morto por passar do tempo.
    pub timeout: bool,
    /// Últimos bytes do stderr (para o motivo).
    pub stderr_cauda: String,
}

/// Parâmetros de [`rodar`].
#[derive(Debug, Clone)]
pub struct Pedido<'a> {
    /// Programa + argumentos.
    pub argv: &'a [String],
    /// Variáveis extras de ambiente.
    pub env: &'a [(String, String)],
    /// Diretório de trabalho.
    pub cwd: &'a Path,
    /// Bytes para o stdin (`None` = stdin nulo).
    pub stdin: Option<&'a [u8]>,
    /// Tempo máximo de parede.
    pub timeout: Duration,
    /// Teto de bytes de stdout guardados.
    pub teto_stdout: usize,
    /// `RLIMIT_AS` do filho em MiB (`None` = sem).
    pub limite_as_mib: Option<u64>,
}

/// **O quê:** lê `r` até EOF guardando no máximo `teto` bytes; devolve `(bytes, passou)`.
/// **Onde:** threads de captura de [`rodar`]. Drena o excedente para o filho nunca bloquear.
fn drenar<R: Read>(mut r: R, teto: usize) -> (Vec<u8>, bool) {
    let (mut v, mut passou, mut buf) = (Vec::new(), false, [0u8; 8192]);
    loop {
        match r.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let livre = teto.saturating_sub(v.len());
                v.extend_from_slice(&buf[..n.min(livre)]);
                passou |= n > livre;
            }
        }
    }
    (v, passou)
}

#[cfg(unix)]
fn preparar(cmd: &mut Command, limite_as_mib: Option<u64>) {
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);
    if let Some(mib) = limite_as_mib {
        let lim = libc::rlimit { rlim_cur: mib << 20, rlim_max: mib << 20 };
        // SAFETY: só chama setrlimit (async-signal-safe) entre fork e exec.
        unsafe {
            cmd.pre_exec(move || {
                if libc::setrlimit(libc::RLIMIT_AS, &lim) == 0 {
                    Ok(())
                } else {
                    Err(std::io::Error::last_os_error())
                }
            });
        }
    }
}

#[cfg(not(unix))]
fn preparar(_cmd: &mut Command, _limite_as_mib: Option<u64>) {}

/// **O quê:** mata o grupo de processos de `pid` (e o próprio filho). **Onde:** timeout de [`rodar`].
#[cfg(unix)]
fn matar_grupo(filho: &mut std::process::Child) {
    // SAFETY: kill(2) com pid de um filho nosso (grupo próprio, `process_group(0)`).
    unsafe {
        libc::kill(-(filho.id() as i32), libc::SIGKILL);
    }
    let _ = filho.kill();
}

#[cfg(not(unix))]
fn matar_grupo(filho: &mut std::process::Child) {
    let _ = filho.kill();
}

/// **O quê:** sinal que matou o filho. **Onde:** [`rodar`].
#[cfg(unix)]
fn sinal_de(st: &std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    st.signal()
}

#[cfg(not(unix))]
fn sinal_de(_st: &std::process::ExitStatus) -> Option<i32> {
    None
}

/// **O quê:** spawna e acompanha o processo do [`Pedido`]. `Err` só se não conseguiu spawnar.
/// **Onde:** `git` da coleta e `claude -p` da camada Claude. **Efeitos:** processo filho.
pub fn rodar(p: &Pedido<'_>) -> Result<Saida, String> {
    let (prog, args) = p.argv.split_first().ok_or("argv vazio")?;
    let mut cmd = Command::new(prog);
    cmd.args(args).current_dir(p.cwd).stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.stdin(if p.stdin.is_some() { Stdio::piped() } else { Stdio::null() });
    for (k, v) in p.env {
        cmd.env(k, v);
    }
    preparar(&mut cmd, p.limite_as_mib);
    let mut filho = cmd.spawn().map_err(|e| format!("spawn {prog}: {e}"))?;
    let entrada = p.stdin.map(<[u8]>::to_vec).zip(filho.stdin.take());
    let h_in = entrada.map(|(b, mut si)| std::thread::spawn(move || si.write_all(&b)));
    let out = filho.stdout.take().map(|o| {
        let t = p.teto_stdout;
        std::thread::spawn(move || drenar(o, t))
    });
    let err = filho.stderr.take().map(|e| std::thread::spawn(move || drenar(e, 2048)));
    let (status, timeout) = esperar(&mut filho, p.timeout)?;
    if let Some(h) = h_in {
        let _ = h.join(); // EPIPE se o filho não leu o stdin: não é erro da execução.
    }
    let (stdout, truncada) = out.and_then(|h| h.join().ok()).unwrap_or_default();
    let (e, _) = err.and_then(|h| h.join().ok()).unwrap_or_default();
    Ok(Saida {
        codigo: status.code(),
        sinal: sinal_de(&status),
        stdout,
        truncada,
        timeout,
        stderr_cauda: String::from_utf8_lossy(&e).trim().to_string(),
    })
}

/// **O quê:** espera o filho até `limite`; passou -> mata o grupo. Devolve `(status, timeout)`.
/// **Onde:** [`rodar`].
fn esperar(
    filho: &mut std::process::Child,
    limite: Duration,
) -> Result<(std::process::ExitStatus, bool), String> {
    let fim = Instant::now() + limite;
    loop {
        match filho.try_wait() {
            Ok(Some(st)) => return Ok((st, false)),
            Ok(None) if Instant::now() >= fim => {
                matar_grupo(filho);
                let st = filho.wait().map_err(|e| format!("wait após kill: {e}"))?;
                return Ok((st, true));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(15)),
            Err(e) => return Err(format!("try_wait: {e}")),
        }
    }
}

#[cfg(all(test, unix))]
mod testes {
    use super::*;

    fn pedido<'a>(argv: &'a [String], t: u64) -> Pedido<'a> {
        Pedido {
            argv,
            env: &[],
            cwd: Path::new("/"),
            stdin: Some(b"entrada"),
            timeout: Duration::from_secs(t),
            teto_stdout: 16,
            limite_as_mib: None,
        }
    }

    #[test]
    fn captura_com_teto_e_stdin() {
        let a: Vec<String> =
            ["sh", "-c", "cat; echo; yes | head -c 5000"].map(String::from).to_vec();
        let s = rodar(&pedido(&a, 5)).unwrap();
        assert_eq!(s.codigo, Some(0));
        assert!(s.truncada && s.stdout.len() == 16 && s.stdout.starts_with(b"entrada"));
    }

    #[test]
    fn timeout_mata_o_grupo() {
        let a: Vec<String> = ["sh", "-c", "sleep 30 & sleep 30"].map(String::from).to_vec();
        let t0 = Instant::now();
        let s = rodar(&pedido(&a, 1)).unwrap();
        assert!(s.timeout && t0.elapsed() < Duration::from_secs(5));
    }
}
