//! Testes de `pergunta` (separados para manter o arquivo abaixo do teto de 750 linhas).

use super::*;

fn base(kind: Kind) -> Pergunta {
    Pergunta {
        id: "q-abc-1".into(),
        criada_em: 1_790_000_000,
        fase: "FH".into(),
        titulo: "`services` merece tela própria?".into(),
        contexto: "Hoje é seção colapsável dos limites.".into(),
        kind,
        opcoes: vec![],
        auto: String::new(),
        linka: vec![],
        resposta: None,
    }
}

fn com_opcoes(kind: Kind) -> Pergunta {
    let mut p = base(kind);
    p.opcoes = vec![
        Opcao { id: "propria".into(), rotulo: "Tela própria".into(), detalhe: String::new() },
        Opcao {
            id: "secundaria".into(),
            rotulo: "Lista secundária".into(),
            detalhe: String::new(),
        },
    ];
    p
}

/// **O CONTRATO.** Renomear um campo faz este teste mostrar a diferença — é o que substitui
/// escrever o JSON à mão, e a razão está no doc do módulo.
#[test]
fn o_shape_do_json_e_contrato() {
    let mut p = com_opcoes(Kind::Escolha);
    p.auto = "secundaria".into();
    p.linka = vec!["ovf:q:xyz".into()];
    p.resposta = Some(Resposta {
        em: 1_790_000_100,
        por: "humano".into(),
        escolhas: vec!["propria".into()],
        texto: String::new(),
        revisada: false,
        revisao: String::new(),
    });
    let v: serde_json::Value = serde_json::to_value(&p).expect("serializa");
    // **Nomes em ordem alfabética, e não na ordem do `struct`.** O `serde_json` guarda o
    // objeto num mapa ordenado, e afirmar a ordem de declaração faria este teste reprovar
    // por um motivo que NÃO é o contrato — ordem de chave não é contrato em JSON, o
    // CONJUNTO de nomes é. O primeiro rascunho deste teste errava exatamente nisso, e
    // reprovou na primeira execução.
    let chaves = |x: &serde_json::Value| -> Vec<String> {
        x.as_object().expect("objeto").keys().cloned().collect()
    };
    assert_eq!(
        chaves(&v),
        [
            "auto",
            "contexto",
            "criada_em",
            "fase",
            "id",
            "kind",
            "linka",
            "opcoes",
            "resposta",
            "titulo"
        ],
        "os nomes de topo são contrato com a GUI — mudar um tem de aparecer AQUI"
    );
    assert_eq!(chaves(&v["resposta"]), ["em", "escolhas", "por", "revisada", "revisao", "texto"]);
    assert_eq!(chaves(&v["opcoes"][0]), ["detalhe", "id", "rotulo"]);
    // O `kind` vai em snake_case, e a GUI casa por este texto.
    assert_eq!(v["kind"], "escolha");
    assert_eq!(serde_json::to_value(Kind::SimNaoOutro).unwrap(), "sim_nao_outro");
}

/// Ida e volta não perde nada.
#[test]
fn serializa_e_volta_igual() {
    let p = com_opcoes(Kind::Multipla);
    let s = serde_json::to_string(&p).unwrap();
    assert_eq!(serde_json::from_str::<Pergunta>(&s).unwrap(), p);
}

// ---------------------------------------------------------------------
// As invariantes, cada uma vista REPROVANDO.
// ---------------------------------------------------------------------

/// **(b)** `escolha`/`multipla` com menos de 2 opções — o caso que gerou este módulo.
#[test]
fn pergunta_de_uma_opcao_nao_e_pergunta() {
    for k in [Kind::Escolha, Kind::Multipla] {
        let mut p = base(k);
        assert!(
            matches!(p.validar().as_slice(), [Invalida::OpcoesInsuficientes(0)]),
            "{k:?} sem opção tinha de reprovar"
        );
        p.opcoes = vec![Opcao { id: "a".into(), rotulo: "A".into(), detalhe: String::new() }];
        assert!(
            matches!(p.validar().as_slice(), [Invalida::OpcoesInsuficientes(1)]),
            "{k:?} com UMA opção tinha de reprovar"
        );
        let ok = com_opcoes(k);
        assert!(ok.validar().is_empty(), "{k:?} com duas opções é válida");
    }
}

/// **(a)** A resposta só pode escolher id declarado. Senão é prosa disfarçada de decisão.
#[test]
fn escolha_fora_das_opcoes_reprova() {
    let mut p = com_opcoes(Kind::Escolha);
    p.resposta = Some(Resposta {
        em: 1,
        por: "humano".into(),
        escolhas: vec!["sim".into()],
        texto: String::new(),
        revisada: false,
        revisao: String::new(),
    });
    assert_eq!(
        p.validar(),
        vec![Invalida::EscolhaDesconhecida("sim".into())],
        "um `sim` numa pergunta de duas pontas é exatamente o bug de 2026-09-24"
    );
}

/// **(c)** `auto` tem de apontar opção existente, senão a tela mostra sugestão fantasma.
#[test]
fn auto_fora_das_opcoes_reprova() {
    let mut p = com_opcoes(Kind::Escolha);
    p.auto = "nao-existe".into();
    assert_eq!(p.validar(), vec![Invalida::AutoDesconhecida("nao-existe".into())]);
    p.auto = "propria".into();
    assert!(p.validar().is_empty());
}

/// **(e)** Id de opção duplicado deixaria a resposta ambígua.
#[test]
fn opcao_duplicada_reprova() {
    let mut p = com_opcoes(Kind::Escolha);
    p.opcoes.push(Opcao { id: "propria".into(), rotulo: "De novo".into(), detalhe: String::new() });
    assert_eq!(p.validar(), vec![Invalida::OpcaoDuplicada("propria".into())]);
}

/// `kind` de opções implícitas não recebe lista — ou o kind está errado, ou a lista é ruído.
#[test]
fn kind_implicito_com_opcoes_reprova() {
    for k in [Kind::Aprovar, Kind::SimNaoOutro, Kind::Livre] {
        let p = com_opcoes(k);
        assert_eq!(p.validar(), vec![Invalida::OpcoesInesperadas], "{k:?}");
        assert!(base(k).validar().is_empty(), "{k:?} sem opções é válida");
    }
}

/// **O BURACO QUE O TESTE DE PONTA A PONTA ACHOU.** As opções estavam declaradas, a
/// validação de `escolhas` funcionava — e `--texto "sim"` passava por baixo, gravando
/// exatamente a resposta ambígua que este formato existe para impedir.
#[test]
fn texto_livre_num_kind_de_clique_reprova() {
    let mut p = com_opcoes(Kind::Escolha);
    p.resposta = Some(Resposta {
        em: 1,
        por: "humano".into(),
        escolhas: vec!["propria".into()],
        texto: "sim".into(),
        revisada: false,
        revisao: String::new(),
    });
    assert!(
        p.validar().contains(&Invalida::TextoOndeSoCabeClique),
        "texto num `escolha` é a ambiguidade voltando pela porta dos fundos: {:?}",
        p.validar()
    );
    // E em `aprovar`/`multipla` também.
    for k in [Kind::Aprovar, Kind::Multipla] {
        let mut q = if k.exige_opcoes() { com_opcoes(k) } else { base(k) };
        let escolha = if k.exige_opcoes() { "propria" } else { "aprovar" };
        q.resposta = Some(Resposta {
            em: 1,
            por: "humano".into(),
            escolhas: vec![escolha.into()],
            texto: "talvez".into(),
            revisada: false,
            revisao: String::new(),
        });
        assert!(q.validar().contains(&Invalida::TextoOndeSoCabeClique), "{k:?}");
    }
}

/// Onde o texto é legítimo, ele passa — senão o `sim_nao_outro` perderia o "outro".
#[test]
fn texto_passa_onde_a_resposta_e_aberta() {
    for k in [Kind::Livre, Kind::SimNaoOutro] {
        let mut p = base(k);
        p.resposta = Some(Resposta {
            em: 1,
            por: "humano".into(),
            escolhas: vec![],
            texto: "pode fazer".into(),
            revisada: false,
            revisao: String::new(),
        });
        assert!(p.validar().is_empty(), "{k:?} tinha de aceitar texto: {:?}", p.validar());
    }
}

/// Resposta que não escolheu nada, num kind de clique, não decidiu nada.
#[test]
fn kind_de_clique_sem_escolha_reprova() {
    for k in [Kind::Aprovar, Kind::Escolha, Kind::Multipla] {
        let mut p = if k.exige_opcoes() { com_opcoes(k) } else { base(k) };
        p.resposta = Some(Resposta {
            em: 1,
            por: "humano".into(),
            escolhas: vec![],
            texto: String::new(),
            revisada: false,
            revisao: String::new(),
        });
        assert!(p.validar().contains(&Invalida::SemEscolha), "{k:?}");
    }
}

/// `escolha` é UMA de N. Duas viram `multipla`, ou o kind está errado.
#[test]
fn escolha_com_duas_reprova_e_multipla_aceita() {
    let mut p = com_opcoes(Kind::Escolha);
    let r = Resposta {
        em: 1,
        por: "humano".into(),
        escolhas: vec!["propria".into(), "secundaria".into()],
        texto: String::new(),
        revisada: false,
        revisao: String::new(),
    };
    p.resposta = Some(r.clone());
    assert!(p.validar().contains(&Invalida::EscolhaMultiplaOndeCabeUma(2)), "{:?}", p.validar());
    let mut q = com_opcoes(Kind::Multipla);
    q.resposta = Some(r);
    assert!(q.validar().is_empty(), "`multipla` com duas é o caso normal: {:?}", q.validar());
}

/// `aprovar` e `sim_nao_outro` respondem por ids IMPLÍCITOS, sem declarar opção.
#[test]
fn opcoes_implicitas_sao_ids_validos_e_viram_rotulo() {
    let mut p = base(Kind::Aprovar);
    p.resposta = Some(Resposta {
        em: 1,
        por: "humano".into(),
        escolhas: vec!["aprovar".into()],
        texto: String::new(),
        revisada: false,
        revisao: String::new(),
    });
    assert!(p.validar().is_empty(), "{:?}", p.validar());
    assert_eq!(p.resposta_em_texto(), "Aprovar", "o log é para pessoa ler");

    // E um id que NÃO é implícito nem declarado segue reprovando.
    p.resposta.as_mut().unwrap().escolhas = vec!["talvez".into()];
    assert_eq!(p.validar(), vec![Invalida::EscolhaDesconhecida("talvez".into())]);

    let mut q = base(Kind::SimNaoOutro);
    q.resposta = Some(Resposta {
        em: 1,
        por: "humano".into(),
        escolhas: vec!["nao".into()],
        texto: String::new(),
        revisada: false,
        revisao: String::new(),
    });
    assert!(q.validar().is_empty(), "{:?}", q.validar());
    assert_eq!(q.resposta_em_texto(), "Não");
}

#[test]
fn sem_id_ou_sem_titulo_reprova() {
    let mut p = base(Kind::Livre);
    p.id = "  ".into();
    assert!(p.validar().contains(&Invalida::SemId));
    let mut p = base(Kind::Livre);
    p.titulo = String::new();
    assert!(p.validar().contains(&Invalida::SemTitulo));
}

/// Devolve TODOS os problemas, não o primeiro — quem conserta à mão quer a lista.
#[test]
fn validar_acumula_em_vez_de_parar_no_primeiro() {
    let mut p = base(Kind::Escolha);
    p.id = String::new();
    p.titulo = String::new();
    p.auto = "x".into();
    assert!(p.validar().len() >= 4, "{:?}", p.validar());
}

// ---------------------------------------------------------------------
// O estado que o usuário pediu: respondida != liberada.
// ---------------------------------------------------------------------

/// **"vc tem que revisar minha resposta, não passar direto"** — este é o teste disso.
#[test]
fn respondida_sem_revisao_nao_esta_fechada() {
    let mut p = com_opcoes(Kind::Escolha);
    assert!(p.aberta(), "sem resposta está aberta");
    assert!(!p.aguarda_revisao());

    p.resposta = Some(Resposta {
        em: 1_790_000_100,
        por: "humano".into(),
        escolhas: vec!["propria".into()],
        texto: String::new(),
        revisada: false,
        revisao: String::new(),
    });
    assert!(!p.aberta(), "já foi respondida");
    assert!(p.aguarda_revisao(), "respondida e NÃO revisada: a máquina não pode agir ainda");

    p.resposta.as_mut().unwrap().revisada = true;
    p.resposta.as_mut().unwrap().revisao = "escolheu tela própria; item E3 liberado".into();
    assert!(!p.aguarda_revisao(), "revisada: agora libera");
}

/// A resposta em texto usa o RÓTULO, não o id — o log é para pessoa ler.
#[test]
fn resposta_em_texto_mostra_rotulo() {
    let mut p = com_opcoes(Kind::Escolha);
    assert_eq!(p.resposta_em_texto(), "(sem resposta)");
    p.resposta = Some(Resposta {
        em: 1,
        por: "humano".into(),
        escolhas: vec!["propria".into()],
        texto: String::new(),
        revisada: false,
        revisao: String::new(),
    });
    assert_eq!(p.resposta_em_texto(), "Tela própria");
    // Sim/Não/outro: o texto entra junto.
    let mut q = base(Kind::SimNaoOutro);
    q.resposta = Some(Resposta {
        em: 1,
        por: "humano".into(),
        escolhas: vec![],
        texto: "pode fazer".into(),
        revisada: false,
        revisao: String::new(),
    });
    assert_eq!(q.resposta_em_texto(), "pode fazer");
}

// ---------------------------------------------------------------------
// Disco.
// ---------------------------------------------------------------------

/// Id com `/` ou `..` não escreve fora do diretório.
#[test]
fn id_hostil_nao_escapa_do_diretorio() {
    let raiz = Path::new("/tmp/ovtest");
    let c = caminho(raiz, "../../etc/passwd");
    assert!(c.starts_with(dir_perguntas(raiz)), "{}", c.display());
    assert!(!c.to_string_lossy().contains(".."), "{}", c.display());
}

/// Gravar recusa a inválida — o erro chega em quem causou, não em quem lê depois.
#[test]
fn gravar_recusa_pergunta_invalida() {
    let tmp = std::env::temp_dir().join(format!("ovp-{}", std::process::id()));
    let p = base(Kind::Escolha); // sem opções
    let e = gravar(&tmp, &p).expect_err("tinha de recusar");
    assert!(e.contains("2 opções ou mais"), "{e}");
    let _ = std::fs::remove_dir_all(&tmp);
}

/// Ciclo real: grava, lê de volta, lista.
#[test]
fn grava_le_e_lista() {
    let tmp = std::env::temp_dir().join(format!(
        "ovp2-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    let p = com_opcoes(Kind::Escolha);
    let arq = gravar(&tmp, &p).expect("grava");
    assert_eq!(ler(&arq).expect("lê"), p);
    let (ok, ruins) = listar(&tmp);
    assert_eq!(ok, vec![p]);
    assert!(ruins.is_empty());
    let _ = std::fs::remove_dir_all(&tmp);
}

/// **Arquivo ruim NÃO é pulado em silêncio** — pergunta que desaparece da fila é pior que
/// erro na tela: a pessoa acha que respondeu tudo.
#[test]
fn arquivo_ilegivel_aparece_na_lista_de_ruins() {
    let tmp = std::env::temp_dir().join(format!(
        "ovp3-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    gravar(&tmp, &com_opcoes(Kind::Escolha)).expect("grava a boa");
    let d = dir_perguntas(&tmp);
    std::fs::write(d.join("q-lixo.json"), "{ isto nao e json").expect("grava lixo");
    // E um JSON válido que é pergunta INVÁLIDA: pior caso, porque `from_str` passa.
    std::fs::write(
        d.join("q-invalida.json"),
        r#"{"id":"q-invalida","criada_em":1,"titulo":"T","kind":"escolha","opcoes":[]}"#,
    )
    .expect("grava inválida");
    let (ok, ruins) = listar(&tmp);
    assert_eq!(ok.len(), 1, "só a boa entra na fila");
    assert_eq!(ruins.len(), 2, "as duas ruins têm de APARECER: {ruins:?}");
    assert!(ruins.iter().any(|r| r.contains("q-lixo")), "{ruins:?}");
    assert!(ruins.iter().any(|r| r.contains("2 opções ou mais")), "{ruins:?}");
    let _ = std::fs::remove_dir_all(&tmp);
}

/// Entrada hostil não panica — a fila é lida por uma janela, e janela que morre ao abrir é
/// pior que fila vazia.
#[test]
fn entrada_hostil_nao_panica() {
    let d = std::env::temp_dir();
    for lixo in [
        "",
        "null",
        "[]",
        "{}",
        "{\"id\":null}",
        "\u{0}",
        &"[".repeat(2000),
        r#"{"id":"a","criada_em":-1,"titulo":"t","kind":"escolha"}"#,
        r#"{"id":"a","criada_em":1,"titulo":"t","kind":"inventado"}"#,
    ] {
        let f = d.join(format!("ovp-lixo-{}.json", std::process::id()));
        std::fs::write(&f, lixo).unwrap();
        let _ = ler(&f); // só não pode panicar
        let _ = std::fs::remove_file(&f);
    }
}

/// `kind` desconhecido REPROVA em vez de virar `livre` — kind ignorado desenha tela sem botão.
#[test]
fn kind_desconhecido_reprova_em_vez_de_virar_livre() {
    let e = serde_json::from_str::<Pergunta>(
        r#"{"id":"a","criada_em":1,"titulo":"t","kind":"aprovar_talvez"}"#,
    )
    .expect_err("kind inventado tinha de reprovar");
    assert!(e.to_string().contains("aprovar_talvez") || e.to_string().contains("variant"), "{e}");
}

#[test]
fn id_novo_e_estavel_e_nao_colide_no_mesmo_segundo() {
    let tmp = std::env::temp_dir().join(format!(
        "ovp4-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    let a = novo_id(&tmp, 1_790_000_000);
    let mut p = com_opcoes(Kind::Escolha);
    p.id = a.clone();
    gravar(&tmp, &p).expect("grava");
    let b = novo_id(&tmp, 1_790_000_000);
    assert_ne!(a, b, "dois ids no mesmo segundo não podem colidir");
    let _ = std::fs::remove_dir_all(&tmp);
}
