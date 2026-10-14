//! Os escritores do checklist (`hold`, `park`, `human`, `answer`, `refuse`) nos DOIS layouts:
//! monolito legado e particionado (ADR-0027), pelo binário real, em HOME temporário.

use std::path::{Path, PathBuf};
use std::process::Output;

const BIN: &str = env!("CARGO_BIN_EXE_schematize-overdev");

/// **O quê:** `n` itens de máquina com o prefixo dado. **Onde:** montagem dos layouts.
fn itens(n: usize, marca: &str, pref: &str) -> String {
    (0..n).map(|i| format!("- [{marca}] {pref}{i}\n")).collect()
}

/// **O quê:** projeto temporário; `particionado` escolhe o layout. Em ambos há os mesmos itens:
/// `alfa0..` (máquina), `humano-um` na 1ª parte e `humano-dois` na 3ª. **Onde:** todos os testes.
fn projeto(nome: &str, particionado: bool) -> (PathBuf, PathBuf) {
    let p = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("overdev-esc-{}-{nome}-{particionado}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    let od = p.join(".schematize/overdev");
    std::fs::create_dir_all(od.join("checklist")).expect("criar");
    let a = format!("{}- [H ] humano-um\n", itens(3, " ", "alfa"));
    let c = format!("{}- [H ] humano-dois\n", itens(3, " ", "gama"));
    let b = itens(3, "x", "beta");
    if particionado {
        std::fs::write(od.join("CHECKLIST.md"), "# indice\n").unwrap();
        std::fs::write(od.join("checklist/p2-001-a.md"), &a).unwrap();
        std::fs::write(od.join("checklist/p2-002-b--completed.md"), &b).unwrap();
        std::fs::write(od.join("checklist/p2-003-c.md"), &c).unwrap();
    } else {
        std::fs::write(od.join("CHECKLIST.md"), format!("{a}{b}{c}")).unwrap();
    }
    (p, od)
}

/// **O quê:** roda o binário no projeto. **Onde:** todos os testes.
fn rodar(p: &Path, args: &[&str]) -> Output {
    std::process::Command::new(BIN)
        .args(args)
        .current_dir(p)
        .env("HOME", p)
        .env("SCHEMATIZE_SEM_MARKET", "1")
        .output()
        .expect("rodar")
}

/// **O quê:** o checklist inteiro lido (índice + partições). **Onde:** as asserções.
fn tudo(od: &Path) -> String {
    overdev_casa::paths::read_multidoc(od, "CHECKLIST.md", "checklist")
}

/// **O quê:** nomes dos arquivos em `checklist/`, ordenados. **Onde:** as asserções de sufixo.
fn nomes(od: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(od.join("checklist"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    v.sort();
    v
}

fn ok(o: &Output) {
    assert!(o.status.success(), "falhou: {}", String::from_utf8_lossy(&o.stderr));
}

#[test]
fn hold_nos_dois_layouts() {
    for part in [false, true] {
        let (p, od) = projeto("hold", part);
        ok(&rodar(&p, &["hold", "gama1"]));
        let t = tudo(&od);
        assert!(t.contains("- [~] gama1") && t.contains("- [ ] gama0"), "{part}: {t}");
        let e = rodar(&p, &["hold", "inexistente"]);
        assert!(!e.status.success(), "{part}: sem match é erro");
        let _ = std::fs::remove_dir_all(&p);
    }
}

#[test]
fn park_e_answer_liberam_a_maquina_nos_dois_layouts() {
    for part in [false, true] {
        let (p, od) = projeto("park", part);
        ok(&rodar(&p, &["park", "gama2", "usar", "X", "ou", "Y?"]));
        let t = tudo(&od);
        assert!(t.contains("- [~] gama2 <!-- ovf:q:"), "{part}: {t}");
        // Humanos abertos, em ordem global: humano-um(1), pergunta parkeada(2), humano-dois(3).
        ok(&rodar(&p, &["answer", "2", "use", "X"]));
        let t = tudo(&od);
        assert!(t.contains("- [ ] gama2 <!-- ovf:q:"), "{part}: máquina liberada");
        assert!(t.contains("- [H r]") && t.contains("resposta: use X"), "{part}: {t}");
        assert!(std::fs::read_to_string(od.join("DECISOES.md")).unwrap().contains("use X"));
        let _ = std::fs::remove_dir_all(&p);
    }
}

#[test]
fn refuse_cancela_nos_dois_layouts() {
    for part in [false, true] {
        let (p, od) = projeto("refuse", part);
        ok(&rodar(&p, &["park", "alfa1", "pode?"]));
        ok(&rodar(&p, &["refuse", "pode?", "não", "cabe"]));
        let t = tudo(&od);
        assert!(t.contains("- [-] alfa1"), "{part}: {t}");
        let _ = std::fs::remove_dir_all(&p);
    }
}

#[test]
fn human_por_indice_global_e_por_texto() {
    for part in [false, true] {
        let (p, od) = projeto("human", part);
        ok(&rodar(&p, &["human", "--done", "2"]));
        let t = tudo(&od);
        assert!(t.contains("- [H x] humano-dois") && t.contains("- [H ] humano-um"), "{part}");
        ok(&rodar(&p, &["human", "humano-um"]));
        assert!(tudo(&od).contains("- [H x] humano-um"));
        assert!(!rodar(&p, &["human", "--done", "1"]).status.success(), "acabaram os humanos");
        let _ = std::fs::remove_dir_all(&p);
    }
}

/// Fechar o último item pendente de uma partição pelo escritor faz o `sync` pôr o sufixo;
/// e reabrir (park/hold não reabrem; a resposta libera `[~]` → `[ ]`) tira o sufixo.
#[test]
fn escrita_roda_sync_e_o_sufixo_acompanha_o_estado() {
    let (p, od) = projeto("sufixo", true);
    // Partição 1 tem 3 itens + humano-um: fechar o humano não basta (3 máquina abertos).
    ok(&rodar(&p, &["human", "humano-um"]));
    assert!(nomes(&od).contains(&"p2-001-a.md".to_string()));
    let idx = std::fs::read_to_string(od.join("CHECKLIST.md")).unwrap();
    assert!(idx.contains("| partição | prio | abertos | total | depende | estado |"), "{idx}");
    assert!(!idx.lines().any(|l| l.trim_start().starts_with("- [")));
    // Fecha tudo da partição 1 à mão e dispara um escritor qualquer: sync renomeia.
    let f = od.join("checklist/p2-001-a.md");
    std::fs::write(&f, std::fs::read_to_string(&f).unwrap().replace("- [ ]", "- [x]")).unwrap();
    ok(&rodar(&p, &["human", "humano-dois"]));
    assert!(nomes(&od).contains(&"p2-001-a--completed.md".to_string()), "{:?}", nomes(&od));
    let _ = std::fs::remove_dir_all(&p);
}

/// O alvo posicional vale no checklist INTEIRO: o 2º humano aberto está na 3ª partição.
#[test]
fn answer_posicional_atravessa_particoes() {
    let (p, od) = projeto("posicional", true);
    ok(&rodar(&p, &["answer", "2", "feito", "assim"]));
    let t = tudo(&od);
    assert!(t.contains("- [H r] humano-dois") && t.contains("- [H ] humano-um"), "{t}");
    let _ = std::fs::remove_dir_all(&p);
}
