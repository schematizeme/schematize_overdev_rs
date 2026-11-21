//! Testes PUROS do lançamento jaulado (sem systemd): política, nome, jsonl, gerações, wrapper.

use super::*;
use schematize_plataforma::jaula::Classe;

fn tmp(nome: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("sz-jaulado-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("criar tmp");
    d
}

fn oom() -> Veredito {
    Veredito::EstourouMemoria { pico_mib: Some(510), teto_mib: 512 }
}

/// Classe por tipo: agente LLM é Médio (1 GiB), script Leve, build Pesado.
#[test]
fn classe_por_tipo() {
    assert_eq!(classe_do_lancamento(TipoExecucao::AgenteLlm), Classe::Medio);
    assert_eq!(classe_do_lancamento(TipoExecucao::Script), Classe::Leve);
    assert_eq!(classe_do_lancamento(TipoExecucao::Build), Classe::Pesado);
    assert_eq!(orcamento_para(TipoExecucao::AgenteLlm).mem_max_mib, 1024);
    assert_eq!(classe_do_teto(500), Classe::Leve);
    assert_eq!(classe_do_teto(3000), Classe::Pesado);
}

/// Nome de unidade hostil nunca vira opção, caminho ou shell.
#[test]
fn nome_de_unidade_hostil() {
    let n = nome_da_unidade("../; rm -rf $(x)\0 ü", "1-2");
    assert!(n.starts_with("schematize-"), "{n}");
    assert!(n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'), "{n}");
    assert!(n.len() <= 64);
    assert_eq!(nome_da_unidade("Meu App", "7-0"), "schematize-meu-app-7-0");
    assert_eq!(slug_do_projeto("///"), "proj");
}

/// Política: OOM e Timeout re-decompõem 3 vezes e depois parkeiam; Falhou segue a política atual.
#[test]
fn reacao_limitada_a_tres_geracoes() {
    assert_eq!(reagir(&oom(), 0), ReacaoAoVeredito::Redecompor { geracao: 1 });
    assert_eq!(reagir(&Veredito::Timeout, 2), ReacaoAoVeredito::Redecompor { geracao: 3 });
    assert_eq!(reagir(&oom(), 3), ReacaoAoVeredito::Parkear);
    assert_eq!(reagir(&Veredito::Falhou(1), 0), ReacaoAoVeredito::PoliticaAtual);
    assert_eq!(reagir(&Veredito::Concluiu(0), 5), ReacaoAoVeredito::Seguir);
}

/// SemBackend recusa com mensagem acionável e sem culpa, citando o opt-in explícito.
#[test]
fn sem_backend_recusa_acionavel() {
    let ReacaoAoVeredito::Recusar(m) = reagir(&Veredito::SemBackend("falta X.".into()), 0) else {
        panic!("esperava Recusar");
    };
    assert!(m.contains("Não consegui criar a jaula") && m.contains("falta X."), "{m}");
    assert!(m.contains(ENV_SEM_JAULA) && m.contains("Eu preparo"), "{m}");
    assert!(
        sem_jaula_optado(Some("1")) && !sem_jaula_optado(Some("true")) && !sem_jaula_optado(None)
    );
}

/// jsonl: uma linha JSON por unidade, append, linha corrompida é contada e não derruba a leitura.
#[test]
fn jsonl_append_e_leitura() {
    let od = tmp("jsonl");
    let r = registro_de("schematize-p-1-0", "medio", 1024, 0, &oom(), 99);
    anexar(&od, &r).expect("anexar 1");
    anexar(&od, &registro_de("schematize-p-1-1", "leve", 512, 0, &Veredito::Concluiu(0), 100))
        .expect("anexar 2");
    let bruto = std::fs::read_to_string(caminho(&od)).expect("ler");
    assert_eq!(bruto.lines().count(), 2);
    let v: serde_json::Value =
        serde_json::from_str(bruto.lines().next().expect("l1")).expect("json");
    assert_eq!(v["veredito"], "oom");
    assert_eq!(v["pico_mib"], 510);
    assert_eq!(v["teto_mib"], 1024);
    std::fs::write(caminho(&od), format!("{bruto}{{lixo\n")).expect("corromper");
    let (regs, ruins) = ler(&od);
    assert_eq!((regs.len(), ruins), (2, 1));
    let _ = std::fs::remove_dir_all(&od);
}

/// Gerações: mortes seguidas contam; `concluiu`/`parkeado` zeram; falha comum e aviso são neutros.
#[test]
fn geracao_conta_mortes_seguidas() {
    let mk = |v: &str| Registro {
        unidade: "u".into(),
        classe: "medio".into(),
        teto_mib: 1024,
        pico_mib: None,
        veredito: v.into(),
        geracao: 0,
        timestamp: 0,
        aviso: None,
    };
    assert_eq!(geracao_atual(&[]), 0);
    assert_eq!(geracao_atual(&[mk("oom"), mk("falhou"), mk("timeout"), mk("aviso")]), 2);
    assert_eq!(geracao_atual(&[mk("oom"), mk("concluiu"), mk("oom")]), 1);
    assert_eq!(geracao_atual(&[mk("oom"), mk("oom"), mk("parkeado")]), 0);
}

/// O prompt de relançamento: 1ª a 3ª morte re-decompõem; a 4ª parkeia e zera a contagem.
#[test]
fn relancamento_redecompoe_ate_tres_e_parkeia() {
    let od = tmp("ger");
    assert!(prompt_de_relancamento(&od).is_none(), "sem histórico: retomada comum");
    for n in 1..=3u32 {
        anexar(&od, &registro_de(&format!("u{n}"), "medio", 1024, n - 1, &oom(), n.into()))
            .expect("a");
        let p = prompt_de_relancamento(&od).expect("prompt");
        assert!(p.contains("MORTA-POR-OOM") && p.contains(&format!("geração {n}/3")), "{p}");
        assert!(p.contains("NÃO reexecute igual"), "{p}");
    }
    anexar(&od, &registro_de("u4", "medio", 1024, 3, &oom(), 4)).expect("a4");
    let p = prompt_de_relancamento(&od).expect("parkear");
    assert!(p.contains("- [~]"), "{p}");
    assert_eq!(geracao_atual(&ler(&od).0), 0, "marcador parkeado zera a contagem");
    assert!(prompt_de_relancamento(&od).is_none(), "depois de parkear volta à retomada comum");
    let _ = std::fs::remove_dir_all(&od);
}

/// `.exit` -> veredito: `oom-kill` ou SIGKILL perto do teto é estouro; código comum é falha/sucesso.
#[test]
fn exit_vira_veredito() {
    assert!(matches!(
        veredito_do_exit("137 oom-kill\n", 1024),
        Some(Veredito::EstourouMemoria { .. })
    ));
    assert_eq!(veredito_do_exit("0 success\n", 1024), Some(Veredito::Concluiu(0)));
    assert_eq!(veredito_do_exit("2 exit-code\n", 1024), Some(Veredito::Falhou(2)));
    assert_eq!(veredito_do_exit("lixo", 1024), None);
}

/// `apurar` casa `.exit` com `.pendente`, grava no jsonl e limpa; unidade morta não afeta a outra.
#[test]
fn apurar_registra_e_limpa() {
    let od = tmp("apurar");
    for (u, exit) in [("schematize-p-1-0", "137 oom-kill"), ("schematize-p-1-1", "0 success")] {
        let pend =
            Pendente { unidade: u.into(), classe: "medio".into(), teto_mib: 1024, geracao: 0 };
        std::fs::write(
            od.join(format!("jaula-{u}.pendente")),
            serde_json::to_string(&pend).expect("j"),
        )
        .expect("pend");
        std::fs::write(od.join(format!("jaula-{u}.exit")), exit).expect("exit");
    }
    let mut novos = apurar(&od);
    novos.sort_by(|a, b| a.unidade.cmp(&b.unidade));
    assert_eq!(novos.len(), 2);
    assert_eq!((novos[0].veredito.as_str(), novos[1].veredito.as_str()), ("oom", "concluiu"));
    assert!(apurar(&od).is_empty(), "idempotente: nada pendente sobra");
    assert_eq!(ler(&od).0.len(), 2);
    let _ = std::fs::remove_dir_all(&od);
}

/// `apurar_com`: o callback roda ANTES de gravar; recusando, nada é gravado nem removido.
#[test]
fn apurar_com_recusa_deixa_para_a_proxima_ronda() {
    let od = tmp("apurar-com");
    let u = "schematize-p-9-0";
    let pend = Pendente { unidade: u.into(), classe: "medio".into(), teto_mib: 1024, geracao: 0 };
    std::fs::write(od.join(format!("jaula-{u}.pendente")), serde_json::to_string(&pend).expect("j"))
        .expect("pend");
    std::fs::write(od.join(format!("jaula-{u}.exit")), "137 oom-kill").expect("exit");
    let mut vistos = 0;
    assert!(apurar_com(&od, &mut |_| {
        vistos += 1;
        false
    })
    .is_empty());
    assert_eq!((vistos, ler(&od).0.len()), (1, 0), "recusado: nada gravado");
    assert_eq!(apurar_com(&od, &mut |_| true).len(), 1, "na próxima ronda entra");
    let _ = std::fs::remove_dir_all(&od);
}

/// Wrapper: sem `--collect`, argumentos entre aspas, prompt expandido, unidade `schematize-<proj>-<seq>`.
#[test]
fn wrapper_jaulado_monta_o_comando() {
    let od = tmp("wrap");
    let proj = Path::new("/p/Meu App");
    let corpo = corpo_com(
        Backend::SystemdScope,
        false,
        &od,
        proj,
        Path::new("/bin/claude"),
        "so'net",
        &od.join("p.txt"),
    )
    .expect("ok")
    .expect("jaulado");
    assert!(corpo.contains("'systemd-run' '--user' '--scope'"), "{corpo}");
    assert!(!corpo.contains("--collect"), "{corpo}");
    assert!(corpo.contains("'--unit=schematize-meu-app-"), "{corpo}");
    assert!(corpo.contains("'MemoryMax=1024M'") && corpo.contains("'OOMPolicy=kill'"), "{corpo}");
    assert!(corpo.contains("'so'\\''net'"), "modelo hostil escapado: {corpo}");
    assert!(corpo.contains("\"$(cat '") && corpo.contains("Result --value"), "{corpo}");
    assert!(std::fs::read_dir(&od)
        .expect("dir")
        .flatten()
        .any(|e| e.file_name().to_string_lossy().ends_with(".pendente")));
    let _ = std::fs::remove_dir_all(&od);
}

/// Sem backend: recusa acionável; com opt-in explícito: corpo legado + aviso no jsonl.
#[test]
fn sem_backend_recusa_ou_avisa() {
    let od = tmp("semb");
    let args = (Path::new("/p"), Path::new("/bin/claude"), "m", Path::new("/x"));
    let e =
        corpo_com(Backend::Nenhum, false, &od, args.0, args.1, args.2, args.3).expect_err("recusa");
    assert!(e.contains("Não consegui criar a jaula"), "{e}");
    assert!(ler(&od).0.is_empty(), "nada rodou nem foi registrado");
    let ok =
        corpo_com(Backend::Prlimit, true, &od, args.0, args.1, args.2, args.3).expect("opt-in");
    assert!(ok.is_none());
    let (regs, _) = ler(&od);
    assert_eq!(regs.len(), 1);
    assert_eq!(regs[0].veredito, "aviso");
    assert!(regs[0].aviso.as_deref().unwrap_or("").contains(ENV_SEM_JAULA));
    let _ = std::fs::remove_dir_all(&od);
}
