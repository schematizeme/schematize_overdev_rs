//! Integração REAL da jaula: um alocador de ~800 MiB sob teto de 500 MiB morre SÓ ele.
//!
//! Prova: veredito `EstourouMemoria` no `jaulas.jsonl`, o processo de teste (pai) segue vivo e
//! uma segunda unidade concorrente conclui normalmente. Sem backend, PULA com aviso explícito.

use overdev_casa::agentrun::jaulado::{executar_jaulado, ler};
use schematize_plataforma::jaula::{detectar_backend, Backend, Orcamento, Veredito};
use std::path::Path;

/// **O quê:** argv de um `python3` que aloca e TOCA `mib` MiB e dorme. **Onde:** o teste.
fn alocador(mib: u64) -> Vec<String> {
    let src =
        format!("b = b'x' * ({mib} * 1024 * 1024)\nimport time\ntime.sleep(30)\nprint(len(b))");
    vec!["python3".into(), "-c".into(), src]
}

#[test]
fn oom_mata_so_a_unidade_e_o_pai_segue() {
    if detectar_backend() != Backend::SystemdScope {
        eprintln!(
            "PULADO: sem systemd-run --user + cgroup memory delegado; a jaula real não roda aqui"
        );
        return;
    }
    if std::process::Command::new("python3").arg("--version").output().is_err() {
        eprintln!("PULADO: python3 ausente; sem alocador para o teste");
        return;
    }
    let od = std::env::temp_dir().join(format!("sz-jaulas-oom-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&od);
    std::fs::create_dir_all(&od).expect("criar od");
    let proj = Path::new("/tmp/proj-oom");
    let teto = Orcamento::novo(500, None).expect("orcamento");

    let (od2, teto2) = (od.clone(), teto.clone());
    let irma = std::thread::spawn(move || {
        let argv = vec!["sh".to_string(), "-c".into(), "sleep 1; exit 0".into()];
        executar_jaulado(&od2, Path::new("/tmp/proj-oom"), &teto2, &argv, 0)
    });
    let v = executar_jaulado(&od, proj, &teto, &alocador(800), 0);
    assert!(matches!(v, Veredito::EstourouMemoria { teto_mib: 500, .. }), "{v:?}");
    let v2 = irma.join().expect("a unidade concorrente não pode derrubar o teste");
    assert_eq!(v2, Veredito::Concluiu(0), "a 2ª unidade conclui normalmente");

    let (regs, ruins) = ler(&od);
    assert_eq!(ruins, 0);
    assert_eq!(regs.len(), 2, "{regs:?}");
    let morta = regs.iter().find(|r| r.veredito == "oom").expect("registro oom");
    assert_eq!((morta.teto_mib, morta.classe.as_str()), (500, "leve"));
    assert!(morta.unidade.starts_with("schematize-proj-oom-"), "{}", morta.unidade);
    assert!(regs.iter().any(|r| r.veredito == "concluiu"));
    let _ = std::fs::remove_dir_all(&od);
}
