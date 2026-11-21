//! Testes do despacho: invólucro com portão, resfile, expansão de prompt e adoção.

use super::*;

/// Pedido de teste com o comando `argv` e arquivos sob `dir`. **Onde:** testes deste arquivo.
fn pedido(dir: &Path, argv: &[&str]) -> Pedido {
    Pedido {
        id: "u".into(),
        unidade: String::new(),
        resfile: dir.join("u.res"),
        log: dir.join("u.log"),
        argv: argv.iter().map(|s| s.to_string()).collect(),
        orcamento: None,
        timeout: None,
        env: vec![],
    }
}

/// Dir temporário único. **Onde:** testes.
fn tmp(nome: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("od-desp-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Espera o fim do filho (até 10 s). **Onde:** testes.
fn espera(f: &mut Box<dyn Filho>) -> Fim {
    for _ in 0..500 {
        if let Some(fim) = f.tentar_esperar() {
            return fim;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("a unidade não terminou");
}

/// O comando só roda DEPOIS do portão; o resfile guarda o código.
#[test]
fn portao_segura_o_efeito_ate_liberar() {
    let d = tmp("portao");
    let marca = d.join("rodou");
    let cmd = format!("touch {}", marca.display());
    let mut f = DespachanteProcesso.lancar(&pedido(&d, &["sh", "-c", &cmd])).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    assert!(!marca.exists(), "o efeito rodou antes do portão");
    f.liberar().unwrap();
    assert_eq!(espera(&mut f), Fim::Saida(Saida { veredito: Veredito::Concluiu, pico_mib: None }));
    assert!(marca.exists());
    assert_eq!(ler_codigo(&d.join("u.res")), Some(0));
    std::fs::remove_dir_all(&d).unwrap();
}

/// Orquestrador que morre antes de liberar: o invólucro sai sem executar nada.
#[test]
fn sem_liberar_o_filho_sai_sem_efeito() {
    let d = tmp("semlib");
    let marca = d.join("rodou");
    let cmd = format!("touch {}", marca.display());
    let f = DespachanteProcesso.lancar(&pedido(&d, &["sh", "-c", &cmd])).unwrap();
    // Simula a morte do pai: descartar o filho fecha o stdin (portão) SEM escrever nada.
    drop(f);
    std::thread::sleep(Duration::from_millis(300));
    assert!(!marca.exists(), "sem portão aberto não pode haver efeito");
    std::fs::remove_dir_all(&d).unwrap();
}

/// Código de saída diferente de zero vira `Falhou`.
#[test]
fn falha_vira_veredito_falhou() {
    let d = tmp("falha");
    let mut f = DespachanteProcesso.lancar(&pedido(&d, &["sh", "-c", "exit 7"])).unwrap();
    f.liberar().unwrap();
    let Fim::Saida(s) = espera(&mut f) else { panic!("esperava saída") };
    assert_eq!(s.veredito, Veredito::Falhou);
    assert_eq!(ler_codigo(&d.join("u.res")), Some(7));
    std::fs::remove_dir_all(&d).unwrap();
}

/// Timeout mata só a unidade e vira `Timeout`.
#[test]
fn timeout_mata_so_a_unidade() {
    let d = tmp("timeout");
    let mut p = pedido(&d, &["sleep", "30"]);
    p.timeout = Some(Duration::from_millis(100));
    let mut f = DespachanteProcesso.lancar(&p).unwrap();
    f.liberar().unwrap();
    let Fim::Saida(s) = espera(&mut f) else { panic!("esperava saída") };
    assert_eq!(s.veredito, Veredito::Timeout);
    std::fs::remove_dir_all(&d).unwrap();
}

/// Expansão: argv vazio = `claude -p` com prefixo+brief; placeholders são trocados.
#[test]
fn expande_prompt_com_prefixo_estavel() {
    let v = expandir(&[], "i", "sonnet", "PREFIXO", "BRIEF");
    assert_eq!(v[0..3], ["claude", "-p", "--output-format"]);
    assert!(v.contains(&"sonnet".to_string()));
    assert_eq!(v.last().unwrap(), "PREFIXO\n\nBRIEF");
    let a = expandir(&["x".into(), "{id}-{modelo}-{brief}-{prefixo}".into()], "i", "m", "P", "B");
    assert_eq!(a, vec!["x", "i-m-B-P"]);
    assert_eq!(montar_prompt("", "só brief"), "só brief");
}

/// Adoção: processo morto com resfile = resultado sem re-executar; sem resfile = perdida.
#[test]
fn adotado_le_resfile_ou_declara_perdida() {
    let d = tmp("adota");
    let res = d.join("a.res");
    std::fs::write(&res, "0\n").unwrap();
    let mut a: Box<dyn Filho> =
        Box::new(FilhoAdotado::novo(u32::MAX - 3, 1, res.clone(), String::new(), 0));
    assert_eq!(
        a.tentar_esperar(),
        Some(Fim::Saida(Saida { veredito: Veredito::Concluiu, pico_mib: None }))
    );
    std::fs::remove_file(&res).unwrap();
    assert!(matches!(a.tentar_esperar(), Some(Fim::Perdida(_))));
    std::fs::remove_dir_all(&d).unwrap();
}
