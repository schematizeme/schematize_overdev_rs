//! Testes do modelo: `Entregue` nunca regride, idempotência do plano, ida e volta da codificação.

use super::*;

/// Item de plano mínimo para os testes. **Onde:** todos os testes deste arquivo.
fn ip(id: &str, brief: &str) -> ItemPlano {
    ItemPlano {
        id: id.into(),
        brief: brief.into(),
        tipo: "t".into(),
        teto_mib: None,
        marca: None,
        argv: vec![],
    }
}

/// Estado com os itens `a` e `b` planejados. **Onde:** testes.
fn com_plano() -> EstadoOverdev {
    let mut s = EstadoOverdev::inicial();
    s.aplicar(&Evento::Plano {
        prefixo: "P".into(),
        modelo_base: "sonnet".into(),
        itens: vec![ip("a", "brief a"), ip("b", "brief b")],
    });
    s
}

/// Despacha `id` com o despacho `n`. **Onde:** testes.
fn despacha(s: &mut EstadoOverdev, id: &str, n: u32) {
    s.aplicar(&Evento::Despachada {
        id: id.into(),
        modelo: "sonnet".into(),
        geracao: 0,
        despacho: n,
        unidade: format!("u-{id}"),
        resfile: "r".into(),
        teto_mib: 500,
        inicio: 1,
    });
}

/// Conclui `id` com o veredito `v`. **Onde:** testes.
fn conclui(s: &mut EstadoOverdev, id: &str, v: Veredito) {
    s.aplicar(&Evento::Concluida {
        id: id.into(),
        veredito: v,
        pico_mib: Some(10),
        custo_micro_usd: 5,
        fim: 2,
    });
}

/// `Entregue` nunca regride, nem por replay de evento tardio.
#[test]
fn entregue_nunca_regride() {
    let mut s = com_plano();
    despacha(&mut s, "a", 1);
    conclui(&mut s, "a", Veredito::Concluiu);
    for ev in [
        Evento::Redespachada { id: "a".into(), motivo: "x".into() },
        Evento::Parkeada { id: "a".into(), motivo: "x".into() },
    ] {
        s.aplicar(&ev);
        assert_eq!(s.itens["a"].status, StatusItem::Entregue);
    }
    despacha(&mut s, "a", 2);
    conclui(&mut s, "a", Veredito::EstourouMemoria);
    assert_eq!(s.itens["a"].status, StatusItem::Entregue);
    assert_eq!(s.itens["a"].tentativas, 0);
}

/// Plano repetido não mexe em item existente; brief novo recria o item.
#[test]
fn plano_e_idempotente_e_brief_novo_recria() {
    let mut s = com_plano();
    despacha(&mut s, "a", 1);
    conclui(&mut s, "a", Veredito::Concluiu);
    let antes = s.clone();
    s.aplicar(&Evento::Plano {
        prefixo: "P".into(),
        modelo_base: "sonnet".into(),
        itens: vec![ip("a", "brief a"), ip("b", "brief b")],
    });
    assert_eq!(s, antes);
    s.aplicar(&Evento::Plano {
        prefixo: String::new(),
        modelo_base: String::new(),
        itens: vec![ip("a", "brief NOVO")],
    });
    assert_eq!(s.itens["a"].status, StatusItem::Pendente);
    assert_eq!(s.ordem, vec!["a", "b"]);
}

/// Falha soma tentativa e custo; perda de unidade só conta redespacho.
#[test]
fn falha_conta_tentativa_e_perda_nao() {
    let mut s = com_plano();
    despacha(&mut s, "b", 1);
    s.aplicar(&Evento::Lancada { id: "b".into(), pid: 7, starttime: 9 });
    assert_eq!(s.voo["b"].pid, Some(7));
    s.aplicar(&Evento::Redespachada { id: "b".into(), motivo: "orfa".into() });
    assert_eq!((s.itens["b"].tentativas, s.itens["b"].redespachos), (0, 1));
    assert_eq!(s.itens["b"].status, StatusItem::Pendente);
    despacha(&mut s, "b", 2);
    conclui(&mut s, "b", Veredito::Timeout);
    assert_eq!((s.itens["b"].tentativas, s.custo_micro_usd), (1, 5));
    assert_eq!(s.pendentes().len(), 2);
    assert!(!s.concluido());
}

/// Codificação ida e volta (estado e evento) e entrada hostil vira `Err`, nunca panic.
#[test]
fn codec_ida_e_volta_e_hostil() {
    let s = com_plano();
    assert_eq!(EstadoOverdev::decodificar(&s.codificar()).unwrap(), s);
    let ev = Evento::Observacao { tipo: "ç".into(), detalhe: "\u{0}\u{1F600}".into() };
    assert_eq!(EstadoOverdev::decodificar_evento(&EstadoOverdev::codificar_evento(&ev)), Ok(ev));
    for ruim in [&b""[..], b"{", b"\xff\xfe", b"{\"t\":\"Nada\"}"] {
        assert!(EstadoOverdev::decodificar(ruim).is_err());
        assert!(EstadoOverdev::decodificar_evento(ruim).is_err());
    }
}

/// O anel de observações é limitado.
#[test]
fn observacoes_sao_limitadas() {
    let mut s = EstadoOverdev::inicial();
    for n in 0..100 {
        s.aplicar(&Evento::Observacao { tipo: "x".into(), detalhe: n.to_string() });
    }
    assert_eq!(s.observacoes.len(), MAX_OBSERVACOES);
    assert!(s.observacoes.last().unwrap().ends_with("99"));
}
