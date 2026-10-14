//! O subcomando `delegar` (ADR-0024) — a borda de linha de comando da estrutura de agentes.
//!
//! **O quê:** lê o brief, chama [`overdev_casa::agentes::delegar`] e imprime o resultado.
//! Códigos de saída: 0 ok · 2 uso/config inválida · 3 indisponível · 4 motor falhou.
//! **Onde:** `comandos::executar`; chamado por agentes (principal) via `--brief-stdin --json`.

use overdev_casa::agentes::{self, Agentes, Desfecho, Pedido};
use serde_json::{json, Value};
use std::io::Read;
use std::path::PathBuf;

/// **O quê:** imprime `doc` (JSON) ou `texto` e sai com `rc`. **Onde:** todo desfecho ≠ 0.
fn encerra(rc: i32, json_out: bool, doc: Value, texto: &str) -> ! {
    if json_out {
        println!("{doc}");
    } else if rc == 0 {
        println!("{texto}");
    } else {
        eprintln!("{texto}");
    }
    std::process::exit(rc)
}

/// **O quê:** erro de uso/config (rc 2). **Onde:** [`executar`].
fn uso(json_out: bool, msg: &str) -> ! {
    encerra(
        2,
        json_out,
        json!({"ok": false, "motivo": "uso", "erro": msg}),
        &format!("erro: {msg}"),
    )
}

/// **O quê:** lê o brief do arquivo ou do stdin. **Onde:** [`executar`].
fn le_brief(arquivo: Option<PathBuf>, stdin: bool) -> Result<String, String> {
    match (arquivo, stdin) {
        (Some(a), _) => std::fs::read_to_string(&a).map_err(|e| format!("{}: {e}", a.display())),
        (None, true) => {
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s).map_err(|e| format!("stdin: {e}"))?;
            Ok(s)
        }
        (None, false) => Err("informe --brief <arquivo> ou --brief-stdin".to_string()),
    }
}

/// **O quê:** `--listar`: papéis efetivos + disponibilidade de cada motor. **Onde:** [`executar`].
fn listar(ag: &Agentes, json_out: bool) {
    let motores: Vec<_> =
        agentes::Motor::TODOS.iter().map(|m| (*m, agentes::disponibilidade(*m))).collect();
    if json_out {
        let papeis: Vec<Value> = ag
            .papeis
            .iter()
            .map(|(n, p)| {
                let d = motores.iter().find(|(m, _)| *m == p.motor).map(|(_, d)| d);
                json!({"papel": n, "motor": p.motor.nome(), "modelo": p.modelo, "ativo": p.ativo,
                       "descricao": p.descricao, "disponivel": d.is_some_and(|d| d.ok())})
            })
            .collect();
        let mot: serde_json::Map<String, Value> = motores
            .iter()
            .map(|(m, d)| {
                (m.nome().to_string(), json!({"disponivel": d.ok(), "detalhe": d.detalhe()}))
            })
            .collect();
        println!(
            "{}",
            json!({"papeis": papeis, "escada": ag.escada, "roteamento": ag.roteamento, "motores": mot})
        );
        return;
    }
    println!("{:<16} {:<7} {:<16} {:<5} disponível", "papel", "motor", "modelo", "ativo");
    for (n, p) in &ag.papeis {
        let ok = motores.iter().any(|(m, d)| *m == p.motor && d.ok());
        println!("{n:<16} {:<7} {:<16} {:<5} {}", p.motor.nome(), p.modelo, p.ativo, ok);
    }
    println!("escada: {}", ag.escada.join(" -> "));
    for (m, d) in &motores {
        println!("motor {}: {}", m.nome(), d.detalhe());
    }
}

/// **O quê:** o ponto de entrada do subcomando. Sai com o rc do contrato.
/// **Onde:** `comandos::executar`.
pub fn executar(
    papel: Option<String>,
    brief: Option<PathBuf>,
    brief_stdin: bool,
    projeto: Option<PathBuf>,
    json_out: bool,
    listar_flag: bool,
) -> Result<(), String> {
    let raiz = match projeto {
        Some(p) => p,
        None => std::env::current_dir().map_err(|e| format!("cwd inacessível: {e}"))?,
    };
    let ag = match agentes::carregar(Some(&raiz)) {
        Ok(a) => a,
        Err(e) => uso(json_out, &e.to_string()),
    };
    if listar_flag {
        listar(&ag, json_out);
        return Ok(());
    }
    let Some(papel) = papel else { uso(json_out, "informe --papel <nome> (ou --listar)") };
    let brief = match le_brief(brief, brief_stdin) {
        Ok(b) => b,
        Err(e) => uso(json_out, &e),
    };
    let pedido = Pedido { papel, brief, projeto: raiz };
    match agentes::delegar(&ag, &pedido) {
        Err(e) => uso(json_out, &e.to_string()),
        Ok(Desfecho::Indisponivel { detalhe, proximo }) => encerra(
            3,
            json_out,
            json!({"ok": false, "motivo": "indisponivel", "proximo": proximo}),
            &format!(
                "indisponível: {detalhe}; próximo da escada: {}",
                proximo.as_deref().unwrap_or("(nenhum)")
            ),
        ),
        Ok(Desfecho::Executado(r)) => {
            let doc = json!({"ok": r.ok, "papel": r.papel, "motor": r.motor.nome(), "modelo": r.modelo,
                "rc": r.rc, "duracao_ms": r.duracao_ms as u64, "saida_resumo": r.saida_resumo,
                "arquivos": r.arquivos});
            let txt = format!(
                "{} ({} {}) rc={} em {} ms\n{}\n{}",
                r.papel,
                r.motor.nome(),
                r.modelo,
                r.rc,
                r.duracao_ms,
                r.saida_resumo,
                r.arquivos
            );
            encerra(if r.ok { 0 } else { 4 }, json_out, doc, &txt)
        }
    }
}
