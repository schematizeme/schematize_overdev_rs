//! `Processos` falso para os testes: respostas fixas e registro do que foi perguntado.
//!
//! **O quê:** [`Falso`] implementa o seam `Processos` sem tocar `/proc`, `ps` nem o sistema.
//!
//! **Onde:** testes de `agents`, `overdev::supervisor` e `overdev::trava` (ADR-0026 F4b).

use std::path::Path;
use std::sync::Mutex;

use schematize_plataforma::seams::processos::{Modo, Processos};

/// Respostas programadas de um `Processos` de teste.
#[derive(Default)]
pub struct Falso {
    /// Pids que "existem".
    pub vivos: Vec<u32>,
    /// Quantos processos `contar("claude")` devolve.
    pub claudes: usize,
    /// `em_dir` devolve isto.
    pub em_dir: bool,
    /// `pids_por_argv_em` devolve isto.
    pub argv_em: Vec<u32>,
    /// RAM livre informada.
    pub mem: Option<u64>,
    /// Load de 1 min informado.
    pub carga: Option<f64>,
    /// Perguntas recebidas, em texto (`em_dir:claude:Prefixo`, `argv_em:overdev+supervise`...).
    pub perguntas: Mutex<Vec<String>>,
}

impl Falso {
    /// **O quê:** anota uma pergunta. **Onde:** cada método do trait.
    fn anota(&self, p: String) {
        if let Ok(mut v) = self.perguntas.lock() {
            v.push(p);
        }
    }

    /// **O quê:** as perguntas feitas até agora. **Onde:** as asserções dos testes.
    pub fn feitas(&self) -> Vec<String> {
        self.perguntas.lock().map(|v| v.clone()).unwrap_or_default()
    }
}

impl Processos for Falso {
    /// **O quê:** `pid` está na lista `vivos`. **Onde:** teste da trava.
    fn vivo(&self, pid: u32) -> bool {
        self.anota(format!("vivo:{pid}"));
        self.vivos.contains(&pid)
    }
    /// **O quê:** `claudes` para `claude`, 0 para o resto. **Onde:** teste do snapshot.
    fn contar(&self, nome: &str) -> usize {
        self.anota(format!("contar:{nome}"));
        if nome == "claude" {
            self.claudes
        } else {
            0
        }
    }
    /// **O quê:** o campo `em_dir`. **Onde:** teste do supervisor.
    fn em_dir(&self, nome: &str, _dir: &Path, modo: Modo) -> bool {
        self.anota(format!("em_dir:{nome}:{modo:?}"));
        self.em_dir
    }
    /// **O quê:** vazio. **Onde:** não usado pelos testes.
    fn pids_por_argv(&self, trechos: &[&str]) -> Vec<u32> {
        self.anota(format!("argv:{}", trechos.join("+")));
        Vec::new()
    }
    /// **O quê:** o campo `argv_em`. **Onde:** teste do supervisor.
    fn pids_por_argv_em(&self, trechos: &[&str], _dir: &Path) -> Vec<u32> {
        self.anota(format!("argv_em:{}", trechos.join("+")));
        self.argv_em.clone()
    }
    /// **O quê:** o campo `mem`. **Onde:** teste do snapshot.
    fn memoria_disponivel_mb(&self) -> Option<u64> {
        self.mem
    }
    /// **O quê:** o campo `carga`. **Onde:** teste do snapshot.
    fn carga1(&self) -> Option<f64> {
        self.carga
    }
}
