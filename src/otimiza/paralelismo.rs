//! Backpressure por RAM e paralelismo adaptativo da onda (IA-10).
//!
//! **O quê:** decide se cabe lançar mais uma unidade (RAM/cgroup livre > folga + teto da unidade)
//! e ajusta o paralelismo (cai pela metade sob pressão ou OOM recente, sobe 1 por vez ao
//! recuperar). Leitores puros de `/proc/meminfo` e `memory.max/current` do cgroup v2.
//! **Onde:** scheduler de ondas, antes de cada lançamento. A JAULA por unidade (cgroup com
//! `MemoryMax`, OOM só no filho) já existe em `agrupar`/`agentrun::jaulado` e o governador de
//! admissão do supervisor está em `resiliencia`; este módulo é a política pura que o scheduler
//! de ondas usa, sem duplicar nem tocar o supervisor.

/// Medida de memória (MiB).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Medida {
    /// RAM disponível do sistema (`MemAvailable`).
    pub ram_livre_mib: u64,
    /// Folga do cgroup atual (`memory.max - memory.current`), se limitado.
    pub cgroup_livre_mib: Option<u64>,
    /// Houve OOM de unidade recentemente?
    pub oom_recente: bool,
}

/// Política do scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Politica {
    /// Folga mínima que sempre sobra para o orquestrador/SO.
    pub folga_mib: u64,
    /// Teto estimado de cada unidade (da jaula).
    pub teto_unidade_mib: u64,
    /// Paralelismo mínimo (>= 1).
    pub min: u32,
    /// Paralelismo máximo.
    pub max: u32,
}

/// O menor entre RAM do sistema e folga do cgroup. **Onde:** [`pode_lancar`], [`ajustar`].
pub fn livre_efetivo(m: &Medida) -> u64 {
    m.cgroup_livre_mib.map_or(m.ram_livre_mib, |c| c.min(m.ram_livre_mib))
}

/// Cabe lançar mais uma unidade agora?
///
/// **O quê:** `livre >= folga + teto_unidade` e sem OOM recente. `em_voo == 0` com RAM aquém da
/// reserva ainda recusa (a espera é do chamador, que deve ter anti-deadlock próprio).
/// **Onde:** scheduler, antes de cada `Lancador::lancar`.
pub fn pode_lancar(m: &Medida, p: &Politica) -> bool {
    !m.oom_recente && livre_efetivo(m) >= p.folga_mib.saturating_add(p.teto_unidade_mib)
}

/// Novo paralelismo (AIMD): OOM recente ou RAM abaixo da folga = metade (>= `min`); com RAM
/// para mais de uma unidade, sobe 1 (<= `max`); no meio-termo, mantém.
///
/// **Onde:** scheduler, a cada ciclo de medição. `min` 0 é tratado como 1 (nunca zera a onda).
pub fn ajustar(atual: u32, m: &Medida, p: &Politica) -> u32 {
    let min = p.min.max(1);
    let max = p.max.max(min);
    let livre = livre_efetivo(m);
    if m.oom_recente || livre < p.folga_mib {
        return (atual / 2).clamp(min, max);
    }
    if livre >= p.folga_mib.saturating_add(p.teto_unidade_mib.saturating_mul(2)) {
        return atual.saturating_add(1).clamp(min, max);
    }
    atual.clamp(min, max)
}

/// `MemAvailable` (kB) de um texto estilo `/proc/meminfo`, em MiB.
/// **O quê:** `None` se a linha falta ou está malformada. **Onde:** [`medir_ram`].
pub fn parse_meminfo(txt: &str) -> Option<u64> {
    let l = txt.lines().find(|l| l.starts_with("MemAvailable:"))?;
    let kb: u64 = l.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024)
}

/// Folga do cgroup a partir do conteúdo de `memory.max` e `memory.current` (bytes).
/// **O quê:** `max` = ilimitado = `None`; `current > max` = 0. **Onde:** [`medir_cgroup`].
pub fn parse_cgroup(max: &str, atual: &str) -> Option<u64> {
    let m = max.trim();
    if m == "max" {
        return None;
    }
    let (m, a): (u64, u64) = (m.parse().ok()?, atual.trim().parse().ok()?);
    Some(m.saturating_sub(a) / (1024 * 1024))
}

/// Lê a RAM disponível do sistema. **Onde:** scheduler. Erro de leitura é devolvido (não vira 0).
pub fn medir_ram() -> Result<u64, String> {
    let t =
        std::fs::read_to_string("/proc/meminfo").map_err(|e| format!("ler /proc/meminfo: {e}"))?;
    parse_meminfo(&t).ok_or_else(|| "MemAvailable ausente em /proc/meminfo".to_string())
}

/// Folga do cgroup do próprio processo (`/proc/self/cgroup` + cgroup v2). `Ok(None)` se não há
/// cgroup v2 limitado. **Onde:** scheduler.
pub fn medir_cgroup() -> Result<Option<u64>, String> {
    let Ok(c) = std::fs::read_to_string("/proc/self/cgroup") else { return Ok(None) };
    let Some(rel) = c.lines().find_map(|l| l.strip_prefix("0::")) else { return Ok(None) };
    let dir = std::path::Path::new("/sys/fs/cgroup").join(rel.trim_start_matches('/'));
    let (Ok(max), Ok(cur)) = (
        std::fs::read_to_string(dir.join("memory.max")),
        std::fs::read_to_string(dir.join("memory.current")),
    ) else {
        return Ok(None);
    };
    Ok(parse_cgroup(&max, &cur))
}

/// Mede RAM + cgroup juntos. **Onde:** scheduler; `oom_recente` vem do supervisor.
pub fn medir(oom_recente: bool) -> Result<Medida, String> {
    Ok(Medida { ram_livre_mib: medir_ram()?, cgroup_livre_mib: medir_cgroup()?, oom_recente })
}

#[cfg(test)]
mod testes {
    use super::*;

    const P: Politica = Politica { folga_mib: 1024, teto_unidade_mib: 1024, min: 1, max: 6 };

    fn m(ram: u64) -> Medida {
        Medida { ram_livre_mib: ram, cgroup_livre_mib: None, oom_recente: false }
    }

    #[test]
    fn so_lanca_com_folga_e_cgroup_manda() {
        assert!(pode_lancar(&m(2048), &P));
        assert!(!pode_lancar(&m(2047), &P));
        let cg = Medida { cgroup_livre_mib: Some(500), ..m(99_999) };
        assert!(!pode_lancar(&cg, &P));
        assert!(!pode_lancar(&Medida { oom_recente: true, ..m(99_999) }, &P));
    }

    #[test]
    fn paralelismo_cai_sob_pressao_e_recupera() {
        let mut n = 6;
        n = ajustar(n, &m(500), &P);
        assert_eq!(n, 3);
        n = ajustar(n, &Medida { oom_recente: true, ..m(9999) }, &P);
        assert_eq!(n, 1);
        n = ajustar(n, &m(500), &P);
        assert_eq!(n, 1, "nunca abaixo do minimo");
        for _ in 0..10 {
            n = ajustar(n, &m(10_000), &P);
        }
        assert_eq!(n, 6, "recupera ate o maximo");
        assert_eq!(ajustar(3, &m(2500), &P), 3, "meio-termo mantem");
        let z = Politica { min: 0, max: 0, ..P };
        assert_eq!(ajustar(0, &m(0), &z), 1);
    }

    #[test]
    fn leitores_aceitam_lixo_sem_derrubar() {
        assert_eq!(parse_meminfo("MemTotal: 1 kB\nMemAvailable:   2097152 kB\n"), Some(2048));
        for ruim in ["", "MemAvailable: x kB", "MemAvailable:"] {
            assert_eq!(parse_meminfo(ruim), None, "{ruim}");
        }
        assert_eq!(parse_cgroup("max\n", "5"), None);
        assert_eq!(parse_cgroup("2147483648\n", "1073741824\n"), Some(1024));
        assert_eq!(parse_cgroup("10", "20"), Some(0));
        assert_eq!(parse_cgroup("abc", "1"), None);
        assert_eq!(parse_cgroup("10", "-1"), None);
    }

    #[test]
    fn medicao_real_nao_falha_no_linux() {
        if cfg!(target_os = "linux") {
            assert!(medir(false).is_ok());
        }
    }
}
