//! Testes do motor do handoff vivo: hash/mtime estável, Claude falso (trava, memória, lixo,
//! segredo), backoff sem empilhar, relógio retrocedendo, erro de permissão e RSS do loop.

mod handoff_vivo_comum;

use handoff_vivo_comum::*;
use overdev_casa::handoff_vivo::atomico::{ler_validado, Armazem};
use overdev_casa::handoff_vivo::ciclo::{Motor, Relogio};
use overdev_casa::handoff_vivo::claude::backoff_s;
use overdev_casa::handoff_vivo::config::Config;
use overdev_casa::handoff_vivo::verificar::verificar;
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

/// Relógio controlável.
fn relogio(t: &Arc<AtomicU64>) -> Relogio {
    let t = Arc::clone(t);
    Arc::new(move || t.load(Ordering::SeqCst))
}

fn mtime(p: &std::path::Path) -> SystemTime {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .unwrap_or_else(|e| panic!("mtime {}: {e}", p.display()))
}

/// Cria um stub executável (script bash) e devolve o comando.
fn stub(dir: &std::path::Path, nome: &str, corpo: &str) -> Vec<String> {
    let p = dir.join(nome);
    std::fs::write(&p, format!("#!/usr/bin/env bash\ncat >/dev/null\n{corpo}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    vec![p.to_string_lossy().into_owned()]
}

/// Filho `rss`: 100 ticks mecânicos; imprime o crescimento do VmHWM.
#[test]
fn filho_hv() {
    let Ok(modo) = std::env::var("HV_MODO") else { return };
    assert_eq!(modo, "rss");
    let mut cfg = Config::detectar(std::path::Path::new(&std::env::var("HV_WS").expect("HV_WS")));
    cfg.sem_claude = true;
    let hwm0 = vm_kb("VmHWM");
    let mut m =
        Motor::abrir(cfg, overdev_casa::handoff_vivo::ciclo::relogio_real()).expect("abrir");
    for _ in 0..100 {
        m.tick().expect("tick");
    }
    println!("RSS_CRESCIMENTO_KB {}", vm_kb("VmHWM").saturating_sub(hwm0));
    std::process::exit(0);
}

#[test]
fn tick_sem_mudanca_nao_reescreve_nada() {
    let mut cfg = workspace("estavel");
    cfg.sem_claude = true;
    let t = Arc::new(AtomicU64::new(1_800_000_000));
    let mut m = Motor::abrir(cfg.clone(), relogio(&t)).unwrap();
    let a = m.tick().unwrap();
    assert!(a.escreveu);
    let ctx = cfg.dir_context();
    let arquivos: Vec<_> = std::fs::read_dir(&ctx).unwrap().flatten().map(|e| e.path()).collect();
    assert!(arquivos.len() >= 4, "ULTIMO x2 + datados x2: {arquivos:?}");
    let op = cfg.dir_operacional().join("estado.hv");
    let antes: Vec<_> = arquivos.iter().chain([&op]).map(|p| (p.clone(), mtime(p))).collect();
    std::thread::sleep(Duration::from_millis(30));
    t.fetch_add(30, Ordering::SeqCst);
    for _ in 0..3 {
        let b = m.tick().unwrap();
        assert!(!b.escreveu, "tick sem mudança reescreveu");
        assert_eq!((b.seq_context, b.seq_checklist), (a.seq_context, a.seq_checklist));
    }
    for (p, mt) in antes {
        assert_eq!(mtime(&p), mt, "mtime mudou: {}", p.display());
    }
    // mudou o checklist -> reescreve (e só então)
    std::fs::write(
        cfg.workspace.join(".schematize/overdev/CHECKLIST.md"),
        "- [x] a\n- [x] b\n- [ ] c\n",
    )
    .unwrap();
    let c = m.tick().unwrap();
    assert!(c.escreveu && c.seq_checklist > a.seq_checklist);
    let corpo =
        String::from_utf8(Armazem::novo(&ctx, "ULTIMO-checklist.md").melhor().unwrap().1.corpo)
            .unwrap();
    assert!(corpo.contains("2 feito(s), 1 em aberto") && corpo.contains("- [ ] c"), "{corpo}");
}

#[test]
fn handoff_tem_git_estado_em_voo_relatorio_e_parkeados() {
    let mut cfg = workspace("conteudo");
    cfg.sem_claude = true;
    let mut m = Motor::abrir(cfg.clone(), Arc::new(|| 1_800_000_000)).unwrap();
    m.tick().unwrap();
    let c = String::from_utf8(
        Armazem::novo(&cfg.dir_context(), "ULTIMO-context.md").melhor().unwrap().1.corpo,
    )
    .unwrap();
    for esperado in ["`repo1`", "u1.json", "2026-11-21-x.md — Relatório X", "Repos do workspace"]
    {
        assert!(c.contains(esperado), "falta `{esperado}` em:\n{c}");
    }
    let k = String::from_utf8(
        Armazem::novo(&cfg.dir_context(), "ULTIMO-checklist.md").melhor().unwrap().1.corpo,
    )
    .unwrap();
    assert!(
        k.contains("[~] parkeado um") && k.contains("[x] feito um") && k.contains("[ ] aberto um"),
        "{k}"
    );
}

#[test]
fn claude_falso_ok_redige_segredos_e_vai_ao_ledger() {
    let mut cfg = workspace("claude-ok");
    let d = tmp("stubs-ok");
    cfg.claude_cmd = Some(stub(&d, "ok.sh", "printf '### Decisões e motivos\\nusei flock. chave sk-ant-api03-ABCDEFGHIJKLMNOP de me@lucasandrade.net\\npassword: hunter2xyz\\n### Próximo passo imediato\\nrodar testes\\n'"));
    let mut m = Motor::abrir(cfg.clone(), Arc::new(|| 1_800_000_000)).unwrap();
    m.agora().unwrap();
    let ctx = String::from_utf8(
        Armazem::novo(&cfg.dir_context(), "ULTIMO-context.md").melhor().unwrap().1.corpo,
    )
    .unwrap();
    assert!(ctx.contains("## Notas do Claude (") && ctx.contains("usei flock"), "{ctx}");
    for vazou in ["sk-ant", "lucasandrade", "hunter2xyz"] {
        assert!(!ctx.contains(vazou), "vazou {vazou}");
    }
    assert_eq!(m.estado().falhas, 0);
    let led = std::fs::read_to_string(cfg.dir_overdev().join("ledger.jsonl")).unwrap();
    assert!(
        led.contains("handoff-vivo")
            && led.contains("\"estado\":\"done\"")
            && led.contains("custo_micro_usd"),
        "{led}"
    );
    // estado do Claude igual -> não chama de novo
    let n_linhas = led.lines().count();
    m.tick().unwrap();
    assert!(!m.claude_em_voo());
    assert_eq!(
        std::fs::read_to_string(cfg.dir_overdev().join("ledger.jsonl")).unwrap().lines().count(),
        n_linhas
    );
}

/// Roda um stub que falha e confirma motivo, backoff e que o handoff mecânico segue válido.
fn falha_do_claude(nome: &str, corpo: &str, motivo: &str, ajusta: impl FnOnce(&mut Config)) {
    let mut cfg = workspace(nome);
    let d = tmp(&format!("stubs-{nome}"));
    cfg.claude_cmd = Some(stub(&d, "s.sh", corpo));
    ajusta(&mut cfg);
    let t = Arc::new(AtomicU64::new(1_800_000_000));
    let mut m = Motor::abrir(cfg.clone(), relogio(&t)).unwrap();
    m.agora().unwrap();
    let e = m.estado().clone();
    assert_eq!(e.falhas, 1, "{nome}: {e:?}");
    let erro = e.ultimo_erro.clone().unwrap_or_default();
    assert!(erro.contains(motivo), "{nome}: motivo `{erro}` não contém `{motivo}`");
    assert_eq!(e.proximo_claude_em, 1_800_000_000 + backoff_s(cfg.claude_s, 1));
    assert!(e.notas.is_none());
    assert!(std::fs::read_to_string(cfg.dir_operacional().join("erros.log"))
        .unwrap()
        .contains("camada Claude falhou"));
    assert!(verificar(&cfg, false).unwrap().ok);
    // tick seguinte não chama de novo (backoff)
    t.fetch_add(60, Ordering::SeqCst);
    m.tick().unwrap();
    assert!(!m.claude_em_voo(), "{nome}: não deveria redisparar dentro do backoff");
}

#[test]
fn claude_falso_que_estoura_memoria() {
    falha_do_claude("oom", "x=$(head -c 2000000000 /dev/zero | tr '\\0' a); echo ok", "", |c| {
        c.claude_mem_mib = 200
    });
}

#[test]
fn claude_falso_morto_por_sigkill() {
    falha_do_claude("sigkill", "kill -9 $$", "SIGKILL", |_| {});
}

#[test]
fn claude_falso_lixo_nao_utf8() {
    falha_do_claude("lixo", "printf '\\377\\376lixo'", "UTF-8", |_| {});
}

#[test]
fn claude_falso_saida_gigante() {
    falha_do_claude("gigante", "head -c 3000000 /dev/zero | tr '\\0' a", "grande demais", |c| {
        c.max_notas = 100
    });
}

#[test]
fn claude_falso_com_codigo_de_erro() {
    falha_do_claude("exit1", "echo falhou >&2; exit 7", "Some(7)", |_| {});
}

#[test]
fn claude_que_trava_nao_bloqueia_o_tick_nem_empilha() {
    let mut cfg = workspace("trava");
    let d = tmp("stubs-trava");
    cfg.claude_cmd = Some(stub(&d, "hang.sh", "sleep 60"));
    cfg.claude_timeout_s = 2;
    let t = Arc::new(AtomicU64::new(1_800_000_000));
    let mut m = Motor::abrir(cfg.clone(), relogio(&t)).unwrap();
    m.tick_mecanico(false).unwrap();
    assert!(m.disparar_claude());
    assert!(!m.disparar_claude(), "não pode empilhar");
    let t0 = Instant::now();
    std::fs::write(cfg.workspace.join(".schematize/overdev/CHECKLIST.md"), "- [ ] novo\n").unwrap();
    let tk = m.tick().unwrap();
    assert!(
        tk.escreveu && t0.elapsed() < Duration::from_millis(1500),
        "tick bloqueou {:?}",
        t0.elapsed()
    );
    let ate = Instant::now() + Duration::from_secs(15);
    while m.claude_em_voo() && Instant::now() < ate {
        std::thread::sleep(Duration::from_millis(100));
        m.coletar_claude();
    }
    assert!(!m.claude_em_voo(), "timeout não recolhido");
    assert!(m.estado().ultimo_erro.as_deref().unwrap_or("").contains("timeout"));
    assert_eq!(m.estado().falhas, 1);
}

#[test]
fn relogio_retrocedendo_mantem_seq_monotonica_e_nomes_unicos() {
    let mut cfg = workspace("relogio");
    cfg.sem_claude = true;
    let t = Arc::new(AtomicU64::new(1_900_000_000));
    let mut a = Motor::abrir(cfg.clone(), relogio(&t)).unwrap();
    let t1 = a.tick().unwrap();
    drop(a);
    t.store(1_000_000_000, Ordering::SeqCst); // volta no tempo
    std::fs::write(cfg.workspace.join(".schematize/overdev/CHECKLIST.md"), "- [ ] x\n").unwrap();
    let mut b = Motor::abrir(cfg.clone(), relogio(&t)).unwrap();
    let t2 = b.tick().unwrap();
    assert!(t2.seq_checklist > t1.seq_checklist, "seq deve crescer mesmo com relógio voltando");
    t.store(500_000_000, Ordering::SeqCst);
    std::fs::write(cfg.workspace.join(".schematize/overdev/CHECKLIST.md"), "- [ ] y\n").unwrap();
    let t3 = b.tick().unwrap();
    assert!(t3.seq_checklist > t2.seq_checklist);
    let datados = std::fs::read_dir(cfg.dir_context())
        .unwrap()
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with("ULTIMO"))
        .count();
    assert!(datados >= 3, "um datado por sessão/arquivo, sem sobrescrever o anterior: {datados}");
    assert!(verificar(&cfg, false).unwrap().ok);
}

#[test]
fn erro_de_permissao_mantem_ultimo_valido_e_registra() {
    let mut cfg = workspace("perm-motor");
    cfg.sem_claude = true;
    let mut m = Motor::abrir(cfg.clone(), Arc::new(|| 1_800_000_000)).unwrap();
    m.tick().unwrap();
    let ctx = cfg.dir_context();
    std::fs::set_permissions(&ctx, std::fs::Permissions::from_mode(0o555)).unwrap();
    std::fs::write(cfg.workspace.join(".schematize/overdev/CHECKLIST.md"), "- [ ] z\n").unwrap();
    let r = m.tick();
    std::fs::set_permissions(&ctx, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(r.is_err(), "deveria falhar sem permissão");
    assert!(std::fs::read_to_string(cfg.dir_operacional().join("erros.log"))
        .unwrap()
        .contains("tick mecânico"));
    assert!(ler_validado(&ctx.join("ULTIMO-checklist.md")).is_ok());
    // recupera no tick seguinte, sem intervenção
    assert!(m.tick().unwrap().escreveu);
}

#[test]
fn verificar_reporta_e_repara_geracao_corrompida() {
    let mut cfg = workspace("verif");
    cfg.sem_claude = true;
    let mut m = Motor::abrir(cfg.clone(), Arc::new(|| 1_800_000_000)).unwrap();
    m.tick().unwrap();
    std::fs::write(cfg.workspace.join(".schematize/overdev/CHECKLIST.md"), "- [ ] n\n").unwrap();
    m.tick().unwrap();
    drop(m);
    let p = cfg.dir_context().join("ULTIMO-checklist.md");
    std::fs::write(&p, b"corrompido").unwrap();
    std::fs::write(cfg.dir_context().join(".ULTIMO-checklist.md.tmp-999999-1"), b"x").unwrap();
    let r = verificar(&cfg, false).unwrap();
    assert!(
        r.ok && r.texto.contains("INVALIDA  ULTIMO-checklist.md") && r.texto.contains("TMP-ORFAO"),
        "{}",
        r.texto
    );
    let r = verificar(&cfg, true).unwrap();
    assert!(r.texto.contains("REPARADO") && r.texto.contains("LIMPO"), "{}", r.texto);
    assert!(ler_validado(&p).is_ok());
}

#[test]
fn rss_do_loop_cem_ticks_abaixo_de_6_mib() {
    let mut cfg = workspace("rss");
    cfg.sem_claude = true;
    let ws = std::env::var("HV_WS_REAL")
        .unwrap_or_else(|_| cfg.workspace.to_string_lossy().into_owned());
    let out = filho("rss", &[("HV_WS", &ws)]).output().expect("filho rss");
    let txt = String::from_utf8_lossy(&out.stdout);
    let kb: u64 = txt
        .lines()
        .find_map(|l| l.split("RSS_CRESCIMENTO_KB ").nth(1))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or_else(|| {
            panic!("filho sem medida: {txt} {}", String::from_utf8_lossy(&out.stderr))
        });
    eprintln!("RSS: crescimento do VmHWM em 100 ticks = {kb} kB");
    assert!(kb < 6 * 1024, "RSS cresceu {kb} kB (>= 6 MiB)");
}
