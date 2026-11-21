//! Testes do controle: PSI simulado estrangula, teto recomendado respeita clamp e ADR.

use super::*;
use schematize_plataforma::memoria::{FixtureDir, FonteDir};

/// `/proc/meminfo` com `total_mib`/`disp_mib`. **Onde:** fixtures.
fn meminfo(total_mib: u64, disp_mib: u64) -> String {
    format!("MemTotal: {} kB\nMemAvailable: {} kB\n", total_mib * 1024, disp_mib * 1024)
}

/// PSI `some avg10=<x>`. **Onde:** fixtures.
fn psi(x: f64) -> String {
    format!("some avg10={x:.2} avg60=0.00 avg300=0.00 total=0\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0\n")
}

/// Controle sobre uma fixture com host folgado e PSI baixo. **Onde:** testes.
fn controle(fx: &FixtureDir, picos: Option<PathBuf>) -> Controle {
    fx.escrever("proc/meminfo", &meminfo(16_000, 12_000)).unwrap();
    fx.escrever("proc/pressure/memory", &psi(0.5)).unwrap();
    let pol = Politica { margem_mib: 1024, max_paralelo: 4, ..Politica::default() };
    Controle::novo(pol, Arc::new(FonteDir::com_raiz(fx.raiz())), picos, None)
}

/// Unidade ativa de teste. **Onde:** testes.
fn ativa(id: &str) -> UnidadeAtiva {
    UnidadeAtiva { id: id.into(), teto_mib: 500, atual_mib: 100, mib_por_s: 0.0 }
}

/// Fila de `n` pedidos de 500 MiB. **Onde:** testes.
fn fila(n: usize) -> Vec<PedidoAdmissao> {
    (0..n).map(|i| PedidoAdmissao { id: format!("f{i}"), teto_mib: 500 }).collect()
}

/// Host folgado admite até o limite; PSI alto estrangula e para de admitir; PSI baixo libera.
#[test]
fn psi_simulado_estrangula_e_libera() {
    let fx = FixtureDir::nova("ctl-psi").unwrap();
    let mut c = controle(&fx, None);
    let (d, h) = c.decidir(1000, &[], &fila(8));
    assert_eq!(h.mem_total_mib, 16_000);
    assert!(d.contains(&Decisao::Admitir { quantas: 4 }), "{d:?}");
    let ativas = [ativa("a"), ativa("b"), ativa("c")];
    fx.escrever("proc/pressure/memory", &psi(40.0)).unwrap();
    let (d, _) = c.decidir(2000, &ativas, &fila(3));
    assert!(d.iter().any(|x| matches!(x, Decisao::Estrangular { .. })), "{d:?}");
    assert!(!d.iter().any(|x| matches!(x, Decisao::Admitir { .. })), "sob PSI não admite: {d:?}");
    assert!(c.limite_paralelo() <= 2, "limite {}", c.limite_paralelo());
    fx.escrever("proc/pressure/memory", &psi(0.1)).unwrap();
    let (d, _) = c.decidir(3000, &ativas, &[]);
    assert!(d.contains(&Decisao::Liberar), "{d:?}");
}

/// Host sem folga (MemAvailable < margem) não admite.
#[test]
fn host_sem_margem_nao_admite() {
    let fx = FixtureDir::nova("ctl-margem").unwrap();
    let mut c = controle(&fx, None);
    fx.escrever("proc/meminfo", &meminfo(16_000, 600)).unwrap();
    let (d, _) = c.decidir(1000, &[], &fila(2));
    assert!(!d.iter().any(|x| matches!(x, Decisao::Admitir { .. })), "{d:?}");
}

/// Teto: sem histórico = padrão; com picos = p95 x 1,25 clampado; ADR do plano vence.
#[test]
fn teto_recomendado_clamp_e_adr() {
    let fx = FixtureDir::nova("ctl-teto").unwrap();
    let arq = fx.raiz().join("picos.hist");
    let mut c = controle(&fx, Some(arq.clone()));
    assert_eq!(c.teto_para("build", None, 10_000), TETO_PADRAO_MIB);
    for p in [300, 320, 340, 360] {
        c.registrar_pico("build", p).unwrap();
    }
    assert_eq!(c.teto_para("build", None, 10_000), 500, "pico baixo sobe ao piso de 500");
    for p in [3000, 3200] {
        c.registrar_pico("pesado", p).unwrap();
    }
    assert_eq!(c.teto_para("pesado", None, 10_000), 2048, "pico alto cai no teto de 2048");
    assert_eq!(c.teto_para("pesado", Some(4096), 10_000), 4096, "ADR do plano é respeitado");
    let relido = controle(&fx, Some(arq));
    assert_eq!(relido.teto_para("pesado", None, 10_000), 2048);
    assert_eq!(relido.recomendacoes(10_000).len(), 2);
    assert!(c.registrar_pico("a|b", 1).is_err(), "tipo inválido é erro, não silêncio");
}

/// Sem `/proc/meminfo` o controle não bloqueia nem entra em pânico.
#[test]
fn sem_meminfo_nao_bloqueia() {
    let fx = FixtureDir::nova("ctl-vazio").unwrap();
    let pol = Politica::default();
    let mut c = Controle::new_vazio(pol, &fx);
    let (d, _) = c.decidir(1000, &[], &fila(1));
    assert!(d.contains(&Decisao::Admitir { quantas: 1 }), "{d:?}");
}

impl Controle {
    /// **O quê:** controle sobre fixture sem arquivos. **Onde:** só testes.
    fn new_vazio(pol: Politica, fx: &FixtureDir) -> Controle {
        Controle::novo(pol, Arc::new(FonteDir::com_raiz(fx.raiz())), None, None)
    }
}
