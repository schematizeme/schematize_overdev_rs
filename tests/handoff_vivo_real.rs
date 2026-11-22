//! Handoff vivo em USO REAL: `claude -p` de verdade (modelo haiku) com o hook Stop do overdev.
//!
//! **O quê:** com um checklist overdev ABERTO e estado `active` no workspace, `handoff-vivo agora`
//! grava as "Notas do Claude" sem que o Stop hook bloqueie, e `verificar` passa.
//! **Onde:** NÃO roda por padrão (custa tokens): exige `claude` no PATH e
//! `SCHEMATIZE_TESTE_CLAUDE_REAL=1`; senão imprime o motivo e passa.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// **O quê:** motivo para pular, ou `None` se o teste real pode rodar. **Onde:** início do teste.
fn motivo_de_pular() -> Option<String> {
    if std::env::var("SCHEMATIZE_TESTE_CLAUDE_REAL").as_deref() != Ok("1") {
        return Some("SCHEMATIZE_TESTE_CLAUDE_REAL!=1 (teste real custa tokens)".into());
    }
    let ok = Command::new("claude").arg("--version").output().is_ok_and(|o| o.status.success());
    (!ok).then(|| "`claude` não está no PATH".to_string())
}

/// **O quê:** workspace `<tmp>/proj` com archive, repo git e overdev ativo com checklist aberto.
fn workspace() -> PathBuf {
    let raiz = std::env::temp_dir().join(format!("hv-real-{}", std::process::id())).join("proj");
    let _ = std::fs::remove_dir_all(raiz.parent().expect("pai"));
    let od = raiz.join(".schematize/overdev");
    std::fs::create_dir_all(&od).expect("overdev");
    std::fs::create_dir_all(raiz.join("proj_archive/relatorios")).expect("archive");
    std::fs::create_dir_all(raiz.join("repo")).expect("repo");
    std::fs::write(
        od.join("state.json"),
        r#"{"mode":"active","max_iters":200,"objetivo":"t","started":0}"#,
    )
    .expect("state");
    std::fs::write(od.join("CHECKLIST.md"), "- [x] feito\n- [ ] aberto de maquina\n")
        .expect("checklist");
    std::fs::write(raiz.join("proj_archive/relatorios/2026-11-22-x.md"), "# Relatório X\n")
        .expect("relatorio");
    let g = |a: &[&str]| {
        let s = Command::new("git").args(a).current_dir(raiz.join("repo")).output().expect("git");
        assert!(s.status.success(), "git {a:?}");
    };
    g(&["init", "-q"]);
    g(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "x"]);
    raiz
}

/// **O quê:** roda `schematize-overdev handoff-vivo <args>` no workspace. **Onde:** o teste.
fn hv(ws: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_schematize-overdev"))
        .arg("handoff-vivo")
        .args(args)
        .arg("--projeto")
        .arg(ws)
        .current_dir(ws)
        .output()
        .expect("spawn")
}

/// Uma chamada real: grava notas, não é bloqueada pelo hook Stop e o arquivo verifica.
#[test]
fn agora_com_claude_real_grava_notas_e_verifica() {
    if let Some(m) = motivo_de_pular() {
        eprintln!("[handoff_vivo_real] IGNORADO: {m}");
        return;
    }
    let ws = workspace();
    let o = hv(&ws, &["agora", "--modelo", "haiku"]);
    let (so, se) = (String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    assert!(o.status.success(), "agora falhou: {so} {se}");
    let ctx = std::fs::read_to_string(ws.join("proj_archive/context/ULTIMO-context.md"))
        .expect("ULTIMO-context.md");
    assert!(ctx.contains("## Notas do Claude"), "sem notas do Claude:\n{ctx}");
    assert!(!ctx.contains("decision\":\"block"), "saída do hook vazou no handoff:\n{ctx}");
    let v = hv(&ws, &["verificar"]);
    assert!(v.status.success(), "verificar: {}", String::from_utf8_lossy(&v.stdout));
    let _ = std::fs::remove_dir_all(ws.parent().expect("pai"));
}

/// Contraste SIMULADO (sem Claude real, roda no gate): um stub que, do ambiente que o handoff vivo
/// entrega ao `claude -p`, executa o `check` do hook Stop num workspace com overdev ATIVO. Com a
/// variável o hook libera; sem ela bloqueia (prova que é a variável que protege).
#[test]
fn contraste_stub_hook_stop_com_e_sem_variavel() {
    use overdev_casa::handoff_vivo::ciclo::{relogio_real, Motor};
    use overdev_casa::handoff_vivo::config::Config;
    use std::os::unix::fs::PermissionsExt;
    let ws = workspace();
    let bin = env!("CARGO_BIN_EXE_schematize-overdev");
    let saida = ws.join("saida-stub");
    std::fs::create_dir_all(&saida).expect("saida");
    let script = ws.join("stub.sh");
    let corpo = format!(
        "#!/usr/bin/env bash\ncat >/dev/null\n\
         (cd '{ws}' && '{bin}' check </dev/null >'{o}/com' 2>&1)\n\
         (cd '{ws}' && env -u SCHEMATIZE_HANDOFF_VIVO '{bin}' check </dev/null >'{o}/sem' 2>&1)\n\
         printf '### Decisões e motivos\\nsem informação\\n### Hipóteses em aberto\\nsem informação\\n\
### Próximo passo imediato\\nsem informação\\n### Armadilhas\\nsem informação\\n'\n",
        ws = ws.display(),
        o = saida.display()
    );
    std::fs::write(&script, corpo).expect("stub");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let mut cfg = Config::para(&ws);
    cfg.claude_cmd = Some(vec![script.to_string_lossy().into_owned()]);
    Motor::abrir(cfg, relogio_real()).expect("abrir").agora().expect("agora");
    let com = std::fs::read_to_string(saida.join("com")).expect("com");
    let sem = std::fs::read_to_string(saida.join("sem")).expect("sem");
    assert!(com.trim().is_empty(), "com a variável o hook não pode bloquear: {com}");
    assert!(
        sem.contains("\"decision\":\"block\""),
        "sem a variável o hook deveria bloquear: {sem}"
    );
    let _ = std::fs::remove_dir_all(ws.parent().expect("pai"));
}
