//! O loop `run`: ticks no intervalo, shutdown gracioso por SIGTERM/SIGINT, último handoff ao sair.
//!
//! **O quê:** [`rodar`] abre o [`Motor`], instala os handlers (só flag atômica), dorme em fatias de
//! 200 ms (relógio monotônico: relógio de parede voltando não trava nem acelera o loop) e chama
//! [`Motor::tick`]. Erro de tick é registrado e o loop segue. Ao receber o sinal grava o último
//! handoff ([`Motor::finalizar`]) e sai com 0.
//! **Onde:** `overdev handoff-vivo run` (via [`super::comandos`]) e a unit systemd.

use super::ciclo::{relogio_real, Motor, Relogio};
use super::config::Config;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static PARAR: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn ao_sinal(_: libc::c_int) {
    PARAR.store(true, Ordering::SeqCst);
}

/// **O quê:** SIGTERM/SIGINT viram a flag [`PARAR`]; SIGXFSZ é ignorado (disco/arquivo cheio
/// vira erro de `write`, nunca morte do processo). **Onde:** início de [`rodar`].
#[cfg(unix)]
pub fn instalar_sinais() {
    // SAFETY: o handler só faz um store atômico (async-signal-safe); SIG_IGN é válido.
    unsafe {
        libc::signal(libc::SIGTERM, ao_sinal as extern "C" fn(libc::c_int) as libc::sighandler_t);
        libc::signal(libc::SIGINT, ao_sinal as extern "C" fn(libc::c_int) as libc::sighandler_t);
        libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
    }
}

#[cfg(not(unix))]
pub fn instalar_sinais() {}

/// **O quê:** pede ao loop que pare (usado por testes e pelo handler). **Onde:** testes.
pub fn pedir_parada() {
    PARAR.store(true, Ordering::SeqCst);
}

/// **O quê:** limpa a flag de parada (reuso do processo em testes). **Onde:** início de [`rodar`].
pub fn limpar_parada() {
    PARAR.store(false, Ordering::SeqCst);
}

/// **O quê:** dorme até `fim` em fatias de 200 ms, acordando se pediram parada. **Onde:** [`rodar`].
fn dormir_ate(fim: Instant) {
    while !PARAR.load(Ordering::SeqCst) && Instant::now() < fim {
        std::thread::sleep(
            Duration::from_millis(200).min(fim.saturating_duration_since(Instant::now())),
        );
    }
}

/// **O quê:** loop até SIGTERM/SIGINT; devolve o código de saída. **Onde:** `handoff-vivo run`.
/// **Efeitos:** lock, gravações atômicas, thread/processo do Claude.
pub fn rodar(cfg: Config) -> i32 {
    rodar_com(cfg, relogio_real())
}

/// **O quê:** [`rodar`] com relógio injetado. **Onde:** [`rodar`] e testes.
pub fn rodar_com(cfg: Config, relogio: Relogio) -> i32 {
    limpar_parada();
    instalar_sinais();
    let intervalo = Duration::from_secs(cfg.mecanico_s.max(1));
    let mut motor = match Motor::abrir(cfg, relogio) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("erro: {e}");
            return 1;
        }
    };
    while !PARAR.load(Ordering::SeqCst) {
        let inicio = Instant::now();
        if let Err(e) = motor.tick() {
            eprintln!("handoff-vivo: tick falhou (mantive a geração anterior): {e}");
        }
        dormir_ate(inicio + intervalo);
    }
    match motor.finalizar() {
        Ok(t) => {
            eprintln!(
                "handoff-vivo: encerrado; último handoff seq={}/{}",
                t.seq_context, t.seq_checklist
            );
            0
        }
        Err(e) => {
            eprintln!("erro: último handoff falhou: {e}");
            1
        }
    }
}
