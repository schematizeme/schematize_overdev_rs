//! Testes severos de integridade do handoff vivo: kill -9 real, truncamento, bit-flip,
//! permissão, disco cheio, dois escritores. Seed registrada (`HV_SEED` reproduz).

mod handoff_vivo_comum;

use handoff_vivo_comum::*;
use overdev_casa::handoff_vivo::atomico::{ler_validado, limpar_tmp, Armazem};
use overdev_casa::handoff_vivo::ciclo::{relogio_real, Motor};
use overdev_casa::handoff_vivo::trava::Trava;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Corpo coerente de `n` linhas, todas com a mesma etiqueta (mistura de gravações seria visível).
fn corpo(etiqueta: u64, linhas: usize) -> Vec<u8> {
    let mut v = Vec::new();
    for i in 0..linhas {
        v.extend_from_slice(
            format!("{etiqueta:012} linha {i} preenchimento-preenchimento\n").as_bytes(),
        );
    }
    v
}

/// Todas as linhas começam com a mesma etiqueta?
fn coerente(c: &[u8]) -> bool {
    let s = String::from_utf8_lossy(c);
    let mut it = s.lines().map(|l| l.get(..12).unwrap_or("?"));
    let Some(p) = it.next() else { return true };
    it.all(|x| x == p)
}

/// Filho: modos `escritor` (grava sem parar), `cheio` (RLIMIT_FSIZE) e `segurar` (segura o lock).
#[test]
fn filho_hv() {
    let Ok(modo) = std::env::var("HV_MODO") else { return };
    let dir = std::path::PathBuf::from(std::env::var("HV_DIR").expect("HV_DIR"));
    match modo.as_str() {
        "escritor" => filho_escritor(&dir),
        "cheio" => filho_cheio(&dir),
        "segurar" => {
            let _t = Trava::adquirir(&dir.join("escritor.lock")).expect("lock");
            println!("READY");
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
        outro => panic!("modo {outro}"),
    }
    std::process::exit(0);
}

fn filho_escritor(dir: &Path) -> ! {
    let mut rng = Rng(std::env::var("HV_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(7) | 1);
    let armazens = [
        Armazem::novo(dir, "ULTIMO-context.md"),
        Armazem::novo(dir, "ULTIMO-checklist.md"),
        Armazem::novo(dir, "estado.hv"),
    ];
    let mut n = 0u64;
    loop {
        n += 1;
        for a in &armazens {
            let linhas = 1 + rng.ate(2500) as usize;
            if let Err(e) = a.gravar(&corpo(n, linhas)) {
                eprintln!("erro de gravação no filho: {e}");
                std::process::exit(3);
            }
        }
        if n == 1 {
            println!("READY");
        }
    }
}

fn filho_cheio(dir: &Path) {
    let lim = libc::rlimit { rlim_cur: 6000, rlim_max: 6000 };
    // SAFETY: setrlimit/signal no processo filho de teste, antes de qualquer escrita grande.
    unsafe {
        libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
        assert_eq!(libc::setrlimit(libc::RLIMIT_FSIZE, &lim), 0);
    }
    let a = Armazem::novo(dir, "ULTIMO-context.md");
    a.gravar(&corpo(1, 5)).expect("pequena cabe");
    let r = a.gravar(&corpo(2, 500));
    assert!(r.is_err(), "gravação acima do RLIMIT_FSIZE deveria falhar");
    println!("ERRO_ESPERADO {}", r.err().unwrap_or_default());
    let b = a.melhor().expect("geração anterior intacta");
    assert_eq!(b.1.seq, 1);
}

#[test]
fn kill9_do_escritor_em_200_momentos_nunca_deixa_parcial() {
    let mut rng = Rng::nova("kill9");
    let d = tmp("kill9");
    let iteracoes: usize =
        std::env::var("HV_ITER").ok().and_then(|v| v.parse().ok()).unwrap_or(220);
    let mut seq_visto = 0u64;
    for it in 0..iteracoes {
        let atraso = rng.ate(70);
        let seed = rng.proximo();
        let cmd = filho(
            "escritor",
            &[("HV_DIR", d.to_str().expect("utf8")), ("HV_SEED", &seed.to_string())],
        );
        let mut f = spawn_pipe(cmd);
        esperar_pronto(&mut f);
        std::thread::sleep(std::time::Duration::from_millis(atraso));
        f.kill().expect("kill -9");
        f.wait().expect("wait");
        for nome in ["ULTIMO-context.md", "ULTIMO-checklist.md", "estado.hv"] {
            let a = Armazem::novo(&d, nome);
            let melhor = a
                .melhor()
                .unwrap_or_else(|| panic!("sem geração válida: {nome} it={it} atraso={atraso}ms"));
            for g in a.geracoes() {
                if g.caminho.exists() {
                    let v = g.lida.unwrap_or_else(|e| {
                        panic!("arquivo existente INVÁLIDO {} it={it}: {e}", g.caminho.display())
                    });
                    assert!(
                        coerente(&v.corpo),
                        "corpo misto/parcial em {} it={it}",
                        g.caminho.display()
                    );
                }
            }
            if nome == "estado.hv" {
                assert!(melhor.1.seq >= seq_visto, "seq regrediu: {} < {seq_visto}", melhor.1.seq);
                seq_visto = melhor.1.seq;
            }
        }
        // o próximo escritor retoma e limpa o que o morto deixou
        limpar_tmp(&d).expect("limpar");
    }
    assert!(seq_visto >= iteracoes as u64 / 2, "pouco progresso gravado: {seq_visto}");
}

#[test]
fn truncamento_em_todos_os_offsets_cai_na_geracao_anterior() {
    let d = tmp("trunc");
    let a = Armazem::novo(&d, "U.md");
    a.gravar(&corpo(1, 4)).unwrap();
    a.gravar(&corpo(2, 4)).unwrap();
    let original = std::fs::read(a.caminho(0)).unwrap();
    for n in 0..original.len() {
        std::fs::write(a.caminho(0), &original[..n]).unwrap();
        assert!(ler_validado(&a.caminho(0)).is_err(), "truncado em {n} foi aceito");
        let m = a.melhor().unwrap();
        assert_eq!(m.1.seq, 1, "offset {n}: deveria cair na geração 1");
    }
    std::fs::write(a.caminho(0), &original).unwrap();
    assert_eq!(a.melhor().unwrap().1.seq, 2);
}

#[test]
fn bitflip_em_cada_byte_nunca_e_lido_como_valido() {
    let mut rng = Rng::nova("bitflip");
    let d = tmp("flip");
    let a = Armazem::novo(&d, "U.md");
    a.gravar(&corpo(1, 4)).unwrap();
    a.gravar(&corpo(2, 4)).unwrap();
    let original = std::fs::read(a.caminho(0)).unwrap();
    for i in 0..original.len() {
        let mut c = original.clone();
        c[i] ^= 1 << rng.ate(8);
        std::fs::write(a.caminho(0), &c).unwrap();
        assert!(ler_validado(&a.caminho(0)).is_err(), "flip no byte {i} aceito");
        assert_eq!(a.melhor().unwrap().1.seq, 1, "byte {i}");
    }
}

#[test]
fn diretorio_sem_permissao_falha_sem_destruir_a_geracao_anterior() {
    let d = tmp("perm");
    let a = Armazem::novo(&d, "U.md");
    a.gravar(&corpo(1, 4)).unwrap();
    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o555)).unwrap();
    let r = a.gravar(&corpo(2, 4));
    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(r.is_err(), "gravar em dir sem permissão deveria falhar");
    assert_eq!(a.melhor().unwrap().1.seq, 1);
    let sobras: Vec<_> = std::fs::read_dir(&d).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(sobras.len(), 1, "sem sobras: {sobras:?}");
}

#[test]
fn disco_cheio_rlimit_fsize_mantem_a_geracao_anterior() {
    let d = tmp("cheio");
    let mut f = spawn_pipe(filho("cheio", &[("HV_DIR", d.to_str().unwrap())]));
    let st = f.wait().unwrap();
    assert!(st.success(), "filho de disco cheio falhou: {st:?}");
    let a = Armazem::novo(&d, "ULTIMO-context.md");
    assert_eq!(a.melhor().unwrap().1.seq, 1);
    assert!(std::fs::read_dir(&d)
        .unwrap()
        .flatten()
        .all(|e| !e.file_name().to_string_lossy().contains(".tmp-")));
}

#[test]
fn dois_escritores_o_segundo_e_recusado_e_o_orfao_e_retomado() {
    let cfg = workspace("dois");
    let _m = Motor::abrir(cfg.clone(), relogio_real()).expect("primeiro");
    let e = Motor::abrir(cfg.clone(), relogio_real()).err().expect("segundo deve falhar");
    assert!(e.contains("outro escritor ativo"), "{e}");
    drop(_m);
    // processo filho segura o lock e morre por kill -9: o lock é retomado e o órfão sinalizado
    let mut f =
        spawn_pipe(filho("segurar", &[("HV_DIR", cfg.dir_operacional().to_str().unwrap())]));
    esperar_pronto(&mut f);
    assert!(Motor::abrir(cfg.clone(), relogio_real()).is_err(), "lock do filho vivo");
    f.kill().unwrap();
    f.wait().unwrap();
    let _m2 = Motor::abrir(cfg, relogio_real()).expect("retoma lock de dono morto");
}
