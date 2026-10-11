//! A aba EXECUÇÃO: executar, parar, acompanhar, dividir (multiagent) e a coluna Sistema.
//!
//! **O quê:** os controles do run e o monitor — tokens, itens abertos e conclusões recentes.
//!
//! **Onde:** a aba Execução; veio da aba Overdev do hub.
//!
//! ## O que abre TERMINAL e o que é comando rápido (D6)
//!
//! - **Executar** (`run`), **Dividir** e **Dividir + lançar** (`split K [--dispatch]`) → TERMINAL.
//!   Spawnam o `claude` ou reorganizam o checklist em arquivos; no terminal o comando aparece
//!   ANTES de rodar, o resultado é visível e o `Ctrl-C` funciona. Executar ainda passa por um
//!   guardrail que MOSTRA a linha de comando e só abre o terminal se a pessoa confirmar.
//! - **Parar** (`stop`) → comando rápido: encerra o run (os hooks voltam a ser inertes).
//! - **Reload / Acompanhar** NÃO spawna nada: liga um relógio de 3 s que relê `progresso` e
//!   `monitor` (baratos). Os tokens (`tokens --json`, que varre transcritos de 100 MB+) só rodam
//!   quando a pessoa pede, e SEMPRE numa thread à parte — nunca congelam a tela.

use super::{cli, comandos, leituras, ponte, Estado, MainWindow, Ov};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::path::PathBuf;
use std::rc::Rc;

/// **O quê:** relê a coluna Sistema (e o plano do split atual) por `agents --json --split K`.
///
/// **Onde:** a abertura, o «Reler», «Rechecar» e a mudança de K. **Falha aparece** na coluna: o
/// teto 0 de um documento não lido diria "máquina cheia".
pub fn atualizar(w: &MainWindow, e: &Estado) {
    let o = w.global::<Ov>();
    let k = o.get_split_k().max(2).to_string();
    let r = cli::rodar(&e.dir, &["agents", "--json", "--split", &k])
        .and_then(|t| leituras::ler_sistema(&t));
    match r {
        Ok(s) => {
            o.set_sis_teto(s.teto as i32);
            o.set_sis_livre(s.livre as i32);
            o.set_sis_rodando(s.rodando as i32);
            o.set_sis_load(s.load.clone().into());
            o.set_sis_erro(SharedString::new());
            o.set_split_k(leituras::prender_k(o.get_split_k() as i64, s.teto) as i32);
            o.set_split_plano(
                s.plano.map(|p| leituras::plano_em_texto(&p, s.teto)).unwrap_or_default().into(),
            );
        }
        Err(err) => {
            o.set_sis_teto(0);
            o.set_sis_livre(0);
            o.set_sis_rodando(0);
            o.set_sis_load("—".into());
            o.set_sis_erro(err.lines().next().unwrap_or("").to_string().into());
            o.set_split_plano(SharedString::new());
        }
    }
}

/// **O quê:** escreve o status da aba. **Onde:** as ações abaixo.
fn status(w: &MainWindow, msg: String, erro: bool) {
    w.global::<Ov>().set_exec_status(msg.into());
    w.global::<Ov>().set_exec_erro(erro);
}

/// **O quê:** relê o monitor leve (`monitor --json`) e escreve os itens abertos e as conclusões.
///
/// **Onde:** [`tick`] e a abertura. Falha vira o status da aba, não lista vazia.
pub fn atualizar_monitor(w: &MainWindow, e: &Estado) {
    let o = w.global::<Ov>();
    match cli::rodar(&e.dir, &["monitor", "--json"]).and_then(|t| leituras::ler_monitor(&t)) {
        Ok(m) => {
            let lista = |v: Vec<String>| {
                ModelRc::new(VecModel::from(
                    v.into_iter().map(SharedString::from).collect::<Vec<_>>(),
                ))
            };
            o.set_mon_abertos(lista(m.abertos));
            o.set_mon_conclusoes(lista(m.conclusoes));
        }
        Err(err) => status(w, err.lines().next().unwrap_or("").to_string(), true),
    }
}

/// **O quê:** lê os tokens numa THREAD e escreve a linha na tela quando chegar.
///
/// **Onde:** «Atualizar tokens» e o arranque do «Acompanhar». O `tokens --json` varre os
/// transcritos do Claude (100 MB+): no laço da tela congelaria a janela inteira.
fn tokens_em_thread(w: &MainWindow, dir: PathBuf) {
    let o = w.global::<Ov>();
    o.set_tokens_linha("Tokens: lendo os transcritos…".into());
    let weak = w.as_weak();
    std::thread::spawn(move || {
        let linha = cli::rodar(&dir, &["tokens", "--json"])
            .and_then(|t| leituras::linha_de_tokens(&t))
            .unwrap_or_else(|err| format!("Tokens: {}", err.lines().next().unwrap_or("")));
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(w) = weak.upgrade() {
                w.global::<Ov>().set_tokens_linha(linha.into());
            }
        });
    });
}

/// **O quê:** um ciclo do «Acompanhar»: progresso + monitor, e a releitura completa se algo mudou.
///
/// **Onde:** o `Timer` de 3 s do `.slint` (só roda com `acompanhando`).
///
/// **Para sozinho** quando o run termina — mas só DEPOIS de ter visto o run `active` uma vez: um
/// `state.json` velho ("stopped") não pode encerrar o acompanhamento antes de o overdev sequer
/// arrancar no terminal que acabou de abrir.
fn tick(w: &MainWindow, e: &Estado) {
    let o = w.global::<Ov>();
    let Ok(p) = crate::publicar_progresso(w, e) else { return };
    atualizar_monitor(w, e);
    let ativo = p.modo.as_deref() == Some("active");
    if ativo {
        e.visto_ativo.set(true);
    }
    let agora = (p.feitos, p.abertos);
    if e.contagem.replace(agora) != agora {
        // Algo andou: o checklist, a fila e a caixa mudaram junto.
        crate::recarregar(w, e);
    }
    if e.visto_ativo.get() && (!ativo || p.abertos == 0) {
        o.set_acompanhando(false);
        crate::recarregar(w, e);
        status(w, "o run terminou — acompanhamento encerrado.".into(), false);
    }
}

/// **O quê:** registra os callbacks da aba. **Onde:** [`super::main`], uma vez.
pub fn ligar(w: &MainWindow, e: &Rc<Estado>) {
    com_ov!(w, e, on_executar_pedir, |w, e| {
        // O guardrail MOSTRA o comando real: o mostrado e o executado vêm da mesma função.
        let bin = cli::bin().display().to_string();
        let linha = cli::linha_de_comando(&bin, &comandos::executar());
        let o = w.global::<Ov>();
        o.set_exec_cmd(format!("{linha}    (em {})", e.dir.display()).into());
        o.set_exec_confirmar(true);
    });
    com_ov!(w, e, on_executar_cancelar, |w, e| {
        let _ = e;
        w.global::<Ov>().set_exec_confirmar(false);
    });
    com_ov!(w, e, on_executar_confirmar, |w, e| {
        w.global::<Ov>().set_exec_confirmar(false);
        let (msg, erro) = ponte::no_terminal(e, &comandos::executar());
        status(&w, msg, erro);
        if !erro {
            // Passa a acompanhar o run que acabou de abrir, esperando ele ficar `active`.
            e.visto_ativo.set(false);
            w.global::<Ov>().set_acompanhando(true);
            tokens_em_thread(&w, e.dir.clone());
        }
    });
    com_ov!(w, e, on_parar, |w, e| {
        let args: Vec<String> = comandos::parar().iter().map(|s| s.to_string()).collect();
        let (msg, erro) = ponte::rodar(e, &args);
        w.global::<Ov>().set_acompanhando(false);
        status(&w, msg, erro);
        crate::recarregar(&w, e);
    });
    com_ov!(w, e, on_acompanhar, |w, e| {
        let o = w.global::<Ov>();
        if o.get_acompanhando() {
            // Segundo clique desliga: o botão é um interruptor.
            o.set_acompanhando(false);
            status(&w, "acompanhamento desligado.".into(), false);
        } else {
            // Anexa a um run que JÁ roda por fora: segue de imediato, sem esperar `active`.
            e.visto_ativo.set(true);
            e.contagem.set((u64::MAX, u64::MAX));
            o.set_acompanhando(true);
            status(&w, "acompanhando o overdev deste projeto…".into(), false);
            tick(&w, e);
            tokens_em_thread(&w, e.dir.clone());
        }
    });
    com_ov!(w, e, on_tick, |w, e| {
        tick(&w, e);
    });
    com_ov!(w, e, on_atualizar_tokens, |w, e| {
        tokens_em_thread(&w, e.dir.clone());
    });
    com_ov!(w, e, on_rechecar, |w, e| {
        atualizar(&w, e);
    });
    com_ov!(w, e, on_split_mudar, |w, e, delta| {
        let o = w.global::<Ov>();
        let k = leituras::prender_k(o.get_split_k() as i64 + delta as i64, o.get_sis_teto() as i64);
        o.set_split_k(k as i32);
        atualizar(&w, e);
    });
    com_ov!(w, e, on_dividir, |w, e, dispatch| {
        let o = w.global::<Ov>();
        let (k, teto, livre) =
            (o.get_split_k() as i64, o.get_sis_teto() as i64, o.get_sis_livre() as i64);
        // Sem teto lido não dá para checar nada — e dividir às cegas é o que o governador evita.
        let bloqueio = if teto <= 0 {
            Some("não consegui ler o teto da máquina — clique em «Rechecar».".to_string())
        } else {
            leituras::bloqueio_do_split(k, dispatch, teto, livre)
        };
        let (msg, erro) = match bloqueio {
            Some(b) => (b, true),
            None => {
                let args = comandos::dividir(k, dispatch);
                let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                ponte::no_terminal(e, &refs)
            }
        };
        o.set_split_status(msg.into());
        o.set_split_erro(erro);
    });
}
