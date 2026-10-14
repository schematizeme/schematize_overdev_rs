//! `schematize-overdev delegar` (ADR-0024) com motores FALSOS: scripts de shell que gravam o
//! argv recebido. Prova a linha de comando exata de cada motor, os códigos de saída (0/2/3/4)
//! e o shape do JSON — rodando o binário de verdade e parseando o stdout.
//!
//! **Isolamento:** HOME aponta para o projeto temporário (sob `target/`), o PATH contém SÓ o
//! diretório dos falsos e `SCHEMATIZE_AGENTES_SO_PATH=1` desliga os dirs de fallback — assim
//! "motor ausente" não depende da máquina onde o teste roda.

use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_schematize-overdev");

/// Ambiente de um teste: projeto + diretório de binários falsos + onde o argv é gravado.
struct Amb {
    proj: PathBuf,
    bin: PathBuf,
}

/// **O quê:** cria o ambiente limpo `nome`. **Onde:** todo teste.
fn amb(nome: &str) -> Amb {
    let proj = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("delegar-{}-{nome}", std::process::id()));
    let _ = std::fs::remove_dir_all(&proj);
    let bin = proj.join("fakebin");
    std::fs::create_dir_all(&bin).expect("criar fakebin");
    Amb { proj, bin }
}

impl Amb {
    /// **O quê:** instala o motor falso `nome`: grava o argv (uma linha por argumento) e o cwd
    /// em `<nome>.argv`/`<nome>.cwd`; `login status`/`status` saem com `$FAKE_LOGIN_RC`.
    fn falso(&self, nome: &str) {
        let log = self.proj.join(nome);
        let sh = format!(
            "#!/bin/sh\n\
             if [ \"$1\" = login ] || [ \"$1\" = status ]; then exit \"${{FAKE_LOGIN_RC:-0}}\"; fi\n\
             printf '%s\\n' \"$@\" > '{log}.argv'\n\
             pwd > '{log}.cwd'\n\
             echo '{{\"result\":\"feito pelo falso {nome}\"}}'\n\
             echo 'aviso no stderr' >&2\n\
             exit \"${{FAKE_RC:-0}}\"\n",
            log = log.display()
        );
        let p = self.bin.join(nome);
        std::fs::write(&p, sh).expect("escrever falso");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    /// **O quê:** roda `delegar` com `args`, `stdin` opcional e variáveis extras.
    fn rodar(&self, args: &[&str], stdin: Option<&str>, env: &[(&str, &str)]) -> Output {
        let mut c = Command::new(BIN);
        c.arg("delegar").args(args).arg("--projeto").arg(&self.proj);
        c.current_dir(&self.proj)
            .env_clear()
            .env("HOME", &self.proj)
            .env("PATH", &self.bin)
            .env("SCHEMATIZE_AGENTES_SO_PATH", "1")
            .env("SCHEMATIZE_SEM_MARKET", "1");
        for (k, v) in env {
            c.env(k, v);
        }
        c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut filho = c.spawn().expect("rodar o binário");
        if let Some(s) = stdin {
            filho.stdin.take().expect("stdin").write_all(s.as_bytes()).expect("escrever stdin");
        }
        filho.wait_with_output().expect("aguardar")
    }

    /// O argv gravado pelo motor falso `nome` (None se nunca rodou).
    fn argv(&self, nome: &str) -> Option<String> {
        std::fs::read_to_string(self.proj.join(format!("{nome}.argv"))).ok()
    }
}

/// Parseia o stdout como JSON estrito.
fn json(o: &Output) -> Value {
    let s = String::from_utf8_lossy(&o.stdout).to_string();
    serde_json::from_str(&s).unwrap_or_else(|e| panic!("JSON inválido: {e}\n{s}"))
}

/// O argv gravado é EXATAMENTE `esperado` (um argumento por linha).
fn argv_igual(gravado: &str, esperado: &[&str]) -> bool {
    gravado.lines().collect::<Vec<_>>() == esperado
}

/// Self-check: o comparador de argv REPROVA o argv errado (um teste que nunca falha é cego).
#[test]
fn comparador_de_argv_reprova_o_errado() {
    let gravado = "-p\n--model\nsonnet\n";
    assert!(argv_igual(gravado, &["-p", "--model", "sonnet"]));
    assert!(!argv_igual(gravado, &["-p", "--model", "opus"]));
    assert!(!argv_igual(gravado, &["-p", "--model"]));
}

/// Claude: argv do contrato, rc 0, JSON completo e arquivos gravados em `delegacoes/`.
#[test]
fn claude_sucesso_argv_e_json() {
    let a = amb("claude");
    a.falso("claude");
    let brief = a.proj.join("brief.txt");
    std::fs::write(&brief, "faça X").expect("brief");
    let o = a.rodar(
        &["--papel", "executor", "--brief", brief.to_str().expect("utf8"), "--json"],
        None,
        &[],
    );
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let j = json(&o);
    let argv = a.argv("claude").expect("claude foi chamado");
    assert!(
        argv_igual(&argv, &["-p", "--model", "sonnet", "--output-format", "json", "faça X"]),
        "{argv}"
    );
    assert_eq!(j["ok"], true);
    assert_eq!(j["papel"], "executor");
    assert_eq!(j["motor"], "claude");
    assert_eq!(j["modelo"], "sonnet");
    assert_eq!(j["rc"], 0);
    assert!(j["duracao_ms"].is_u64());
    assert_eq!(j["saida_resumo"], "feito pelo falso claude");
    let arq = j["arquivos"].as_str().expect("arquivos");
    assert!(arq.contains("delegacoes/") && arq.contains("-executor."), "{arq}");
    let dir = a.proj.join(".schematize/overdev/delegacoes");
    let nomes: Vec<String> = std::fs::read_dir(&dir)
        .expect("delegacoes/")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    for ext in ["brief.md", "saida.json", "log"] {
        assert!(
            nomes.iter().any(|n| n.ends_with(&format!("-executor.{ext}"))),
            "falta {ext}: {nomes:?}"
        );
    }
    let log = nomes.iter().find(|n| n.ends_with(".log")).expect("log");
    assert!(std::fs::read_to_string(dir.join(log)).expect("ler log").contains("aviso no stderr"));
    let cwd = std::fs::read_to_string(a.proj.join("claude.cwd")).expect("cwd");
    assert_eq!(
        Path::new(cwd.trim()).canonicalize().expect("c1"),
        a.proj.canonicalize().expect("c2")
    );
}

/// Codex: `exec -m … -s workspace-write --json -C <projeto> <brief>`, brief pelo stdin.
#[test]
fn codex_argv_exato_com_brief_stdin() {
    let a = amb("codex");
    a.falso("codex");
    let o =
        a.rodar(&["--papel", "fallback-1", "--brief-stdin", "--json"], Some("pinte o botão"), &[]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let proj = a.proj.display().to_string();
    let argv = a.argv("codex").expect("codex foi chamado");
    let esperado = [
        "exec",
        "-m",
        "gpt-5.6-terra",
        "-s",
        "workspace-write",
        "--json",
        "-C",
        &proj,
        "pinte o botão",
    ];
    assert!(argv_igual(&argv, &esperado), "{argv}");
    assert_eq!(json(&o)["motor"], "codex");
}

/// Cursor: `-p --model … --output-format json --force <brief>`.
#[test]
fn cursor_argv_exato() {
    let a = amb("cursor");
    a.falso("cursor-agent");
    let o =
        a.rodar(&["--papel", "fallback-cursor", "--brief-stdin", "--json"], Some("tarefa"), &[]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let argv = a.argv("cursor-agent").expect("cursor foi chamado");
    assert!(
        argv_igual(
            &argv,
            &["-p", "--model", "auto", "--output-format", "json", "--force", "tarefa"]
        ),
        "{argv}"
    );
}

/// Brief hostil (aspas, `$()`, `;`) chega como UM argumento e nada é interpretado por shell.
#[test]
fn brief_hostil_nao_passa_por_shell() {
    let a = amb("hostil");
    a.falso("claude");
    let marca = a.proj.join("PWNED");
    let brief = format!("a \"b\" $(touch {}) ; `touch {}` 'c'", marca.display(), marca.display());
    let o = a.rodar(&["--papel", "trivial", "--brief-stdin", "--json"], Some(&brief), &[]);
    assert_eq!(o.status.code(), Some(0));
    assert!(!marca.exists(), "o brief foi executado por um shell!");
    let argv = a.argv("claude").expect("argv");
    assert!(
        argv_igual(&argv, &["-p", "--model", "haiku", "--output-format", "json", &brief]),
        "{argv}"
    );
}

/// Motor que sai com erro: rc 4, `ok:false`, rc do motor preservado.
#[test]
fn motor_que_falha_da_rc4() {
    let a = amb("falha");
    a.falso("claude");
    let o = a.rodar(
        &["--papel", "executor", "--brief-stdin", "--json"],
        Some("x"),
        &[("FAKE_RC", "7")],
    );
    assert_eq!(o.status.code(), Some(4));
    let j = json(&o);
    assert_eq!(j["ok"], false);
    assert_eq!(j["rc"], 7);
    assert_eq!(j["papel"], "executor");
}

/// Motor ausente: rc 3, `motivo:indisponivel`, `proximo` = próximo ativo da escada.
#[test]
fn motor_ausente_da_rc3_com_proximo() {
    let a = amb("ausente");
    a.falso("claude");
    a.falso("codex"); // sem cursor-agent
    let o = a.rodar(&["--papel", "fallback-cursor", "--brief-stdin", "--json"], Some("x"), &[]);
    assert_eq!(o.status.code(), Some(3));
    let j = json(&o);
    assert_eq!(
        j,
        serde_json::json!({"ok": false, "motivo": "indisponivel", "proximo": "fallback-2"})
    );
    assert!(a.argv("cursor-agent").is_none());
}

/// Sem login (`codex login status` ≠ 0) é indisponível; `proximo` é o degrau seguinte; e o
/// último degrau devolve `proximo: null`.
#[test]
fn sem_login_e_fim_da_escada() {
    let a = amb("login");
    a.falso("codex");
    a.falso("claude");
    let o = a.rodar(
        &["--papel", "fallback-1", "--brief-stdin", "--json"],
        Some("x"),
        &[("FAKE_LOGIN_RC", "1")],
    );
    assert_eq!(o.status.code(), Some(3));
    // fallback-cursor (cursor-agent ausente) é PULADO: vai direto ao fallback-2 (claude).
    assert_eq!(json(&o)["proximo"], "fallback-2");
    assert!(a.argv("codex").is_none(), "não pode executar sem login");
    // fallback-2 é claude (ausente neste ambiente) e é o último da escada.
    let a = amb("login-fim");
    let o = a.rodar(&["--papel", "fallback-2", "--brief-stdin", "--json"], Some("x"), &[]);
    assert_eq!(o.status.code(), Some(3));
    assert!(json(&o)["proximo"].is_null());
}

/// Papel inativo no override do projeto: rc 3 sem executar nada.
#[test]
fn papel_inativo_da_rc3() {
    let a = amb("inativo");
    a.falso("claude");
    let od = a.proj.join(".schematize/overdev");
    std::fs::create_dir_all(&od).expect("dir");
    std::fs::write(
        od.join("agentes.toml"),
        "[papeis.executor]\nmotor=\"claude\"\nmodelo=\"sonnet\"\nativo=false\n",
    )
    .expect("toml");
    let o = a.rodar(&["--papel", "executor", "--brief-stdin", "--json"], Some("x"), &[]);
    assert_eq!(o.status.code(), Some(3));
    // fallback-1 (codex) e fallback-cursor não estão instalados: pula direto ao fallback-2.
    assert_eq!(json(&o)["proximo"], "fallback-2");
    assert!(a.argv("claude").is_none());
}

/// Uso/config inválida: papel inexistente, sem brief e TOML quebrado => rc 2 (JSON com `ok:false`).
#[test]
fn uso_e_config_invalida_dao_rc2() {
    let a = amb("uso");
    a.falso("claude");
    let o = a.rodar(&["--papel", "naoexiste", "--brief-stdin", "--json"], Some("x"), &[]);
    assert_eq!(o.status.code(), Some(2));
    assert_eq!(json(&o)["ok"], false);
    let o = a.rodar(&["--papel", "executor", "--json"], None, &[]);
    assert_eq!(o.status.code(), Some(2));
    let od = a.proj.join(".schematize/overdev");
    std::fs::create_dir_all(&od).expect("dir");
    std::fs::write(od.join("agentes.toml"), "versao = 2\n").expect("toml");
    let o = a.rodar(&["--papel", "executor", "--brief-stdin", "--json"], Some("x"), &[]);
    assert_eq!(o.status.code(), Some(2));
    assert!(a.argv("claude").is_none());
}

/// `--listar --json`: papéis efetivos + disponibilidade por motor.
#[test]
fn listar_mostra_papeis_e_motores() {
    let a = amb("listar");
    a.falso("claude");
    let o = a.rodar(&["--listar", "--json"], None, &[]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let j = json(&o);
    assert_eq!(j["papeis"].as_array().expect("papeis").len(), 9);
    assert_eq!(j["motores"]["claude"]["disponivel"], true);
    assert_eq!(j["motores"]["codex"]["disponivel"], false);
    assert_eq!(j["escada"][0], "trivial");
    let exec = j["papeis"]
        .as_array()
        .expect("p")
        .iter()
        .find(|p| p["papel"] == "executor")
        .expect("executor");
    assert_eq!(exec["disponivel"], true);
}
