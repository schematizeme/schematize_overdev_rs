//! Fatos sobre processos via `/proc` e sinais: a base da reconciliação de unidades órfãs.
//!
//! **O quê:** `starttime` (campo 22 de `/proc/<pid>/stat`), estado, grupo, RSS e cgroup de um pid;
//! "vivo verificado" (mesmo pid E mesmo `starttime`, senão é PID reciclado); sinal ao grupo.
//! **Onde:** `reconcilia` (decide reanexar/matar), `despacho` (RSS/cgroup da unidade).
//! Só Unix. Nada aqui entra em pânico: ausência de `/proc/<pid>` é `None`/`false`.

use std::process::Command;

/// Campos de `/proc/<pid>/stat` depois do `)` do `comm`: estado, ppid, pgrp, ... starttime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stat {
    /// Estado (`R`, `S`, `Z`...).
    pub estado: char,
    /// Grupo de processos.
    pub pgrp: u32,
    /// Instante de início, em ticks desde o boot.
    pub starttime: u64,
}

/// **O quê:** interpreta o texto de `/proc/<pid>/stat` (o `comm` pode ter espaços e `)`: corta no
/// ÚLTIMO `)`). **Onde:** [`ler_stat`] e os testes. PURA.
pub fn parse_stat(texto: &str) -> Option<Stat> {
    let resto = &texto[texto.rfind(')')? + 1..];
    let campos: Vec<&str> = resto.split_whitespace().collect();
    Some(Stat {
        estado: campos.first()?.chars().next()?,
        pgrp: campos.get(2)?.parse().ok()?,
        starttime: campos.get(19)?.parse().ok()?,
    })
}

/// **O quê:** lê o `stat` de `pid`; `None` se o processo não existe. **Onde:** a reconciliação.
pub fn ler_stat(pid: u32) -> Option<Stat> {
    parse_stat(&std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
}

/// **O quê:** `starttime` do pid (`None` se não existe). **Onde:** gravado em `Lancada`.
pub fn starttime(pid: u32) -> Option<u64> {
    ler_stat(pid).map(|s| s.starttime)
}

/// **O quê:** o pid segue vivo e É o mesmo processo gravado (mesmo `starttime`; zumbi não conta).
/// **Onde:** `reconcilia` — PID reciclado por outro processo dá `false` e NUNCA é morto.
pub fn vivo_verificado(pid: u32, starttime_gravado: u64) -> bool {
    ler_stat(pid).is_some_and(|s| s.starttime == starttime_gravado && s.estado != 'Z')
}

/// **O quê:** envia `sinal` ao grupo de `pid` se ele é líder do próprio grupo (pgrp == pid), senão
/// só ao pid. Devolve se o `kill` foi aceito. **Onde:** matar SÓ aquela unidade (e seus filhos).
/// **Efeitos:** sinal. Só use depois de [`vivo_verificado`].
pub fn sinalizar_unidade(pid: u32, sinal: i32) -> bool {
    let Some(st) = ler_stat(pid) else { return false };
    let alvo = if st.pgrp == pid { -(pid as i64) } else { pid as i64 };
    let Ok(alvo) = i32::try_from(alvo) else { return false };
    // SAFETY: `kill` não toca memória; `alvo` é um pid/pgrp validado acima (nunca 0 ou -1).
    alvo != 0 && alvo != -1 && unsafe { libc::kill(alvo, sinal) } == 0
}

/// **O quê:** `SIGKILL` na unidade (grupo). **Onde:** reconciliação ao matar uma órfã.
pub fn matar_unidade(pid: u32) -> bool {
    sinalizar_unidade(pid, libc::SIGKILL)
}

/// **O quê:** `SIGTERM` gracioso na unidade (pedido de encolher/parar do governador).
/// **Onde:** executor, ao receber `PedirEncolher`/`PedirParar`.
pub fn pedir_parar_unidade(pid: u32) -> bool {
    sinalizar_unidade(pid, libc::SIGTERM)
}

/// **O quê:** `systemctl --user stop <unidade>.scope` (só aquela unidade). Erro volta como texto.
/// **Onde:** reconciliação de unidade jaulada órfã.
pub fn parar_scope(unidade: &str) -> Result<(), String> {
    let nome = format!("{unidade}.scope");
    let s = Command::new("systemctl").args(["--user", "stop", &nome]).output();
    match s {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            Err(format!("systemctl stop {nome}: {}", String::from_utf8_lossy(&o.stderr).trim()))
        }
        Err(e) => Err(format!("systemctl indisponível: {e}")),
    }
}

/// **O quê:** RSS do processo em MiB (`VmRSS` de `/proc/<pid>/status`). **Onde:** `atual_mib` da
/// unidade sem cgroup próprio.
pub fn rss_mib(pid: u32) -> Option<u64> {
    let t = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let kib: u64 = t
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some(kib / 1024)
}

/// **O quê:** caminho do cgroup v2 do pid (linha `0::/...` de `/proc/<pid>/cgroup`), sem a raiz.
/// **Onde:** monitor/leitura de memória da unidade jaulada.
pub fn cgroup_rel(pid: u32) -> Option<String> {
    let t = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    t.lines().find_map(|l| l.strip_prefix("0::")).map(|p| p.trim().to_string())
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O `comm` com espaço e `)` não desloca os campos.
    #[test]
    fn parse_stat_com_comm_hostil() {
        let t = "123 (a b) c) S 1 123 123 0 -1 4194560 100 0 0 0 1 2 0 0 20 0 1 0 9876 1 2";
        let s = parse_stat(t).unwrap();
        assert_eq!((s.estado, s.pgrp, s.starttime), ('S', 123, 9876));
        for ruim in ["", "x", "1 (a) S", "1 (a) S 1 2"] {
            assert_eq!(parse_stat(ruim), None, "{ruim:?}");
        }
    }

    /// O próprio processo é "vivo verificado"; starttime errado (PID reciclado) não é.
    #[test]
    fn vivo_verificado_exige_mesmo_starttime() {
        let eu = std::process::id();
        let st = starttime(eu).expect("starttime próprio");
        assert!(vivo_verificado(eu, st));
        assert!(!vivo_verificado(eu, st + 1));
        assert!(!vivo_verificado(u32::MAX - 1, 1));
        assert!(rss_mib(eu).is_some());
    }
}
