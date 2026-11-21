//! Unit systemd (texto) do serviço do handoff vivo. Nada é instalado por este módulo.
//!
//! **O quê:** [`unit_systemd`] devolve o texto de `schematize-handoff-vivo.service` (user unit,
//! hardened, com teto de memória). **Onde:** quem instala (ops/market) grava em
//! `~/.config/systemd/user/`; este módulo só produz o texto.

/// **O quê:** o texto da unit. `WorkingDirectory=%h` é só default: o serviço detecta o workspace
/// subindo do cwd, então a instalação deve apontar `WorkingDirectory` para a pasta do projeto.
/// **Onde:** instalador (fora deste crate). PURA.
pub fn unit_systemd() -> String {
    "[Unit]\n\
Description=schematize handoff vivo (handoff do projeto sempre atualizado)\n\
After=default.target\n\
\n\
[Service]\n\
Type=simple\n\
WorkingDirectory=%h\n\
ExecStart=%h/.cargo/bin/schematize-overdev handoff-vivo run\n\
Restart=on-failure\n\
RestartSec=5\n\
TimeoutStopSec=30\n\
KillSignal=SIGTERM\n\
MemoryMax=256M\n\
MemorySwapMax=0\n\
NoNewPrivileges=yes\n\
ProtectSystem=strict\n\
ProtectHome=read-only\n\
ReadWritePaths=%h\n\
PrivateTmp=yes\n\
\n\
[Install]\n\
WantedBy=default.target\n"
        .to_string()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn unit_tem_as_tres_secoes_e_shutdown_gracioso() {
        let u = unit_systemd();
        for s in [
            "[Unit]",
            "[Service]",
            "[Install]",
            "KillSignal=SIGTERM",
            "MemoryMax=",
            "handoff-vivo run",
        ] {
            assert!(u.contains(s), "falta {s}");
        }
    }
}
