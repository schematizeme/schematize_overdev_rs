//! Vigilância dos cgroups das unidades: o `memoria::Monitor` com eventos traduzidos para texto.
//!
//! **O quê:** [`Vigilancia`] sobe o monitor (thread única, canal limitado), troca os alvos quando
//! unidades entram/saem e devolve os eventos já descritos como `(tipo, detalhe)`.
//! **Onde:** `executor`: cada evento vira linha `aviso` em `jaulas.jsonl` e `Observacao` no journal.
//! Lê só o kernel (`/proc`, `/sys`); nunca altera nada.

use schematize_plataforma::memoria::{Alvo, ConfigMonitor, Evento, Fonte, Monitor};
use std::sync::mpsc::Receiver;
use std::sync::Arc;

/// O monitor e o canal de eventos.
pub struct Vigilancia {
    monitor: Monitor,
    rx: Receiver<Evento>,
}

/// **O quê:** descreve um evento do monitor como `(tipo estável, detalhe legível)`.
/// **Onde:** [`Vigilancia::drenar`]. PURA.
pub fn descrever(e: &Evento) -> (String, String) {
    match e {
        Evento::CruzouHigh { alvo, atual_bytes, .. } => {
            ("cruzou_high".into(), format!("{alvo}: {} MiB acima do high", atual_bytes >> 20))
        }
        Evento::PressaoPsi { alvo, full_avg10, limiar, .. } => (
            "pressao_psi".into(),
            format!("{alvo}: PSI full {full_avg10:.1}% (limiar {limiar:.1}%)"),
        ),
        Evento::CrescimentoRapido { alvo, mib_por_s, segundos_ate_teto, .. } => (
            "crescimento_rapido".into(),
            format!("{alvo}: +{mib_por_s:.1} MiB/s, teto em {segundos_ate_teto:?} s"),
        ),
        Evento::OomKillOcorreu { alvo, delta, total } => {
            ("oom_kill".into(), format!("{alvo}: oom_kill +{delta} (total {total})"))
        }
        Evento::HostApertado { disponivel_bytes, margem_bytes } => (
            "host_apertado".into(),
            format!(
                "host com {} MiB livres (margem {} MiB)",
                disponivel_bytes >> 20,
                margem_bytes >> 20
            ),
        ),
        Evento::FalhaLeitura { alvo, motivo } => {
            ("falha_leitura".into(), format!("{alvo}: {motivo}"))
        }
    }
}

impl Vigilancia {
    /// **O quê:** inicia o monitor sem alvos. **Onde:** boot do executor. Falha ao subir a thread
    /// volta como texto (o executor segue sem monitor, com aviso).
    pub fn iniciar(cfg: &ConfigMonitor, fonte: Arc<dyn Fonte>) -> Result<Vigilancia, String> {
        let (monitor, rx) =
            Monitor::iniciar(cfg, fonte, Vec::new()).map_err(|e| format!("{e:?}"))?;
        Ok(Vigilancia { monitor, rx })
    }

    /// **O quê:** troca os alvos observados. **Onde:** quando o conjunto de unidades muda.
    pub fn alvos(&self, alvos: Vec<Alvo>) {
        self.monitor.definir_alvos(alvos);
    }

    /// **O quê:** eventos pendentes (não bloqueia). **Onde:** tick do executor.
    pub fn drenar(&self) -> Vec<(String, String)> {
        self.rx.try_iter().map(|e| descrever(&e)).collect()
    }

    /// **O quê:** eventos perdidos por canal cheio. **Onde:** alarme de consumidor lento.
    pub fn descartados(&self) -> u64 {
        self.monitor.descartados()
    }

    /// **O quê:** para e junta a thread; erro volta como texto. **Onde:** shutdown do executor.
    pub fn parar(self) -> Result<(), String> {
        self.monitor.parar().map_err(|e| format!("{e:?}"))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Cada variante vira um tipo estável (contrato com `jaulas.jsonl`).
    #[test]
    fn descreve_cada_evento() {
        let (t, d) = descrever(&Evento::OomKillOcorreu { alvo: "u".into(), delta: 1, total: 3 });
        assert_eq!(t, "oom_kill");
        assert!(d.contains("total 3"), "{d}");
        let (t, _) =
            descrever(&Evento::HostApertado { disponivel_bytes: 1 << 30, margem_bytes: 2 << 30 });
        assert_eq!(t, "host_apertado");
        let (t, _) = descrever(&Evento::FalhaLeitura { alvo: "x".into(), motivo: "m".into() });
        assert_eq!(t, "falha_leitura");
    }
}
