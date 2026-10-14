//! agents — GOVERNADOR DE CONCORRÊNCIA de agents/subagents do Claude.
//!
//! O quê: calcula o número MÁXIMO de instâncias do Claude (agents + subagents + OUTRAS instâncias
//! do `claude` já rodando na MÁQUINA) que dá pra ter em paralelo SEM travar o PC. Quando o Claude
//! abre mais agents que o hardware aguenta, a máquina congela (e já corrompeu a memória do próprio
//! Claude — reinstalar). Este módulo é o cinto de segurança: mede CPU, RAM e load, desconta o que já
//! roda na máquina, e devolve um teto com "gordurinha" pro PC respirar.
//!
//! Onde: `schematize agents` (CLI) imprime o orçamento; a GUI mostra e usa no split/dispatch de
//! multiagent; o teto é persistido em `~/.schematize/agents.json` pra o Claude/overdev respeitarem.
//!
//! Regras (do dono, calibráveis por env):
//! - **CPU:** teto = max(FLOOR, threads − RESERVE). RESERVE=4 deixa o PC respirar; FLOOR=2 garante
//!   que até um PC fraco roda ≥2 (4 núcleos→2, 6→2, 8→4, 16→12).
//! - **RAM:** ~1 GB por agent; usa a RAM DISPONÍVEL − 20% de margem (12 GB livre → 9).
//! - **Load:** se a máquina já está carregada (outros processos importam), os núcleos livres AGORA
//!   (threads − load1) limitam também.
//! - **Máquina, não só eu:** conta as instâncias do `claude` de TODA a máquina (outra janela do
//!   Claude já entra na conta). Disponível-pra-lançar = teto − já-rodando.

use std::path::PathBuf;

use schematize_plataforma::seams::processos::Processos;

/// Núcleos deixados livres pro PC respirar (não viram agents). Calibrável: `SCHEMATIZE_AGENT_RESERVE`.
pub const RESERVE: usize = 4;
/// Piso: mesmo num PC fraco, ao menos isto (o dono exigiu > 1). `SCHEMATIZE_AGENT_FLOOR`.
pub const FLOOR: usize = 2;
/// RAM estimada por agent, em MB (~1 GB). `SCHEMATIZE_AGENT_GB` (em GB, aceita fracionário).
pub const MB_PER_AGENT: u64 = 1024;
/// Margem sobre a RAM disponível (20% de folga pra não lançar no talo). `SCHEMATIZE_AGENT_MARGIN`.
pub const RAM_MARGIN: f64 = 0.20;

/// Leitura crua do sistema num instante — separada da matemática pra a lógica ser testável.
#[derive(Debug, Clone, Copy)]
pub struct Snapshot {
    /// Threads lógicos (núcleos com hyperthreading).
    pub threads: usize,
    /// RAM disponível (MemAvailable), em MB.
    pub mem_available_mb: u64,
    /// Load average de 1 min.
    pub load1: f64,
    /// Instâncias do `claude` rodando na MÁQUINA agora (inclui a atual + outras janelas/subagents).
    pub running_claudes: usize,
}

/// Parâmetros efetivos (defaults + overrides de env). Lidos uma vez por cálculo.
#[derive(Debug, Clone, Copy)]
pub struct Params {
    pub reserve: usize,
    pub floor: usize,
    pub mb_per_agent: u64,
    pub ram_margin: f64,
}

impl Default for Params {
    fn default() -> Self {
        let env_usize =
            |k: &str, d: usize| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        let env_f64 =
            |k: &str, d: f64| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        let gb = env_f64("SCHEMATIZE_AGENT_GB", MB_PER_AGENT as f64 / 1024.0);
        Params {
            reserve: env_usize("SCHEMATIZE_AGENT_RESERVE", RESERVE),
            floor: env_usize("SCHEMATIZE_AGENT_FLOOR", FLOOR),
            mb_per_agent: ((gb * 1024.0).round() as u64).max(1),
            ram_margin: env_f64("SCHEMATIZE_AGENT_MARGIN", RAM_MARGIN).clamp(0.0, 0.9),
        }
    }
}

/// Orçamento calculado: os tetos por recurso, o teto final e o que sobra pra lançar.
#[derive(Debug, Clone)]
pub struct Budget {
    pub snap: Snapshot,
    pub params: Params,
    /// Teto por CPU: `max(floor, threads − reserve)`.
    pub cpu_cap: usize,
    /// Teto por RAM: `floor((mem_disponível·(1−margem)) / mb_por_agent)`, piso `floor`.
    pub ram_cap: usize,
    /// Teto por load: núcleos livres AGORA `max(floor, threads − load1)`.
    pub load_cap: usize,
    /// Teto FINAL = menor dos tetos. É o total absoluto de claudes na máquina.
    pub total_cap: usize,
    /// Disponível pra LANÇAR agora = `total_cap − running_claudes` (nunca < 0).
    pub available: usize,
    /// `true` se a RAM crua nem cobre o piso (PC muito apertado) — aí o piso pode arriscar swap.
    pub ram_tight: bool,
}

/// A MATEMÁTICA (pura). Recebe a leitura do sistema, devolve o orçamento. Testável sem tocar no SO.
pub fn compute(snap: Snapshot, p: Params) -> Budget {
    let cpu_cap = snap.threads.saturating_sub(p.reserve).max(p.floor);

    let ram_budget_mb = (snap.mem_available_mb as f64) * (1.0 - p.ram_margin);
    let ram_raw = (ram_budget_mb / p.mb_per_agent as f64).floor() as i64;
    let ram_tight = ram_raw < p.floor as i64;
    let ram_cap = (ram_raw.max(0) as usize).max(p.floor);

    // Núcleos livres AGORA (a máquina já carregada por outros processos dedica menos).
    let free_now = (snap.threads as f64 - snap.load1).floor();
    let load_cap = (free_now.max(0.0) as usize).max(p.floor);

    let total_cap = cpu_cap.min(ram_cap).min(load_cap);
    let available = total_cap.saturating_sub(snap.running_claudes);

    Budget { snap, params: p, cpu_cap, ram_cap, load_cap, total_cap, available, ram_tight }
}

impl Budget {
    /// Plano de SPLIT: dividindo o trabalho em `k` claudes principais, quantos subagents cada um pode
    /// abrir pra o TOTAL (principais + subagents) não passar do teto. `subagents_por = (cap − k)/k`.
    /// Ex.: cap=12, k=2 → 5 cada (2+10=12); k=4 → 2 cada (4+8=12).
    pub fn split_plan(&self, k: usize) -> SplitPlan {
        let k = k.max(1);
        let subagents_each = self.total_cap.saturating_sub(k) / k;
        let total_used = k + subagents_each * k;
        SplitPlan { mains: k, subagents_each, total_used, cap: self.total_cap }
    }
}

/// Resultado de um split: `mains` claudes principais, cada um com `subagents_each` subagents.
#[derive(Debug, Clone, Copy)]
pub struct SplitPlan {
    pub mains: usize,
    pub subagents_each: usize,
    pub total_used: usize,
    pub cap: usize,
}

// ---------------------------------------------------------------------------
// Leitura do sistema (Linux-first; fallback seguro em outros SOs).
// ---------------------------------------------------------------------------

/// Tira uma leitura do sistema agora (pelo seam `Processos` da plataforma instalada).
pub fn snapshot() -> Snapshot {
    snapshot_com(&*crate::ambiente::plat().processos)
}

/// **O quê:** a leitura do sistema sobre um `Processos` dado (a plataforma real ou um falso).
///
/// **Onde:** [`snapshot`] e os testes. O que a plataforma não sabe dizer (`None`) não limita:
/// RAM desconhecida = folga total (`u64::MAX / 2`), load desconhecido = `0.0`.
pub fn snapshot_com(p: &dyn Processos) -> Snapshot {
    Snapshot {
        threads: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
        mem_available_mb: p.memoria_disponivel_mb().unwrap_or(u64::MAX / 2),
        load1: p.carga1().unwrap_or(0.0),
        running_claudes: p.contar("claude"),
    }
}

/// Orçamento pronto (snapshot + compute com params de env).
pub fn budget() -> Budget {
    compute(snapshot(), Params::default())
}

/// Conta instâncias do `claude` (Claude Code) rodando na MÁQUINA, pelo seam `Processos`.
/// Heurística (na lib da plataforma): o executável é `claude`, OU um runtime (`node`/`bun`) cujo
/// cmdline aponta pro CLI do Claude. NÃO conta o próprio `schematize`. Cada PID = 1.
pub fn count_claude_processes() -> usize {
    crate::ambiente::plat().processos.contar("claude")
}

// ---------------------------------------------------------------------------
// Persistência: ~/.schematize/agents.json — pra outros (Claude/overdev/GUI) lerem o teto.
// ---------------------------------------------------------------------------

/// Caminho do estado do orçamento (global do usuário, não por-projeto).
pub fn state_path() -> PathBuf {
    crate::nucleo::util::home_app_dir().join("agents.json")
}

/// Persiste o orçamento atual em `~/.schematize/agents.json` (best-effort). Retorna o caminho.
pub fn persist(b: &Budget) -> std::io::Result<PathBuf> {
    let path = state_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::json!({
        "total_cap": b.total_cap,
        "available": b.available,
        "cpu_cap": b.cpu_cap,
        "ram_cap": b.ram_cap,
        "load_cap": b.load_cap,
        "threads": b.snap.threads,
        "mem_available_mb": b.snap.mem_available_mb,
        "load1": b.snap.load1,
        "running_claudes": b.snap.running_claudes,
        "reserve": b.params.reserve,
        "floor": b.params.floor,
        "ram_tight": b.ram_tight,
    });
    std::fs::write(&path, serde_json::to_string_pretty(&json).unwrap_or_default())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::processos_falso::Falso;

    fn snap(threads: usize, mem_mb: u64, load1: f64, claudes: usize) -> Snapshot {
        Snapshot { threads, mem_available_mb: mem_mb, load1, running_claudes: claudes }
    }
    fn p() -> Params {
        Params {
            reserve: RESERVE,
            floor: FLOOR,
            mb_per_agent: MB_PER_AGENT,
            ram_margin: RAM_MARGIN,
        }
    }

    /// A tabela do dono: 4c→2, 6c→2, 8c→4, 16t→12 (RAM/load folgados).
    #[test]
    fn cpu_cap_segue_a_tabela() {
        let far_mem = 128 * 1024; // RAM sobrando → não limita
        for (threads, want) in [(4, 2), (6, 2), (8, 4), (16, 12), (2, 2), (1, 2)] {
            let b = compute(snap(threads, far_mem, 0.0, 0), p());
            assert_eq!(b.cpu_cap, want, "threads={threads}");
            assert_eq!(
                b.total_cap, want,
                "total com RAM/load folgados = cpu_cap (threads={threads})"
            );
        }
    }

    /// RAM manda quando é o recurso mais apertado: 12 GB livres → −20% = 9.6 → 9.
    #[test]
    fn ram_cap_com_margem_de_20pct() {
        // 16 threads (cpu_cap=12), 12 GB disponível → ram_cap=9 → total=9.
        let b = compute(snap(16, 12 * 1024, 0.0, 0), p());
        assert_eq!(b.ram_cap, 9);
        assert_eq!(b.total_cap, 9, "RAM é o gargalo");
    }

    /// Máquina carregada dedica menos: 16 threads, load 10 → livres 6.
    #[test]
    fn load_limita_quando_maquina_ocupada() {
        let b = compute(snap(16, 128 * 1024, 10.0, 0), p());
        assert_eq!(b.load_cap, 6);
        assert_eq!(b.total_cap, 6);
    }

    /// Outras instâncias do claude descontam do disponível (não do teto).
    #[test]
    fn running_claudes_descontam_do_disponivel() {
        let b = compute(snap(16, 128 * 1024, 0.0, 3), p());
        assert_eq!(b.total_cap, 12);
        assert_eq!(b.available, 9, "12 teto − 3 já rodando");
        // nunca negativo.
        let b2 = compute(snap(16, 128 * 1024, 0.0, 99), p());
        assert_eq!(b2.available, 0);
    }

    /// Split divide o teto: cap=12 → k=2 dá 5 subagents cada; k=4 dá 2 cada.
    #[test]
    fn split_plan_distribui_o_teto() {
        let b = compute(snap(16, 128 * 1024, 0.0, 0), p()); // cap=12
        let s2 = b.split_plan(2);
        assert_eq!((s2.mains, s2.subagents_each, s2.total_used), (2, 5, 12));
        let s4 = b.split_plan(4);
        assert_eq!((s4.mains, s4.subagents_each, s4.total_used), (4, 2, 12));
    }

    /// O snapshot usa o que a plataforma responde e conta os `claude` pelo nome certo.
    #[test]
    fn snapshot_vem_do_seam_processos() {
        let s = snapshot_com(&Falso {
            mem: Some(8192),
            carga: Some(1.5),
            claudes: 3,
            ..Falso::default()
        });
        assert_eq!((s.mem_available_mb, s.load1, s.running_claudes), (8192, 1.5, 3));
    }

    /// Desconhecido (`None`) não limita: RAM folgada e load zero — nunca um número inventado baixo.
    #[test]
    fn plataforma_que_nao_sabe_nao_limita() {
        let s = snapshot_com(&Falso::default());
        assert_eq!(s.mem_available_mb, u64::MAX / 2);
        assert_eq!(s.load1, 0.0);
    }
}
