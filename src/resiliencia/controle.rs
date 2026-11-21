//! Controle fino de memória do despacho: governador, histórico de picos e teto por tipo.
//!
//! **O quê:** [`Controle`] junta o `memoria::Governador` (admissão por reserva vs margem do host,
//! estrangulamento sob PSI, pedir encolher/parar antes do teto duro) ao `memoria::Historico` de
//! picos (persistido em `picos.hist`, uma linha `v1|<tipo>|<mib>` por unidade) que recomenda o
//! teto por tipo de tarefa (p95 x 1,25, clamp 500-2048 MiB; o teto por ADR do plano manda).
//! **Onde:** `executor`, a cada tick de governo e ao fechar cada unidade.
//! A margem do host vem de `sistema::margem_segura` (piso crítico + folga de cgroup).

use schematize_plataforma::memoria::{
    Decisao, EstadoHost, Fonte, Governador, Historico, Leitura, PedidoAdmissao, Politica,
    UnidadeAtiva,
};
use schematize_plataforma::sistema::MargemMemoria;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

/// Teto padrão (MiB) de um tipo sem histórico nem ADR (classe média de um agente LLM).
pub const TETO_PADRAO_MIB: u64 = 1024;
/// Quantidade "ilimitada" usada quando o host não informa memória (nunca bloqueia por falta de dado).
const SEM_INFORMACAO_MIB: u64 = 1 << 30;

/// Recomendação de teto de um tipo, para `overdev memoria status`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recomendacao {
    /// Tipo de tarefa.
    pub tipo: String,
    /// Picos recentes (MiB).
    pub picos: Vec<u64>,
    /// Teto recomendado (MiB), se há histórico.
    pub recomendado_mib: Option<u64>,
}

/// Governador + histórico + fonte de leitura do host.
pub struct Controle {
    gov: Governador,
    fonte: Arc<dyn Fonte>,
    hist: Historico,
    arq_picos: Option<PathBuf>,
    folga_cgroup_mib: Option<u64>,
    margem_mib: u64,
    /// Linhas ilegíveis de `picos.hist` (contadas, nunca engolidas em silêncio).
    pub linhas_rejeitadas: usize,
}

/// **O quê:** política do governador a partir da margem segura do host. `margem_mib` = piso
/// crítico preservado (ou o override do plano). **Onde:** `Controle::novo` e o CLI.
pub fn politica_para(
    max_paralelo: usize,
    margem_override: Option<u64>,
    m: &MargemMemoria,
) -> Politica {
    let margem_mib = margem_override.unwrap_or(m.piso_critico_mib);
    Politica { margem_mib, max_paralelo: max_paralelo.max(1), ..Politica::default() }
}

impl Controle {
    /// **O quê:** cria o controle e carrega `arq_picos` (linhas ruins são contadas). `folga` é a
    /// folga do cgroup efetivo (`MargemMemoria::folga_cgroup_mib`). **Onde:** boot do executor.
    pub fn novo(
        politica: Politica,
        fonte: Arc<dyn Fonte>,
        arq_picos: Option<PathBuf>,
        folga_cgroup_mib: Option<u64>,
    ) -> Controle {
        let margem_mib = politica.margem_mib;
        let texto = arq_picos.as_ref().and_then(|p| std::fs::read_to_string(p).ok());
        let carga = Historico::de_linhas(&texto.unwrap_or_default());
        Controle {
            gov: Governador::novo(politica),
            fonte,
            hist: carga.historico,
            arq_picos,
            folga_cgroup_mib,
            margem_mib,
            linhas_rejeitadas: carga.rejeitadas.len(),
        }
    }

    /// **O quê:** retrato do host: MemTotal/MemAvailable (MiB) e PSI `some avg10` de memória.
    /// Sem `/proc/meminfo` assume "sem informação" (não bloqueia). A folga de cgroup limita o
    /// disponível. **Onde:** `decidir` e testes (com `FixtureDir`).
    pub fn estado_host(&self, agora_ms: u64) -> EstadoHost {
        let l = Leitura::do_host(&*self.fonte);
        let mi = l.meminfo.v();
        let mib = |b: Option<u64>| b.map(|b| b >> 20);
        let total = mib(mi.and_then(|m| m.total)).unwrap_or(SEM_INFORMACAO_MIB);
        let mut disp = mib(mi.and_then(|m| m.disponivel)).unwrap_or(SEM_INFORMACAO_MIB);
        if let Some(f) = self.folga_cgroup_mib {
            disp = disp.min(self.margem_mib.saturating_add(f));
        }
        let psi = l.psi_memoria.v().and_then(|p| p.some).map_or(0.0, |s| s.avg10);
        EstadoHost { agora_ms, mem_total_mib: total, mem_disponivel_mib: disp, psi_some_avg10: psi }
    }

    /// **O quê:** um tick do governador com o host real da fonte. **Onde:** laço do executor.
    pub fn decidir(
        &mut self,
        agora_ms: u64,
        ativas: &[UnidadeAtiva],
        fila: &[PedidoAdmissao],
    ) -> (Vec<Decisao>, EstadoHost) {
        let h = self.estado_host(agora_ms);
        (self.gov.decidir(&h, ativas, fila), h)
    }

    /// **O quê:** limite de paralelismo vigente (cai sob PSI). **Onde:** telemetria e testes.
    pub fn limite_paralelo(&self) -> usize {
        self.gov.limite_paralelo()
    }

    /// **O quê:** maior teto que o host suporta (`MemTotal - margem`). **Onde:** recomendação.
    pub fn max_host_mib(&self, agora_ms: u64) -> u64 {
        self.gov.max_host_mib(&self.estado_host(agora_ms))
    }

    /// **O quê:** teto (MiB) para `tipo`: ADR do plano > recomendação do histórico > padrão.
    /// **Onde:** montagem do `Orcamento` e do pedido de admissão.
    pub fn teto_para(&self, tipo: &str, adr_mib: Option<u64>, max_host_mib: u64) -> u64 {
        adr_mib.or_else(|| self.hist.recomendar(tipo, max_host_mib)).unwrap_or(TETO_PADRAO_MIB)
    }

    /// **O quê:** registra o pico de uma unidade (memória + `picos.hist` em append). Erro de
    /// gravação sobe como texto. **Onde:** `executor`, ao fechar a unidade.
    pub fn registrar_pico(&mut self, tipo: &str, pico_mib: u64) -> Result<(), String> {
        self.hist.registrar(tipo, pico_mib).map_err(|e| e.to_string())?;
        let Some(arq) = &self.arq_picos else { return Ok(()) };
        let linha = Historico::linha(tipo, pico_mib).map_err(|e| e.to_string())? + "\n";
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(arq)
            .map_err(|e| format!("picos.hist {}: {e}", arq.display()))?;
        f.write_all(linha.as_bytes()).map_err(|e| format!("picos.hist: {e}"))
    }

    /// **O quê:** recomendação por tipo conhecido. **Onde:** `overdev memoria status`.
    pub fn recomendacoes(&self, max_host_mib: u64) -> Vec<Recomendacao> {
        self.hist
            .tipos()
            .into_iter()
            .map(|t| Recomendacao {
                tipo: t.to_string(),
                picos: self.hist.picos(t),
                recomendado_mib: self.hist.recomendar(t, max_host_mib),
            })
            .collect()
    }
}

#[cfg(test)]
mod testes;
