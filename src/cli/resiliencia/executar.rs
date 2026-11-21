//! `overdev executar [--plano <json>]` e `overdev retomar`: o run durável.
//!
//! **O quê:** abre o estado durável (journal + snapshot), registra o plano, monta governador e
//! despachante e roda o executor até tudo ser entregue/parkeado. Com estado existente a retomada
//! é automática (reconcilia órfãs e re-despacha só o que estava em voo); nunca pergunta.
//! **Onde:** `cli::comandos::executar`; o `schematize-vigia` chama `retomar` com `--resume`.

use crate::cli::args::OpcoesExec;
use crate::resiliencia::controle::{politica_para, Controle};
use crate::resiliencia::despacho::DespachanteProcesso;
use crate::resiliencia::duravel::{Duravel, PoliticaSnapshot};
use crate::resiliencia::executor::{Executor, Opcoes, Resumo, PARAR};
use crate::resiliencia::modelo::StatusItem;
use crate::resiliencia::plano::Plano;
use schematize_plataforma::jaula::{detectar_backend, Backend};
use schematize_plataforma::memoria::FonteDir;
use schematize_plataforma::sistema::identificar_host;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// Handler de SIGTERM/SIGINT: só levanta a flag (async-signal-safe).
extern "C" fn ao_sinal(_: libc::c_int) {
    PARAR.store(true, Ordering::Relaxed);
}

/// **O quê:** instala o handler de parada graciosa (as unidades NÃO são mortas: são reanexadas
/// na retomada). **Onde:** início de `executar`.
fn instalar_sinais() {
    let h = ao_sinal as extern "C" fn(libc::c_int) as libc::sighandler_t;
    // SAFETY: o handler só faz um store atômico; `signal` é chamado uma vez, antes das threads.
    unsafe {
        libc::signal(libc::SIGTERM, h);
        libc::signal(libc::SIGINT, h);
    }
}

/// **O quê:** `snapshot a cada N eventos` do ambiente (`OVERDEV_SNAPSHOT_EVENTOS`, para testes);
/// padrão 50 eventos / 30 s. **Onde:** `executar`.
fn politica_snapshot() -> PoliticaSnapshot {
    let mut p = PoliticaSnapshot::default();
    if let Some(n) = std::env::var("OVERDEV_SNAPSHOT_EVENTOS").ok().and_then(|v| v.parse().ok()) {
        p.cada_eventos = n;
    }
    p
}

/// **O quê:** `(margem, folga de cgroup)` em MiB: `--margem-mib`/plano fixam a margem e dispensam
/// identificar o host; senão vem de `sistema::margem_segura` (piso crítico + folga do cgroup).
/// **Onde:** `executar`.
fn margem_do_host(fixa: Option<u64>) -> (u64, Option<u64>) {
    if let Some(m) = fixa {
        return (m, None);
    }
    let m = identificar_host().margem_segura();
    (m.piso_critico_mib, m.folga_cgroup_mib)
}

/// **O quê:** o evento `Plano` só entra se o estado ainda não tem os itens (briefs idênticos).
/// **Onde:** `executar`.
fn plano_ja_registrado(d: &Duravel, p: &Plano) -> bool {
    let e = d.estado();
    let itens_ok = p.itens.iter().all(|i| {
        e.itens
            .get(&i.id)
            .is_some_and(|x| x.brief_hash == crate::resiliencia::modelo::hash_brief(&i.brief))
    });
    itens_ok && e.prefixo == p.prefixo && e.modelo_base == p.modelo
}

/// **O quê:** banner de retomada (stderr): o que o journal/snapshot recuperaram. **Onde:** `executar`.
fn banner(d: &Duravel) {
    let a = &d.abertura;
    if let (Some(q), Some(m)) = (&a.quarentena, &a.motivo_quarentena) {
        eprintln!("overdev: estado ilegível ({m}); movido para {} e recomeçando", q.display());
    }
    if let Some(av) = &a.relatorio.snapshot_aviso {
        eprintln!("overdev: {av}");
    }
    if a.relatorio.bytes_descartados > 0 {
        let rel = &a.relatorio;
        eprintln!(
            "overdev: descartei {} byte(s) de cauda torn do journal ({:?})",
            rel.bytes_descartados, rel.motivo
        );
    }
    if a.retomando {
        let e = d.estado();
        let n = |s: StatusItem| e.itens.values().filter(|i| i.status == s).count();
        eprintln!(
            "overdev: retomando do seq {}: {} entregue(s), {} em voo, {} pendente(s), {} parkeado(s)",
            d.seq(),
            n(StatusItem::Entregue),
            e.voo.len(),
            n(StatusItem::Pendente) + n(StatusItem::Falhou),
            n(StatusItem::Parkeado)
        );
    }
}

/// **O quê:** resolve `(dir_estado, control-plane)`: `--estado` ou `<control-plane>/estado`.
/// **Onde:** `executar`.
fn diretorios(o: &OpcoesExec) -> Result<(PathBuf, PathBuf), String> {
    let cwd = std::env::current_dir().map_err(|e| format!("cwd inacessível: {e}"))?;
    let estado = o.estado.clone().unwrap_or_else(|| crate::resiliencia::dir_estado(&cwd));
    let od = estado.parent().map(Path::to_path_buf).unwrap_or(cwd);
    Ok((estado, od))
}

/// **O quê:** exige jaula (systemd) a menos que `--sem-jaula`: nunca roda sem teto em silêncio.
/// **Onde:** `executar`.
fn exigir_jaula(o: &OpcoesExec) -> Result<bool, String> {
    if o.sem_jaula {
        return Ok(false);
    }
    if detectar_backend() == Backend::SystemdScope {
        return Ok(true);
    }
    Err(crate::agentrun::jaulado::mensagem_sem_backend(
        "não há `systemd-run --user` com o controller `memory` delegado. Se preferir seguir SEM \
         teto, repita com `--sem-jaula`.",
    ))
}

/// **O quê:** o comando: prepara tudo e roda o executor. **Onde:** `Cmd::Executar`/`Cmd::Retomar`.
pub fn executar(plano: Option<&Path>, o: &OpcoesExec) -> Result<(), String> {
    let jaulado = exigir_jaula(o)?;
    let plano = plano.map(Plano::carregar).transpose()?;
    let (estado, od) = diretorios(o)?;
    let mut d = Duravel::abrir(&estado, politica_snapshot()).map_err(|e| e.to_string())?;
    banner(&d);
    if let Some(p) = &plano {
        if !plano_ja_registrado(&d, p) {
            d.registrar(&p.evento()).map_err(|e| e.to_string())?;
        }
    } else if d.estado().itens.is_empty() {
        return Err("nada a retomar: não há estado em ".to_string()
            + &estado.display().to_string()
            + "; informe `--plano <arquivo.json>`");
    }
    let paralelo = o.paralelo.or(plano.as_ref().map(|p| p.paralelo)).unwrap_or(2);
    let (margem, folga) =
        margem_do_host(o.margem_mib.or(plano.as_ref().and_then(|p| p.margem_mib)));
    let pol = politica_para(paralelo, margem);
    let ctl = Controle::novo(pol, Arc::new(FonteDir::real()), Some(od.join("picos.hist")), folga);
    let op = Opcoes {
        jaulado,
        matar_orfas: o.matar_orfas,
        checklist: o.checklist.clone(),
        monitorar: true,
        ..Opcoes::default()
    };
    instalar_sinais();
    let seq = d.seq();
    let r = Executor::novo(d, ctl, Box::new(DespachanteProcesso), op, od).rodar();
    resultado(r.map_err(|e| e.to_string())?, seq, o.json)
}

/// **O quê:** imprime o resumo (texto ou JSON) e converte "incompleto" em erro (exit 1: o vigia
/// relança). **Onde:** fim de `executar`.
fn resultado(r: Resumo, seq_inicial: u64, json: bool) -> Result<(), String> {
    if json {
        let v = serde_json::json!({
            "total": r.total, "entregues": r.entregues, "parkeados": r.parkeados,
            "interrompido": r.interrompido, "devolvidas": r.devolvidas,
            "seq_inicial": seq_inicial, "reconciliacao": r.reconciliacao,
        });
        println!("{v}");
    } else {
        println!("{} de {} entregue(s), {} parkeado(s).", r.entregues, r.total, r.parkeados);
    }
    if r.completo() {
        Ok(())
    } else {
        Err("run incompleto (interrompido): `schematize-overdev retomar` continua de onde parou"
            .into())
    }
}
