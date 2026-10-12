//! As MENSAGENS da janela — tudo que pode acontecer, como dados.
//!
//! **O quê:** [`Msg`] (o evento), [`Aba`] (as seis abas) e [`Origem`] (de qual ação veio a
//! resposta de um comando do CLI).
//!
//! **Onde:** [`super::update`] consome; as telas (`view`) produzem. É a arquitetura Elm do iced:
//! a tela só EMITE `Msg`, e o `update` é quem muda o estado — por isso testável sem janela.

use super::carga::Carga;
use super::projeto::Project;
use super::sino::Notif;
use iced::widget::text_editor;
use std::sync::Arc;

/// As abas, na ordem em que aparecem. **Onde:** o cabeçalho e o `--aba`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aba {
    /// A tela de projeto do hub: iniciar overdev, ações de skills, commits.
    Projeto,
    Perguntas,
    Checklist,
    Historico,
    Execucao,
    Caixa,
    Notas,
}

/// De qual ação veio uma resposta de comando (`Msg::Resp`): decide ONDE a mensagem aparece e o
/// que se relê depois. **Onde:** [`super::tarefa`] marca; [`super::update`] decide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origem {
    /// `reply`/`refuse` de uma pergunta: a mensagem do CLI vai à aba Perguntas.
    Quiz,
    /// `human --done`: a faixa da aba Checklist.
    Item,
    /// `answer`/`refuse` do modal de resolver: erro fica no modal, sucesso vira faixa.
    Resolver,
    /// «Abrir no terminal» do Executar: sucesso passa a acompanhar o run.
    Executar,
    /// «Parar».
    Parar,
    /// «Dividir» / «Dividir + lançar» (terminal).
    Dividir,
    /// «Capturar» da Caixa: limpa o campo só se entrou.
    CaixaCapturar,
    /// «Organizar com agente»: sem terminal o CLI sai 0 mas a janela trata como falha.
    CaixaAgente,
    /// «Fundir no checklist».
    CaixaFundir,
    /// «Anexar» nota (`true` = prompt de correção).
    Nota(bool),
    /// «Abrir no editor» da aba Notas.
    Arquivo,
    /// «Restaurar» um snapshot (terminal): a mensagem vai ao topo da janela.
    Restaurar,
    /// Resultado de uma ação da aba Projeto (skill, afazeres, painel, terminal, rerun).
    Home,
    /// `start`: com sucesso relê tudo e limpa o campo do objetivo.
    Iniciar,
    /// Ações do topo (pasta, editor, nova janela, fixar): vira o aviso do topo.
    Topo,
    /// `schematize projects add|remove`: aviso no modal e relista os projetos.
    Fixar,
    /// `schematize notifications --concluir`: aviso no painel e relê a lista.
    Sino,
}

/// Tudo que pode acontecer na janela.
#[derive(Debug, Clone)]
pub enum Msg {
    // ---- janela ----
    Aba(Aba),
    Reler,
    AlternarTema,
    /// A leitura completa (ou parcial) de um projeto chegou.
    Carga(Arc<Carga>),
    /// A leitura leve do ciclo do «Acompanhar» chegou.
    CargaTick(Arc<Carga>),
    /// Um comando do CLI terminou: `(origem, mensagem, deu_erro)`.
    Resp(Origem, String, bool),

    // ---- perguntas ----
    Responder(String, String),
    /// Texto digitado no campo «outro» da pergunta `id`.
    Outro(String, String),
    ResponderTexto(String),
    Marcar(String, String),
    ConfirmarMultipla(String),
    MarcarFeito(String),
    Recusar(String),

    // ---- checklist ----
    Filtro(i32),
    Pagina(usize),
    ItemFeito(usize),
    ResolverPedir(usize, String, bool),
    ResolverEditar(text_editor::Action),
    ResolverCancelar,
    ResolverConfirmar,

    // ---- histórico ----
    Restaurar(i64),

    // ---- execução ----
    ExecutarPedir,
    ExecutarCancelar,
    ExecutarConfirmar,
    Parar,
    Acompanhar,
    /// O relógio do «Acompanhar» disparou (o número é o `ciclo` que o agendou).
    Tick(u64),
    AtualizarTokens,
    Tokens(String),
    Rechecar,
    SplitMudar(i64),
    Dividir(bool),

    // ---- caixa ----
    CaixaEditar(text_editor::Action),
    CaixaAdicionar,
    CaixaAgente,
    CaixaMesclar,

    // ---- notas ----
    AbrirArquivo(String),
    NotaTask(String),
    NotaCorrecao(String),
    NotaAdicionar(bool),

    // ---- seletor de projeto ----
    AbrirSeletor,
    FecharSeletor,
    /// A lista de projetos (fixados primeiro) chegou.
    Projetos(Vec<Project>),
    SeletorCaminho(String),
    /// «Abrir» o caminho digitado.
    SeletorAbrirCaminho,
    /// Escolheu um projeto da lista.
    SeletorEscolher(String),

    // ---- cabeçalho, aba Projeto e sino ----
    Topo(TopoMsg),
    Home(HomeMsg),
    Sino(SinoMsg),
}

/// O seletor de projeto do topo escolhido: um projeto da lista, ou a entrada «Outro caminho…».
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opcao {
    pub rotulo: String,
    /// Vazio = a entrada «Outro caminho…» (abre o campo de caminho).
    pub caminho: String,
}

impl std::fmt::Display for Opcao {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.rotulo)
    }
}

/// O cabeçalho: seletor, os cinco ícones, a versão do hub e o gerenciador de projetos fixados.
#[derive(Debug, Clone)]
pub enum TopoMsg {
    Escolher(Opcao),
    /// O seletor abriu: o filtro zera para mostrar TODOS os projetos.
    ComboAbrir,
    /// O seletor fechou sem escolher: volta a mostrar o projeto atual.
    ComboFechar,
    AbrirPasta,
    AbrirCodigo,
    NovaJanela,
    /// A versão do hub (`schematize --version`) chegou.
    Versao(Result<String, String>),
    GerenciarAbrir,
    GerenciarFechar,
    GerenciarCaminho(String),
    Fixar,
    Desafixar(String),
}

/// A aba Projeto (a tela de Overdev do hub + «Iniciar overdev»).
#[derive(Debug, Clone)]
pub enum HomeMsg {
    Objetivo(String),
    Iniciar,
    IniciarConfirmar,
    IniciarCancelar,
    /// O botão de ação de skill de índice `i` (de `schematize-skills acoes`).
    Skill(usize),
    GerarAfazeres,
    Navegador,
    Terminal,
    Recarregar,
    Rerodar,
    Pagina(usize),
}

/// O sino de notificações.
#[derive(Debug, Clone)]
pub enum SinoMsg {
    Alternar,
    /// A lista chegou (ou o motivo de não ter chegado).
    Carregado(Result<Vec<Notif>, String>),
    /// O relógio de 90 s: sincroniza e relê.
    Tick,
    Concluir(String),
    /// `(kind, ação)` do item.
    Acao(String, String),
}
