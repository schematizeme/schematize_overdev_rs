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

/// O que a PLATAFORMA faz por fora do CLI do overdev: abrir pasta, editor, janela nova, link.
#[derive(Debug, Clone)]
pub enum Plataforma {
    Pasta(PathBuf),
    Codigo(PathBuf),
    NovaJanela(PathBuf),
    Url(String),
}

/// **O quê:** executa uma ação de [`Plataforma`] fora da thread da tela. **Onde:** os ícones do topo
/// e a ação de notícia do sino; o resultado volta como `Msg::Resp(origem, …)`.
pub fn plataforma(p: Plataforma, origem: Origem) -> Task<Msg> {
    Task::perform(
        async move {
            match p {
                Plataforma::Pasta(d) => super::terminal::abrir_pasta(&d),
                Plataforma::Codigo(d) => super::terminal::abrir_vscode(&d),
                Plataforma::NovaJanela(d) => super::terminal::nova_janela(&d),
                Plataforma::Url(u) => super::terminal::abrir_url(&u),
            }
        },
        move |r| match r {
            Ok(m) => Msg::Resp(origem, m, false),
            Err(m) => Msg::Resp(origem, m, true),
        },
    )
}

/// **O quê:** `schematize-overdev terminal --json …` e traduz o `{"terminal": "<nome>"}` numa frase.
/// **Onde:** as ações da aba Projeto que abrem o `claude` num terminal.
pub fn terminal_json(dir: PathBuf, args: Vec<String>, rotulo: String, origem: Origem) -> Task<Msg> {
    Task::perform(
        async move {
            match super::cli::rodar(&dir, &args).and_then(|t| super::externos::ler_terminal(&t)) {
                Ok(t) => (format!("{rotulo}: terminal `{t}` aberto em {}", dir.display()), false),
                Err(e) => (ponte::primeira_linha(&e), true),
            }
        },
        move |(m, e)| Msg::Resp(origem, m, e),
    )
}

/// **O quê:** um terminal que roda `bin args…` (o comando aparece antes de rodar). **Onde:** o
/// «Atualizar» do sino.
pub fn terminal_de(bin: String, dir: PathBuf, args: Vec<String>, origem: Origem) -> Task<Msg> {
    Task::perform(
        async move {
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            ponte::no_terminal_de(&bin, &dir, &refs)
        },
        move |(m, e)| Msg::Resp(origem, m, e),
    )
}

/// **O quê:** roda um subcomando do `schematize` (o hub, não o overdev) e devolve a primeira linha.
/// **Onde:** concluir notificação e fixar/desafixar projeto.
pub fn schematize(dir: PathBuf, args: Vec<String>, origem: Origem) -> Task<Msg> {
    Task::perform(
        async move {
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            match super::externos::rodar("schematize", &dir, &refs) {
                Ok(t) => (ponte::primeira_linha(&t), false),
                Err(e) => (ponte::primeira_linha(&e), true),
            }
        },
        move |(m, e)| Msg::Resp(origem, m, e),
    )
}

/// **O quê:** lê o sino: sincroniza (rede) e/ou marca como lidas, e SEMPRE relê a lista.
/// **Onde:** a abertura, o painel, o relógio de 90 s e depois de concluir. Falha de qualquer passo
/// vira o `Err` da lista (o painel mostra) — nada é engolido.
pub fn sino(dir: PathBuf, sincronizar: bool, marcar: bool) -> Task<Msg> {
    Task::perform(
        async move {
            let rodar = |args: &[&str]| super::externos::rodar("schematize", &dir, args);
            if sincronizar {
                rodar(&["notifications", "--sync"])?;
            }
            if marcar {
                rodar(&["notifications", "--lidas"])?;
            }
            let t = rodar(&["notifications", "--historico"])?;
            super::sino::ler_notificacoes(&t)
        },
        |r| Msg::Sino(super::msg::SinoMsg::Carregado(r)),
    )
}

/// **O quê:** o relógio de 90 s do sino. **Onde:** depois de cada leitura sincronizada.
pub fn espera_sino() -> Task<Msg> {
    Task::perform(esperar(Duration::from_secs(90)), |()| Msg::Sino(super::msg::SinoMsg::Tick))
}

/// **O quê:** a versão do hub (`schematize --version`). **Onde:** a abertura.
pub fn versao_do_hub(dir: PathBuf) -> Task<Msg> {
    Task::perform(
        async move {
            super::externos::rodar("schematize", &dir, &["--version"])
                .and_then(|t| super::externos::ler_versao_do_hub(&t))
        },
        |r| Msg::Topo(super::msg::TopoMsg::Versao(r)),
    )
}

/// **O quê:** relê a lista de projetos do seletor. **Onde:** a abertura e depois de fixar.
pub fn projetos(atual: PathBuf) -> Task<Msg> {
    Task::perform(async move { super::projeto::carregar_lista(&atual) }, Msg::Projetos)
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
