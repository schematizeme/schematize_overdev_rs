//! Contrato do binário para `checklist particionar|sync|check|proxima` (ADR-0027): saída e
//! código de saída reais, num HOME temporário (nunca o real).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_schematize-overdev");

/// **O quê:** projeto temporário com o dir do overdev. **Onde:** todos os testes daqui.
fn projeto(nome: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("overdev-part-{}-{nome}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join(".schematize/overdev/checklist")).expect("criar");
    d
}

/// **O quê:** roda o binário no projeto. **Onde:** todos os testes daqui.
fn rodar(p: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(p)
        .env("HOME", p)
        .env("SCHEMATIZE_SEM_MARKET", "1")
        .output()
        .expect("rodar o binário")
}

#[test]
fn ciclo_completo_pelo_binario() {
    let p = projeto("ciclo");
    let od = p.join(".schematize/overdev");
    let corpo: String = (0..30).map(|i| format!("- [ ] item {i}\n")).collect();
    std::fs::write(od.join("CHECKLIST.md"), format!("# t\n\n## [p1] Fase um\n{corpo}")).unwrap();

    let o = rodar(&p, &["checklist", "particionar", "--json"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).expect("JSON");
    assert_eq!(v["itens"], 30);
    assert_eq!(v["particoes"][0], "p1-001-fase-um.md");

    let o = rodar(&p, &["checklist", "proxima"]);
    assert!(String::from_utf8_lossy(&o.stdout).trim().ends_with("checklist/p1-001-fase-um.md"));

    // Fecha tudo; o sync renomeia e a próxima some.
    let f = od.join("checklist/p1-001-fase-um.md");
    std::fs::write(&f, std::fs::read_to_string(&f).unwrap().replace("- [ ]", "- [x]")).unwrap();
    let o = rodar(&p, &["checklist", "sync"]);
    assert!(String::from_utf8_lossy(&o.stdout)
        .contains("p1-001-fase-um.md -> p1-001-fase-um--completed.md"));
    let o = rodar(&p, &["checklist", "proxima", "--json"]);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&o.stdout).unwrap()["proxima"],
        serde_json::Value::Null
    );
    assert!(rodar(&p, &["checklist", "check"]).status.success());
    let _ = std::fs::remove_dir_all(&p);
}

#[test]
fn check_sai_nao_zero_com_violacao() {
    let p = projeto("violacao");
    let od = p.join(".schematize/overdev/checklist");
    std::fs::write(od.join("p2-001-a--completed.md"), "- [ ] aberto escondido\n").unwrap();
    let o = rodar(&p, &["checklist", "check", "--json"]);
    assert!(!o.status.success(), "violação tem de dar exit != 0");
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).expect("JSON");
    assert_eq!(v["ok"], false);
    assert!(v["erros"][0].as_str().unwrap().contains("--completed"));
    // `checklist --json` (sem subcomando) continua o de sempre.
    let o = rodar(&p, &["checklist", "--json"]);
    assert!(o.status.success());
    let _ = std::fs::remove_dir_all(&p);
}
