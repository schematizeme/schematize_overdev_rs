//! Lock exclusivo do escritor do handoff vivo (um escritor só), com PID e `starttime`.
//!
//! **O quê:** [`Trava::adquirir`] pega `flock(LOCK_EX|LOCK_NB)` em `escritor.lock` e grava
//! `"<pid> <starttime>"`. O kernel solta o flock quando o dono morre (inclusive SIGKILL), então
//! um lock cujo dono morreu é RETOMADO; o par pid+`starttime` (campo 22 de `/proc/<pid>/stat`)
//! distingue "dono morto" de "pid reaproveitado por outro processo" e vira o aviso de órfão.
//! **Onde:** [`super::ciclo::Motor::abrir`]; `verificar --reparar` e `status` lêem o dono.
//! Fora do Unix: fallback `create_new` (sem detecção de órfão).

use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Dono do lock gravado no arquivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dono {
    /// PID gravado.
    pub pid: u32,
    /// `starttime` (ticks desde o boot) gravado.
    pub starttime: u64,
}

/// Por que não adquiriu.
#[derive(Debug)]
pub enum ErroTrava {
    /// Outro escritor vivo segura o lock.
    Ocupada(Option<Dono>),
    /// Falha de E/S.
    Io(String),
}

impl std::fmt::Display for ErroTrava {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErroTrava::Ocupada(Some(d)) => write!(f, "outro escritor ativo (pid {})", d.pid),
            ErroTrava::Ocupada(None) => write!(f, "outro escritor ativo"),
            ErroTrava::Io(e) => write!(f, "lock: {e}"),
        }
    }
}

/// Lock adquirido; solta ao descartar (ou ao morrer o processo).
#[derive(Debug)]
pub struct Trava {
    _arquivo: File,
    /// Havia dono anterior morto (ou pid reaproveitado) quando adquiriu.
    pub orfao_retomado: Option<Dono>,
    caminho: PathBuf,
}

/// **O quê:** `starttime` do processo `pid` lendo `/proc/<pid>/stat`. `None` se não existe.
/// **Onde:** gravação do dono e [`dono_vivo`]. Trata `comm` com espaços/parênteses (após o último `)`).
pub fn starttime_de(pid: u32) -> Option<u64> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let resto = &s[s.rfind(')')? + 1..];
    resto.split_whitespace().nth(19)?.parse().ok()
}

/// **O quê:** o dono gravado ainda é o MESMO processo (pid vivo e `starttime` igual)? PURA-ish.
/// **Onde:** aviso de órfão, `status`.
pub fn dono_vivo(d: &Dono) -> bool {
    starttime_de(d.pid) == Some(d.starttime)
}

/// **O quê:** lê `"<pid> <starttime>"` do conteúdo do lock. **Onde:** [`Trava::adquirir`], `status`.
pub fn ler_dono(caminho: &Path) -> Option<Dono> {
    let s = std::fs::read_to_string(caminho).ok()?;
    let mut it = s.split_whitespace();
    Some(Dono { pid: it.next()?.parse().ok()?, starttime: it.next()?.parse().ok()? })
}

#[cfg(unix)]
fn flock_nb(f: &File) -> Result<bool, String> {
    use std::os::unix::io::AsRawFd;
    // SAFETY: fd válido enquanto `f` vive; flock não toca memória.
    let r = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if r == 0 {
        return Ok(true);
    }
    let e = std::io::Error::last_os_error();
    if e.kind() == std::io::ErrorKind::WouldBlock {
        Ok(false)
    } else {
        Err(format!("flock: {e}"))
    }
}

#[cfg(not(unix))]
fn flock_nb(_f: &File) -> Result<bool, String> {
    Ok(true)
}

impl Trava {
    /// **O quê:** adquire o lock de `caminho` (cria o diretório pai). Segundo escritor vivo recebe
    /// [`ErroTrava::Ocupada`]; lock de dono morto é retomado e sinalizado em `orfao_retomado`.
    /// **Onde:** abertura do motor. **Efeitos:** grava o pid/starttime próprios no arquivo.
    pub fn adquirir(caminho: &Path) -> Result<Trava, ErroTrava> {
        let io =
            |c: &str, e: std::io::Error| ErroTrava::Io(format!("{c} {}: {e}", caminho.display()));
        if let Some(p) = caminho.parent() {
            std::fs::create_dir_all(p).map_err(|e| io("criar dir de", e))?;
        }
        let anterior = ler_dono(caminho);
        let mut f = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(caminho)
            .map_err(|e| io("abrir", e))?;
        match flock_nb(&f) {
            Ok(true) => {}
            Ok(false) => return Err(ErroTrava::Ocupada(anterior)),
            Err(e) => return Err(ErroTrava::Io(e)),
        }
        let meu = std::process::id();
        let orfao = anterior.filter(|d| d.pid != meu && !dono_vivo(d));
        let conteudo = format!("{meu} {}\n", starttime_de(meu).unwrap_or(0));
        let _ = f.seek(SeekFrom::Start(0));
        f.set_len(0).map_err(|e| io("truncar", e))?;
        f.write_all(conteudo.as_bytes()).map_err(|e| io("gravar dono em", e))?;
        f.sync_all().map_err(|e| io("fsync", e))?;
        Ok(Trava { _arquivo: f, orfao_retomado: orfao, caminho: caminho.to_path_buf() })
    }

    /// **O quê:** caminho do arquivo de lock. **Onde:** mensagens e `status`.
    pub fn caminho(&self) -> &Path {
        &self.caminho
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn segundo_escritor_e_recusado_e_orfao_e_retomado() {
        let d = std::env::temp_dir().join(format!("hv-trava-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let lock = d.join("escritor.lock");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(&lock, "4000000 12345\n").unwrap();
        let t = Trava::adquirir(&lock).unwrap();
        assert_eq!(t.orfao_retomado, Some(Dono { pid: 4_000_000, starttime: 12345 }));
        match Trava::adquirir(&lock) {
            Err(ErroTrava::Ocupada(Some(d))) => assert_eq!(d.pid, std::process::id()),
            o => panic!("esperava Ocupada: {o:?}"),
        }
        drop(t);
        assert!(Trava::adquirir(&lock).is_ok());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn starttime_do_proprio_processo_existe() {
        assert!(starttime_de(std::process::id()).is_some());
    }
}
