//! Testes do checklist particionado (ADR-0027). Tudo em diretório temporário, sem efeito externo.

use super::estado::{Estado, INDICE, PASTA};
use super::nome::{parse, slugar, Nome};
use super::secoes::eh_item;
use super::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const FIXTURE: &str = include_str!("../../../tests/fixtures/CHECKLIST_221.md");

/// **O quê:** raiz de projeto temporária limpa + dir do overdev dentro. **Onde:** todo teste.
fn projeto(nome: &str) -> (PathBuf, PathBuf) {
    let raiz = std::env::temp_dir().join(format!("ovd-part-{}-{nome}", std::process::id()));
    let _ = std::fs::remove_dir_all(&raiz);
    let od = raiz.join(".schematize/overdev");
    std::fs::create_dir_all(od.join(PASTA)).expect("criar dir");
    (raiz, od)
}

/// **O quê:** `n` itens `- [x]`/`- [ ]` numerados. **Onde:** montagem de partições de teste.
fn itens(n: usize, marca: &str, pref: &str) -> String {
    (0..n).map(|i| format!("- [{marca}] {pref}{i}\n")).collect()
}

/// **O quê:** grava uma partição de teste. **Onde:** os testes de sync/check/proxima.
fn grava(od: &Path, nome: &str, corpo: &str) {
    std::fs::write(od.join(PASTA).join(nome), corpo).expect("gravar partição");
}

/// **O quê:** multiset de linhas (contagem por linha). **Onde:** provas de integridade.
fn multiset<'a>(it: impl Iterator<Item = &'a str>) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for l in it {
        *m.entry(l.to_string()).or_insert(0) += 1;
    }
    m
}

/// **O quê:** todo o conteúdo de `checklist/` + índice, por nome. **Onde:** idempotência.
fn foto(od: &Path) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    m.insert(INDICE.to_string(), std::fs::read_to_string(od.join(INDICE)).unwrap_or_default());
    for e in std::fs::read_dir(od.join(PASTA)).expect("ler dir").flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        m.insert(n, std::fs::read_to_string(e.path()).unwrap_or_default());
    }
    m
}

#[test]
fn nome_valido_roundtrip() {
    let n = parse("p0-001-auth-login.md").expect("válido");
    assert_eq!(n, Nome { prio: 0, id: 1, slug: "auth-login".into(), concluida: false });
    assert_eq!(n.formatar(), "p0-001-auth-login.md");
    let c = parse("p3-999-x--completed.md").expect("válido");
    assert!(c.concluida && c.slug == "x" && c.id == 999);
    assert_eq!(c.formatar(), "p3-999-x--completed.md");
    assert_eq!(c.com_conclusao(false).formatar(), "p3-999-x.md");
}

#[test]
fn nome_hostil_e_rejeitado_sem_panico() {
    let ruins = [
        "p4-001-x.md",    // prioridade fora de 0..=3
        "p0-01-x.md",     // NNN com 2 dígitos
        "p0-0001-x.md",   // NNN com 4 dígitos
        "p0-001-x",       // sem .md
        "p0-001-x.MD",    // extensão em caixa errada
        "p0-001-a--b.md", // `--` no slug
        "p0-001-.md",     // slug vazio
        "p0-001--x.md",   // slug começa com hífen
        "p0-001-x-.md",   // slug termina com hífen
        "p0-001-Auth.md", // maiúscula
        "p0-001-açaí.md", // unicode
        "p0-001-ａ.md",   // fullwidth
        "p0-001-x--completed--completed.md",
        "p0-001-x--completed-.md",
        "p0-001-x_y.md", // underscore
        "P0-001-x.md",
        "p0-00a-x.md",
        "p-001-x.md",
        "",
        ".md",
        "p0-001-x\0.md",
        "p0-001-x.md.bak",
        "p0-001-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.md", // slug > 40
        "--completed.md",
    ];
    for r in ruins {
        assert!(parse(r).is_none(), "deveria rejeitar {r:?}");
    }
    assert!(parse("p0-001-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.md").is_some(), "40 chars ok");
}

#[test]
fn slugar_normaliza_acentos_e_limita() {
    assert_eq!(slugar("F2 · Market: a janela vira mercado", 40), "f2-market-a-janela-vira-mercado");
    assert_eq!(slugar("Ação — não é ÇÃO", 40), "acao-nao-e-cao");
    assert_eq!(slugar("???", 40), "geral");
    let s = slugar(&"abc ".repeat(30), 40);
    assert!(s.len() <= 40 && !s.ends_with('-') && parse(&format!("p2-001-{s}.md")).is_some());
}

#[test]
fn particionar_221_itens_reais_sem_perder_nem_duplicar() {
    let (_r, od) = projeto("fixture");
    std::fs::write(od.join(INDICE), FIXTURE).unwrap();
    let antes = Estado::de(FIXTURE).total();
    assert_eq!(antes, 221, "a fixture tem os 221 itens reais");

    let rel = particionar(&od, &Opcoes::default()).expect("particionar");
    assert_eq!(rel.itens, 221);
    assert_eq!(std::fs::read_to_string(rel.backup.expect("backup")).unwrap(), FIXTURE);

    let l = listar(&od).expect("listar");
    assert!(l.invalidos.is_empty(), "todo nome gerado é válido: {:?}", l.invalidos);
    let soma: usize = l.validas.iter().map(|p| p.estado.total()).sum();
    assert_eq!(soma, antes, "soma de itens antes == depois");
    assert!(l.validas.iter().all(|p| p.estado.total() <= 50), "nenhuma partição > 50");

    let indice = std::fs::read_to_string(od.join(INDICE)).unwrap();
    assert!(!indice.lines().any(|x| x.trim_start().starts_with("- [")), "índice sem `- [`");
    assert_eq!(Estado::de(&indice).total(), 0);

    // Multiset EXATO das linhas de item (byte a byte): nenhuma perdida, nenhuma duplicada.
    let textos: Vec<String> =
        l.validas.iter().map(|p| std::fs::read_to_string(&p.caminho).unwrap()).collect();
    let depois = multiset(textos.iter().flat_map(|t| t.split('\n')).filter(|x| eh_item(x)));
    let orig = multiset(FIXTURE.split('\n').filter(|x| eh_item(x)));
    assert_eq!(depois, orig, "multiset de linhas de item idêntico");

    // Nenhuma linha NÃO vazia do original some (itens, cabeçalhos, prosa).
    let tudo = multiset(
        textos.iter().map(String::as_str).chain([indice.as_str()]).flat_map(|t| t.split('\n')),
    );
    for (linha, n) in multiset(FIXTURE.split('\n').filter(|x| !x.trim().is_empty())) {
        assert!(tudo.get(&linha).copied().unwrap_or(0) >= n, "linha perdida: {linha:?}");
    }
    assert!(checar(&od).expect("check").erros.is_empty());
}

#[test]
fn particionar_e_idempotente() {
    let (_r, od) = projeto("idem");
    std::fs::write(od.join(INDICE), FIXTURE).unwrap();
    particionar(&od, &Opcoes::default()).expect("1ª");
    let a = foto(&od);
    let rel = particionar(&od, &Opcoes::default()).expect("2ª");
    assert!(rel.criadas.is_empty() && rel.backup.is_none());
    assert_eq!(foto(&od), a, "rodar 2x = mesmo resultado");
    sync(&od).expect("sync");
    assert_eq!(foto(&od), a, "sync depois também não muda nada");
}

#[test]
fn particionar_nao_renumera_e_usa_proximo_id() {
    let (_r, od) = projeto("ids");
    std::fs::write(od.join(INDICE), FIXTURE).unwrap();
    particionar(&od, &Opcoes::default()).unwrap();
    let ids_antes: Vec<u16> = listar(&od).unwrap().validas.iter().map(|p| p.nome.id).collect();
    // Agente joga itens novos no índice; a nova rodada cria só partições novas, ids max+1.
    let mut t = std::fs::read_to_string(od.join(INDICE)).unwrap();
    t.push_str(&format!("\n## [p0] Novo bloqueante\n{}", itens(5, " ", "novo")));
    std::fs::write(od.join(INDICE), t).unwrap();
    particionar(&od, &Opcoes::default()).unwrap();
    let l = listar(&od).unwrap();
    let novos: Vec<_> = l.validas.iter().filter(|p| !ids_antes.contains(&p.nome.id)).collect();
    assert_eq!(novos.len(), 1);
    assert_eq!(novos[0].nome.id, ids_antes.iter().max().unwrap() + 1);
    assert_eq!(novos[0].nome.prio, 0, "tag [p0] do título manda na prioridade");
    assert_eq!(
        l.validas.iter().filter(|p| ids_antes.contains(&p.nome.id)).count(),
        ids_antes.len()
    );
}

#[test]
fn secao_grande_e_fatiada_por_h3_e_respeita_o_teto() {
    let (_r, od) = projeto("h3");
    let mut t = String::from("# t\n\n## [p1] Fase grande\n");
    for g in 0..4 {
        t.push_str(&format!("### grupo {g}\n{}\n", itens(30, " ", &format!("g{g}-"))));
    }
    std::fs::write(od.join(INDICE), &t).unwrap();
    particionar(&od, &Opcoes::default()).unwrap();
    let l = listar(&od).unwrap();
    assert!(l.validas.len() >= 3);
    for p in &l.validas {
        assert!(p.estado.total() <= 50 && p.nome.prio == 1);
        let txt = std::fs::read_to_string(&p.caminho).unwrap();
        // corte nas fronteiras ###: um grupo nunca é partido ao meio aqui (30 <= 50)
        for g in 0..4 {
            let n = txt.lines().filter(|x| x.contains(&format!("g{g}-"))).count();
            assert!(n == 0 || n == 30, "grupo {g} partido: {n}");
        }
    }
    let soma: usize = l.validas.iter().map(|p| p.estado.total()).sum();
    assert_eq!(soma, 120);
    let dep: Vec<_> = l.validas.iter().filter(|p| !p.depende.is_empty()).collect();
    assert_eq!(dep.len(), l.validas.len() - 1, "fatias 2+ dependem da anterior");
}

#[test]
fn secao_sem_h3_acima_de_50_e_fatiada_equilibrada_e_preserva_crlf() {
    let (_r, od) = projeto("crlf");
    let corpo: String = (0..51).map(|i| format!("- [ ] item {i}\r\n")).collect();
    std::fs::write(od.join(INDICE), format!("## S\r\n{corpo}")).unwrap();
    particionar(&od, &Opcoes::default()).unwrap();
    let l = listar(&od).unwrap();
    let tams: Vec<usize> = l.validas.iter().map(|p| p.estado.total()).collect();
    assert_eq!(tams.iter().sum::<usize>(), 51);
    assert!(tams.iter().all(|t| *t <= 50) && tams.len() == 2, "{tams:?}");
    let todo: String =
        l.validas.iter().map(|p| std::fs::read_to_string(&p.caminho).unwrap()).collect();
    assert_eq!(todo.matches("item 7\r\n").count(), 1, "\\r preservado byte a byte");
}

#[test]
fn sync_renomeia_nas_duas_direcoes_e_regenera_indice() {
    let (_r, od) = projeto("sync");
    grava(&od, "p1-001-a.md", &itens(26, "x", "a"));
    grava(
        &od,
        "p2-002-b--completed.md",
        &format!("{}{}", itens(25, "x", "b"), itens(1, " ", "reaberto")),
    );
    grava(&od, "p2-003-c.md", &format!("{}- [H x] humano\n- [-] cancelado\n", itens(25, "x", "c")));
    grava(&od, "p2-004-d.md", &format!("{}- [~] preso\n", itens(25, "x", "d")));
    let m = sync(&od).expect("sync");
    let mut ren = m.renomeados.clone();
    ren.sort();
    assert_eq!(
        ren,
        vec![
            ("p1-001-a.md".into(), "p1-001-a--completed.md".into()),
            ("p2-002-b--completed.md".into(), "p2-002-b.md".into()),
            ("p2-003-c.md".into(), "p2-003-c--completed.md".into()),
        ]
    );
    assert!(od.join(PASTA).join("p2-004-d.md").is_file(), "on-hold mantém aberta");
    let idx = std::fs::read_to_string(od.join(INDICE)).unwrap();
    assert!(idx.contains("| partição | prio | abertos | total | depende | estado |"));
    assert!(
        idx.contains("| p1 | 0 | 26 | - | concluída |")
            || idx.contains("p1 | 0 | 26 | - | concluída")
    );
    assert!(!idx.lines().any(|l| l.trim_start().starts_with("- [")));
    let a = foto(&od);
    assert_eq!(sync(&od).unwrap(), Mudancas::default(), "idempotente");
    assert_eq!(foto(&od), a);
}

#[test]
fn sync_nao_toca_o_monolito_com_itens() {
    let (_r, od) = projeto("mono");
    grava(&od, "p2-001-a.md", &itens(3, " ", "a"));
    std::fs::write(od.join(INDICE), "- [ ] solto\n").unwrap();
    let m = sync(&od).unwrap();
    assert!(m.monolito && !m.indice_atualizado);
    assert_eq!(std::fs::read_to_string(od.join(INDICE)).unwrap(), "- [ ] solto\n");
}

/// Cada regra do `check` reprova sozinha (self-check: o lint não está cego).
#[test]
fn check_reprova_cada_violacao() {
    type Caso<'a> = (&'a str, Vec<(&'a str, String)>, &'a str);
    let casos: Vec<Caso> = vec![
        ("nome", vec![("foo.md", itens(30, "x", "a"))], "nome inválido"),
        ("max", vec![("p2-001-a.md", itens(51, " ", "a"))], "máximo 50"),
        ("completed", vec![("p2-001-a--completed.md", itens(30, " ", "a"))], "--completed"),
        (
            "dup",
            vec![("p2-001-a.md", itens(30, " ", "a")), ("p1-001-b.md", itens(30, " ", "b"))],
            "duplicado",
        ),
        (
            "depende",
            vec![(
                "p2-001-a.md",
                format!("> fase: F · depende: 009 · plano: -\n{}", itens(30, " ", "a")),
            )],
            "não existe",
        ),
    ];
    for (id, arquivos, esperado) in casos {
        let (_r, od) = projeto(&format!("chk-{id}"));
        for (n, c) in &arquivos {
            grava(&od, n, c);
        }
        let r = checar(&od).expect("checar");
        assert!(r.erros.iter().any(|e| e.contains(esperado)), "{id}: {:?}", r.erros);
    }
    // Controle negativo: o caso limpo passa sem erro.
    let (_r, od) = projeto("chk-ok");
    grava(&od, "p2-001-a.md", &itens(30, " ", "a"));
    assert!(checar(&od).unwrap().erros.is_empty());
}

#[test]
fn check_avisa_pequena_e_monolito_legado_sem_reprovar() {
    let (_r, od) = projeto("avisos");
    grava(&od, "p2-001-a.md", &itens(5, " ", "a"));
    grava(&od, "part-01.md", &itens(3, " ", "legado"));
    std::fs::write(od.join(INDICE), itens(60, "x", "m")).unwrap();
    let r = checar(&od).unwrap();
    assert!(r.erros.is_empty(), "{:?}", r.erros);
    assert!(r.avisos.iter().any(|a| a.contains("< 25")));
    assert!(r.avisos.iter().any(|a| a.contains("monolítico") && a.contains("particionar")));
    assert!(r.avisos.iter().any(|a| a.contains("part-01.md")));
}

#[test]
fn proxima_respeita_prioridade_ordem_e_depende() {
    let (_r, od) = projeto("prox");
    grava(&od, "p2-001-a.md", &itens(2, " ", "a"));
    grava(&od, "p0-002-b--completed.md", &itens(2, "x", "b"));
    grava(
        &od,
        "p1-003-c.md",
        &format!("> fase: - · depende: 001 · plano: -\n{}", itens(2, " ", "c")),
    );
    grava(&od, "p1-004-d.md", &itens(2, " ", "d"));
    let nome = |p: Option<PathBuf>| p.map(|p| p.file_name().unwrap().to_string_lossy().to_string());
    // p1-003 vem antes por (pN,NNN) mas depende de 001 (aberta): pula para p1-004.
    assert_eq!(nome(proxima(&od).unwrap()).as_deref(), Some("p1-004-d.md"));
    std::fs::write(od.join(PASTA).join("p1-004-d.md"), itens(2, "x", "d")).unwrap();
    sync(&od).unwrap();
    assert_eq!(
        nome(proxima(&od).unwrap()).as_deref(),
        Some("p2-001-a.md"),
        "003 bloqueada por 001"
    );
    std::fs::write(od.join(PASTA).join("p2-001-a.md"), itens(2, "x", "a")).unwrap();
    sync(&od).unwrap();
    assert_eq!(nome(proxima(&od).unwrap()).as_deref(), Some("p1-003-c.md"));
    std::fs::write(od.join(PASTA).join("p1-003-c.md"), itens(2, "x", "c")).unwrap();
    sync(&od).unwrap();
    assert_eq!(proxima(&od).unwrap(), None, "tudo concluído");
}

#[test]
fn afazeres_ignoram_completed_mas_o_total_inclui() {
    let (raiz, od) = projeto("leitura");
    grava(&od, "p2-001-a--completed.md", &itens(4, "x", "feito"));
    grava(&od, "p2-002-b.md", &format!("{}{}", itens(2, " ", "aberto"), itens(1, "x", "ok")));
    let abertos = crate::paths::read_multidoc_abertos(&od, "CHECKLIST.md", "checklist");
    let todos = crate::paths::read_multidoc(&od, "CHECKLIST.md", "checklist");
    assert!(!abertos.contains("feito0") && abertos.contains("aberto0"));
    assert!(todos.contains("feito0"));
    let p = crate::overdev::progress_at(&raiz);
    assert_eq!((p.open, p.done), (2, 5), "progresso conta o histórico inteiro");
    assert_eq!(crate::overdev::open_items_at(&raiz, 10).len(), 2);
    // O item aberto de uma --completed some dos afazeres (por isso o Stop-hook checa a integridade).
    grava(&od, "p2-003-c--completed.md", &itens(1, " ", "fantasma"));
    assert!(!crate::overdev::open_items_at(&raiz, 10).iter().any(|i| i.contains("fantasma")));
    let l = listar(&od).unwrap();
    assert_eq!(integridade(&l).len(), 1);
    assert!(integridade(&l)[0].contains("reabra"));
}

#[test]
fn split_round_robin_recusa_checklist_particionado() {
    let (raiz, od) = projeto("split");
    grava(&od, "p2-001-a.md", &itens(4, " ", "a"));
    let e = crate::overdev::split(&raiz, 2).expect_err("deve recusar");
    assert!(e.contains("particionado"), "{e}");
    let _ = od;
}

#[test]
fn migra_arquivos_legados_do_split() {
    let (_r, od) = projeto("legado");
    std::fs::write(od.join(INDICE), "# t\n").unwrap();
    grava(&od, "part-01.md", &format!("# Split 1/2\n{}", itens(3, " ", "p1-")));
    grava(&od, "part-02.md", &itens(2, "x", "p2-"));
    let rel = particionar(&od, &Opcoes { max: 50, min: 1 }).unwrap();
    assert_eq!(rel.itens, 5);
    assert!(od.join(PASTA).join("legado/part-01.md").is_file());
    let l = listar(&od).unwrap();
    assert!(l.invalidos.is_empty());
    assert_eq!(l.validas.iter().map(|p| p.estado.total()).sum::<usize>(), 5);
}

#[test]
fn parse_depende_tolera_lixo() {
    use super::estado::parse_depende;
    assert_eq!(parse_depende("> fase: F2 · depende: 001, 004 · plano: x"), vec![1, 4]);
    assert!(parse_depende("> fase: F2 · depende: - · plano: x").is_empty());
    assert!(parse_depende("sem cabeçalho").is_empty());
    assert!(parse_depende("> depende: \u{0}zz, 12345 ·").is_empty());
}
