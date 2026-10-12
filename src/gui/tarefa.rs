//! As TAREFAS assíncronas da janela: ação do CLI, abertura do terminal, espera e carga.
//!
//! **O quê:** envolve cada efeito (subprocesso, relógio) numa [`iced::Task`] que devolve uma
//! mensagem — o `update` nunca bloqueia e nunca spawna nada por conta própria.
//!
//! **Onde:** [`super::update`] e os `impl App` de cada aba.
//!
//! ## Por que o `update` é testável
//!
//! O `update` só MUDA ESTADO e DEVOLVE tarefas; o efeito acontece quando o iced executa a tarefa.
//! O teste chama `update` e olha o estado e `Task::units()` (0 = nenhum efeito pedido, 1 = um).
//! Nenhuma tarefa é executada no teste, então nenhum binário é chamado e nenhum terminal abre.

use super::carga::{self, Carga, Escopo};
use super::msg::{Msg, Origem};
use super::ponte;
use iced::Task;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// **O quê:** roda `<bin> <args>` no projeto e devolve `Msg::Resp(origem, mensagem, erro)`.
///
/// **Onde:** as ações RÁPIDAS (responder, recusar, fechar item, capturar, fundir, anotar, parar).
pub fn cli(dir: PathBuf, args: Vec<String>, origem: Origem) -> Task<Msg> {
    Task::perform(async move { ponte::rodar(&dir, &args) }, move |(m, e)| Msg::Resp(origem, m, e))
}

/// **O quê:** abre `<bin> <args>` no TERMINAL e devolve `Msg::Resp(origem, mensagem, erro)`.
///
/// **Onde:** Executar, Dividir, Restaurar (D6: o que muda estado vai ao terminal).
pub fn terminal(dir: PathBuf, args: Vec<String>, origem: Origem) -> Task<Msg> {
    Task::perform(
        async move {
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            ponte::no_terminal(&dir, &refs)
        },
        move |(m, e)| Msg::Resp(origem, m, e),
    )
}

/// **O quê:** lê os documentos de `escopo` e devolve `Msg::Carga` (ou `Msg::CargaTick`).
///
/// **Onde:** a abertura, o «Reler», a troca de projeto, e depois de cada ação.
pub fn carregar(dir: PathBuf, escopo: Escopo, k: i64, tick: bool) -> Task<Msg> {
    Task::perform(
        async move { Arc::new(carga::carregar(&dir, escopo, k)) },
        move |c: Arc<Carga>| {
            if tick {
                Msg::CargaTick(c)
            } else {
                Msg::Carga(c)
            }
        },
    )
}

/// **O quê:** lê os tokens (varre transcritos de 100 MB+) e devolve `Msg::Tokens(linha)`.
///
/// **Onde:** «Atualizar tokens» e o arranque do «Acompanhar». É uma tarefa à parte: nunca congela.
pub fn tokens(dir: PathBuf) -> Task<Msg> {
    Task::perform(
        async move {
            super::cli::rodar(&dir, &["tokens", "--json"])
                .and_then(|t| super::leituras::linha_de_tokens(&t))
                .unwrap_or_else(|err| format!("Tokens: {}", err.lines().next().unwrap_or("")))
        },
        Msg::Tokens,
    )
}

/// **O quê:** abre um arquivo no editor e devolve `Msg::Resp(Origem::Arquivo, ..)`.
///
/// **Onde:** «Abrir no editor» da aba Notas.
pub fn abrir_arquivo(caminho: String) -> Task<Msg> {
    Task::perform(async move { super::terminal::abrir_arquivo(&caminho) }, |r| match r {
        Ok(m) => Msg::Resp(Origem::Arquivo, m, false),
        Err(m) => Msg::Resp(Origem::Arquivo, m, true),
    })
}

/// **O quê:** espera `d` SEM ocupar uma thread do executor: uma thread própria dorme e depois
/// larga o emissor de um canal de uma posição — o receptor acorda com `Canceled`, que AQUI é o aviso.
///
/// **Onde:** o relógio de 3 s do «Acompanhar» ([`espera_tick`]). O `thread-pool` do iced não traz
/// `time::every`; criar uma thread por ciclo (a cada 3 s, só enquanto acompanha) custa menos que
/// uma dependência de relógio assíncrono.
pub async fn esperar(d: Duration) {
    let (tx, rx) = iced::futures::channel::oneshot::channel::<()>();
    std::thread::spawn(move || {
        std::thread::sleep(d);
        drop(tx);
    });
    rx.await.unwrap_or_default();
}

/// **O quê:** agenda o PRÓXIMO tick do «Acompanhar» para daqui a 3 s, marcado com o `ciclo`.
///
/// **Onde:** ao ligar o acompanhamento e ao fim de cada tick. O `ciclo` invalida o relógio antigo
/// quando a pessoa desliga e religa depressa (dois relógios dobrariam a leitura).
pub fn espera_tick(ciclo: u64) -> Task<Msg> {
    Task::perform(esperar(Duration::from_secs(3)), move |()| Msg::Tick(ciclo))
}
