//! [`Duravel`]: a máquina de estados do overdev com política de snapshot e quarentena de estado
//! corrompido.
//!
//! **O quê:** envolve `Maquina<EstadoOverdev>` (journal WAL com fsync a cada evento + snapshot
//! atômico). `registrar` é o ÚNICO caminho de transição: grava no journal ANTES de aplicar e de
//! qualquer efeito (write-ahead). Snapshot a cada N eventos ou M segundos.
//! **Onde:** executor, reconciliação e supervisor. Estado ilegível (bit-flip, truncado, esquema)
//! nunca derruba o processo: cai no `.prev`/replay; sem saída, o diretório vai para
//! `corrompido-<ts>/` e o run recomeça, com o motivo no relatório.

use super::modelo::{EstadoOverdev, Evento};
use schematize_plataforma::persistencia::{Config, Durabilidade, Erro, Maquina, Relatorio};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Política de snapshot.
#[derive(Clone, Copy, Debug)]
pub struct PoliticaSnapshot {
    /// Snapshot a cada N eventos registrados.
    pub cada_eventos: u64,
    /// ... ou quando passaram M segundos desde o último.
    pub cada_segundos: u64,
}

impl Default for PoliticaSnapshot {
    /// **O quê:** 50 eventos / 30 s. **Onde:** `Duravel::abrir` do uso normal.
    fn default() -> Self {
        PoliticaSnapshot { cada_eventos: 50, cada_segundos: 30 }
    }
}

/// O que a abertura encontrou (para logar e alertar).
#[derive(Clone, Debug, Default)]
pub struct Abertura {
    /// Relatório do journal (cauda torn truncada, snapshot ruim -> .prev...).
    pub relatorio: Relatorio,
    /// Estado havia sido recuperado de um run anterior (journal/snapshot não vazios).
    pub retomando: bool,
    /// Diretório de quarentena se o estado era irrecuperável.
    pub quarentena: Option<PathBuf>,
    /// Motivo da quarentena.
    pub motivo_quarentena: Option<String>,
}

/// Estado durável com política de snapshot.
pub struct Duravel {
    maq: Maquina<EstadoOverdev>,
    dir: PathBuf,
    pol: PoliticaSnapshot,
    desde_snapshot: u64,
    ultimo_snapshot: Instant,
    /// Avisos de snapshot que falharam (o evento já estava no journal: nada se perdeu).
    pub avisos: Vec<String>,
    /// O que a abertura encontrou.
    pub abertura: Abertura,
}

/// **O quê:** `true` para erros que significam "estado corrompido" (vale a quarentena); I/O e lock
/// ocupado sobem como estão. **Onde:** [`Duravel::abrir`].
fn corrompido(e: &Erro) -> bool {
    matches!(
        e,
        Erro::BuracoDeSeq { .. }
            | Erro::CorrupcaoNoMeio { .. }
            | Erro::SnapshotInvalido(_)
            | Erro::EsquemaIncompativel { .. }
            | Erro::EventoInvalido { .. }
    )
}

/// **O quê:** move os arquivos de estado (segmentos, snapshots) para `corrompido-<ts>/`, deixando
/// `journal.lock`, o vigia e quarentenas anteriores. **Onde:** estado irrecuperável.
fn por_em_quarentena(dir: &Path) -> Result<PathBuf, Erro> {
    let ts = crate::nucleo::util::now_unix();
    let alvo = dir.join(format!("corrompido-{ts}-{}", std::process::id()));
    let io = |c: &str| {
        let c = c.to_string();
        move |fonte| Erro::Io { contexto: c, fonte }
    };
    std::fs::create_dir_all(&alvo).map_err(io("criar quarentena"))?;
    for e in std::fs::read_dir(dir).map_err(io("listar estado"))?.flatten() {
        let nome = e.file_name().to_string_lossy().into_owned();
        let dado = nome.starts_with("seg-") || nome.starts_with("snapshot.");
        if dado && e.path().is_file() {
            std::fs::rename(e.path(), alvo.join(&nome)).map_err(io("mover para quarentena"))?;
        }
    }
    Ok(alvo)
}

/// **O quê:** config do journal do overdev: `fsync` a cada evento. **Onde:** `abrir`.
fn config_journal() -> Config {
    Config { durabilidade: Durabilidade::Cada, ..Config::default() }
}

impl Duravel {
    /// **O quê:** abre `dir` (cria se faltar), recupera journal+snapshot e devolve o estado.
    /// **Onde:** boot de `executar`/`retomar` e do supervisor. `LockOcupado` (outro overdev vivo
    /// escrevendo) sobe como erro acionável; corrupção vira quarentena + recomeço.
    pub fn abrir(dir: &Path, pol: PoliticaSnapshot) -> Result<Duravel, Erro> {
        let mut abertura = Abertura::default();
        let (maq, rel) = match Maquina::<EstadoOverdev>::abrir(dir, config_journal()) {
            Ok(r) => r,
            Err(e) if corrompido(&e) => {
                abertura.motivo_quarentena = Some(e.to_string());
                abertura.quarentena = Some(por_em_quarentena(dir)?);
                Maquina::<EstadoOverdev>::abrir(dir, config_journal())?
            }
            Err(e) => return Err(e),
        };
        abertura.retomando = rel.ultimo_seq.is_some() || maq.seq() > 0;
        abertura.relatorio = rel;
        Ok(Duravel {
            maq,
            dir: dir.to_path_buf(),
            pol,
            desde_snapshot: 0,
            ultimo_snapshot: Instant::now(),
            avisos: Vec::new(),
            abertura,
        })
    }

    /// **O quê:** estado atual (leitura). **Onde:** decisões do executor.
    pub fn estado(&self) -> &EstadoOverdev {
        self.maq.estado()
    }

    /// **O quê:** seq do último evento. **Onde:** diagnóstico e testes.
    pub fn seq(&self) -> u64 {
        self.maq.seq()
    }

    /// **O quê:** diretório de estado. **Onde:** arquivos auxiliares (`resfile`, picos).
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// **O quê:** registra o evento (journal + fsync) e só então o aplica; depois decide snapshot.
    /// **Onde:** TODA transição do supervisor/executor, ANTES do efeito. Erro de journal sobe
    /// (o chamador não executa o efeito); erro de snapshot vira aviso (nada se perdeu).
    pub fn registrar(&mut self, ev: &Evento) -> Result<u64, Erro> {
        let seq = self.maq.registrar(ev)?;
        self.desde_snapshot += 1;
        let vencido = self.ultimo_snapshot.elapsed() >= Duration::from_secs(self.pol.cada_segundos);
        if self.desde_snapshot >= self.pol.cada_eventos.max(1) || vencido {
            self.snapshot();
        }
        Ok(seq)
    }

    /// **O quê:** grava snapshot agora; falha vira aviso (o journal segue íntegro).
    /// **Onde:** política de `registrar` e shutdown.
    pub fn snapshot(&mut self) {
        match self.maq.snapshot() {
            Ok(()) => {
                self.desde_snapshot = 0;
                self.ultimo_snapshot = Instant::now();
            }
            Err(e) => {
                let aviso = format!("snapshot falhou (journal íntegro, tento de novo): {e}");
                eprintln!("overdev: {aviso}");
                self.avisos.push(aviso);
            }
        }
    }
}
