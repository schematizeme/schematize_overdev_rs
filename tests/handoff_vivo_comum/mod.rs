//! Utilidades compartilhadas dos testes do handoff vivo (não é um alvo de teste por si).
#![allow(dead_code)]

use overdev_casa::handoff_vivo::config::Config;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// Diretório temporário único (limpo) por teste.
pub fn tmp(nome: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hv-it-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("criar tmp");
    d
}

/// xorshift64 com seed registrada (`HV_SEED` reproduz uma falha).
pub struct Rng(pub u64);

impl Rng {
    /// Seed de `HV_SEED` ou do relógio; imprime a seed usada.
    pub fn nova(contexto: &str) -> Rng {
        let s = std::env::var("HV_SEED").ok().and_then(|v| v.parse().ok()).unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(1)
        });
        eprintln!("[{contexto}] seed={s} (reproduza com HV_SEED={s})");
        Rng(s | 1)
    }
    pub fn proximo(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    pub fn ate(&mut self, n: u64) -> u64 {
        self.proximo() % n.max(1)
    }
}

/// Workspace de teste: `<tmp>/proj/` com archive, checklist do overdev, `estado/` e um repo git.
pub fn workspace(nome: &str) -> Config {
    let raiz = tmp(nome).join("proj");
    std::fs::create_dir_all(raiz.join("proj_archive/relatorios")).expect("archive");
    std::fs::create_dir_all(raiz.join(".schematize/overdev/estado")).expect("overdev");
    std::fs::write(
        raiz.join(".schematize/overdev/CHECKLIST.md"),
        "# C\n- [x] feito um\n- [ ] aberto um\n- [~] parkeado um\n",
    )
    .expect("checklist");
    std::fs::write(raiz.join(".schematize/overdev/estado/u1.json"), "{\"unidade\":\"u1\"}\n")
        .expect("estado");
    std::fs::write(raiz.join("proj_archive/relatorios/2026-11-21-x.md"), "# Relatório X\n")
        .expect("relatorio");
    let repo = raiz.join("repo1");
    std::fs::create_dir_all(&repo).expect("repo");
    git(&repo, &["init", "-q"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "inicio"]);
    Config::para(&raiz)
}

/// `git` com identidade fixa de teste.
pub fn git(dir: &Path, args: &[&str]) {
    let s = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "teste")
        .env("GIT_AUTHOR_EMAIL", "teste@example.invalid")
        .env("GIT_COMMITTER_NAME", "teste")
        .env("GIT_COMMITTER_EMAIL", "teste@example.invalid")
        .status()
        .expect("git");
    assert!(s.success(), "git {args:?}");
}

/// Reexecuta ESTE binário de teste como filho no `modo` (a função `filho_hv` do arquivo trata).
pub fn filho(modo: &str, env: &[(&str, &str)]) -> Command {
    let mut c = Command::new(std::env::current_exe().expect("exe"));
    c.args(["--exact", "filho_hv", "--nocapture", "--test-threads=1"]).env("HV_MODO", modo);
    for (k, v) in env {
        c.env(k, v);
    }
    c
}

/// Espera o filho imprimir a linha `READY` no stdout.
pub fn esperar_pronto(filho: &mut Child) {
    use std::io::{BufRead, BufReader};
    let out = filho.stdout.as_mut().expect("stdout do filho");
    let mut r = BufReader::new(out);
    let mut l = String::new();
    loop {
        l.clear();
        let n = r.read_line(&mut l).expect("ler filho");
        assert!(n > 0, "filho terminou antes do READY");
        if l.trim().ends_with("READY") {
            return;
        }
    }
}

/// Spawn do filho com stdout em pipe.
pub fn spawn_pipe(mut c: Command) -> Child {
    c.stdout(Stdio::piped()).stderr(Stdio::null()).spawn().expect("spawn filho")
}

/// Campo `VmHWM`/`VmRSS` (kB) de `/proc/self/status`.
pub fn vm_kb(campo: &str) -> u64 {
    let s = std::fs::read_to_string("/proc/self/status").expect("status");
    s.lines()
        .find(|l| l.starts_with(campo))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}
