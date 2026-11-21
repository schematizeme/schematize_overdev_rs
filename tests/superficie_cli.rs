//! Superfície da CLI de resiliência e do hook Stop sob `SCHEMATIZE_HANDOFF_VIVO`.
//!
//! **O quê:** roda o binário real: `--help` lista os comandos novos, `sistema diag`/`memoria status`
//! devolvem JSON estrito, `vigia unit` imprime a unit, `vigia run` sem o binário dá mensagem
//! acionável e `check` não bloqueia com `SCHEMATIZE_HANDOFF_VIVO=1` (mesmo com checklist aberto).

use std::path::PathBuf;
use std::process::{Command, Output};

/// **O quê:** roda o binário com `args`, `HOME`/cwd em `dir` e o `PATH` dado. **Onde:** os testes.
fn rodar(dir: &PathBuf, args: &[&str], path: Option<&str>, env: &[(&str, &str)]) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_schematize-overdev"));
    c.args(args).current_dir(dir).env("HOME", dir).env("LANG", "en_US.UTF-8");
    c.env_remove("SCHEMATIZE_HANDOFF_VIVO");
    if let Some(p) = path {
        c.env("PATH", p);
    }
    c.envs(env.iter().copied()).output().expect("spawn")
}

/// **O quê:** dir temporário limpo. **Onde:** os testes.
fn tmp(nome: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("od-sup-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("tmp");
    d
}

/// **O quê:** stdout como texto. **Onde:** os testes.
fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

/// `--help` lista todos os comandos novos.
#[test]
fn help_lista_os_comandos_novos() {
    let d = tmp("help");
    let h = out(&rodar(&d, &["--help"], None, &[]));
    for c in ["executar", "retomar", "sistema", "memoria", "vigia", "handoff-vivo"] {
        assert!(h.contains(c), "faltou `{c}` no --help:\n{h}");
    }
}

/// `sistema diag --json` e `memoria status --json` são JSON estrito; o texto pt/en funciona.
#[test]
fn diag_e_memoria_status_sao_json_estrito() {
    let d = tmp("diag");
    let o = rodar(&d, &["sistema", "diag", "--json"], None, &[]);
    assert!(o.status.success());
    serde_json::from_str::<serde_json::Value>(&out(&o)).expect("diag --json inválido");
    let o = rodar(&d, &["memoria", "status", "--json"], None, &[]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_str(&out(&o)).expect("status --json inválido");
    assert!(v["host"]["total_mib"].is_u64() && v["governador"].is_object(), "{v}");
    for l in ["pt", "en"] {
        assert!(rodar(&d, &["sistema", "diag", "--lang", l], None, &[]).status.success());
    }
    assert!(!rodar(&d, &["sistema", "diag", "--lang", "xx"], None, &[]).status.success());
}

/// `vigia unit` só imprime; `vigia run` sem o `schematize-vigia` explica como obter.
#[test]
fn vigia_unit_e_run_sem_binario() {
    let d = tmp("vigia");
    let u = out(&rodar(&d, &["vigia", "unit"], None, &[]));
    assert!(u.contains("Restart=always") && u.contains("retomar"), "{u}");
    assert!(!d.join(".config").exists(), "vigia unit não pode instalar nada");
    let o = rodar(&d, &["vigia", "run", "--", "--status"], Some("/nada"), &[]);
    assert!(!o.status.success());
    let e = String::from_utf8_lossy(&o.stderr);
    assert!(e.contains("schematize-vigia") && e.contains("cargo install"), "{e}");
}

/// Monta um projeto com run ATIVO e checklist de máquina aberto. **Onde:** testes do Stop hook.
fn projeto_com_item_aberto(nome: &str) -> PathBuf {
    let d = tmp(nome);
    let od = d.join(".schematize/overdev");
    std::fs::create_dir_all(&od).expect("od");
    std::fs::write(od.join("state.json"), r#"{"mode":"active","max_iters":200,"objetivo":"t","started":0}"#).unwrap();
    std::fs::write(od.join("CHECKLIST.md"), "- [ ] trabalho aberto\n").unwrap();
    d
}

/// Com `SCHEMATIZE_HANDOFF_VIVO=1` o Stop hook não bloqueia; sem a variável bloqueia como antes.
#[test]
fn stop_hook_livre_com_handoff_vivo() {
    let d = projeto_com_item_aberto("stop");
    let livre = rodar(&d, &["check"], None, &[("SCHEMATIZE_HANDOFF_VIVO", "1")]);
    assert!(livre.status.success());
    assert!(out(&livre).trim().is_empty(), "não deveria bloquear: {}", out(&livre));
    let preso = rodar(&d, &["check"], None, &[]);
    assert!(out(&preso).contains("MODO OVERDEV"), "comportamento atual mudou: {}", out(&preso));
}
