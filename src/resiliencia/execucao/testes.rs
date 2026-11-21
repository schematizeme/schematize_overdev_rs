//! Testes da decisão de próximo passo (cascata, OOM, parkeamento) e do uso lido do log.

use super::*;
use crate::resiliencia::modelo::{ItemPlano, TentativaReg};

/// Item com o histórico `h` (modelo, veredito) e a geração `g`. **Onde:** testes.
fn item(h: &[(&str, Veredito)], g: u32) -> Item {
    let plano = ItemPlano {
        id: "i".into(),
        brief: "b".into(),
        tipo: "t".into(),
        teto_mib: None,
        marca: None,
        argv: vec![],
    };
    Item {
        brief_hash: 1,
        plano,
        status: StatusItem::Falhou,
        tentativas: h.len() as u32,
        geracao: g,
        despachos: h.len() as u32,
        redespachos: 0,
        modelo: h.last().map_or("sonnet", |t| t.0).to_string(),
        historico: h
            .iter()
            .map(|(m, v)| TentativaReg { modelo: m.to_string(), veredito: *v })
            .collect(),
        pico_mib: Some(900),
        motivo_park: None,
    }
}

/// Sem histórico despacha direto no modelo do item.
#[test]
fn sem_historico_despacha() {
    let p = decidir_proximo(&item(&[], 0), "sonnet");
    assert_eq!(p, Proximo::Despachar { modelo: "sonnet".into(), geracao: 0, nota: None });
}

/// Falha de capacidade: 2 rodadas no mesmo modelo, depois re-decompõe, depois escala, depois parkeia.
#[test]
fn falhas_de_capacidade_seguem_a_escada() {
    let f = Veredito::Falhou;
    let um = decidir_proximo(&item(&[("sonnet", f)], 0), "sonnet");
    assert!(
        matches!(um, Proximo::Despachar { ref modelo, geracao: 0, nota: None } if modelo == "sonnet")
    );
    let dois = decidir_proximo(&item(&[("sonnet", f), ("sonnet", f)], 0), "sonnet");
    assert!(matches!(dois, Proximo::Despachar { geracao: 1, nota: Some(_), .. }), "{dois:?}");
    let tres = decidir_proximo(&item(&[("sonnet", f), ("sonnet", f), ("sonnet", f)], 1), "sonnet");
    assert!(matches!(tres, Proximo::Despachar { ref modelo, .. } if modelo == "opus"), "{tres:?}");
    let fim = decidir_proximo(&item(&[("opus", f), ("opus", f), ("opus", f)], 1), "sonnet");
    assert!(matches!(fim, Proximo::Parkear(_)), "{fim:?}");
}

/// OOM re-decompõe até o limite de gerações e então parkeia (nunca repete igual).
#[test]
fn oom_redecompoe_e_depois_parkeia() {
    let oom = Veredito::EstourouMemoria;
    let p = decidir_proximo(&item(&[("sonnet", oom)], 0), "sonnet");
    assert!(
        matches!(p, Proximo::Despachar { geracao: 1, nota: Some(ref n), .. } if n.contains("MORTA-POR-OOM")),
        "{p:?}"
    );
    let esgotou = decidir_proximo(
        &item(&[("sonnet", oom)], crate::agentrun::jaulado::MAX_GERACOES),
        "sonnet",
    );
    assert!(matches!(esgotou, Proximo::Parkear(_)), "{esgotou:?}");
}

/// Modelo: substring decide; desconhecido é Sonnet.
#[test]
fn modelo_por_substring() {
    assert_eq!(parse_modelo("claude-haiku-5"), Modelo::Haiku);
    assert_eq!(parse_modelo("OPUS"), Modelo::Opus);
    assert_eq!(parse_modelo("qualquer"), Modelo::Sonnet);
}

/// Uso do log: última linha JSON com `usage`; lixo/ausência = uso vazio e custo 0.
#[test]
fn uso_do_log_le_ultima_linha_json() {
    let d = std::env::temp_dir().join(format!("od-uso-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    let log = d.join("u.log");
    std::fs::write(&log, "ruído\n{\"usage\":{\"input_tokens\":1000,\"output_tokens\":2000}}\n\n")
        .unwrap();
    let (u, c) = uso_do_log(&log, "claude-sonnet-5");
    assert_eq!((u.input, u.output), (1000, 2000));
    assert!(c > 0);
    std::fs::write(&log, "{truncado").unwrap();
    assert_eq!(uso_do_log(&log, "claude-sonnet-5"), (Uso::default(), 0));
    assert_eq!(uso_do_log(&d.join("nao-existe"), "x"), (Uso::default(), 0));
    std::fs::remove_dir_all(&d).unwrap();
}
