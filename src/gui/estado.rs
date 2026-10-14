//! O ESTADO da janela: tudo que ela guarda entre um clique e outro, e como uma [`Carga`] o preenche.
//!
//! **O quê:** o [`App`] (o estado inteiro), o [`Aviso`] (uma linha de status verde/âmbar) e
//! [`App::aplicar_carga`], que transforma o que o binário disse em campos da tela.
//!
//! **Onde:** [`super::update`] muda; [`super::view`] e as abas leem. Substitui o `Estado` + as
//! propriedades do `MainWindow` do Slint, que viviam em dois mundos ligados por callbacks.
//!
//! ## `dir` é um `PathBuf` simples, e trocar de projeto REINICIA tudo
//!
//! O `dir` é lido em ~18 lugares (cada ação roda o CLI nele). Trocar de projeto não é só mudar o
//! campo: cards, checklist, marcas, acompanhamento e status do projeto ANTIGO não podem vazar para
//! o novo. [`App::trocar_projeto`] reconstrói o estado e deixa a releitura para o `update`.

use super::agentes_estado::EstadoAgentes;
use super::caixa::EstadoCaixa;
use super::carga::{Carga, Escopo};
use super::execucao::EstadoExec;
use super::home::Home;
use super::msg::Aba;
use super::notas::EstadoNotas;
use super::projeto::Seletor;
use super::resolver::Resolver;
use super::sino::Sino;
use super::tema::Cores;
use super::topo::Topo;
use super::{leituras, telas};
use iced::Theme;
use std::collections::HashMap;
use std::path::PathBuf;

/// Uma linha de status: o texto e se é erro (âmbar) ou sucesso (verde). Vazio = não aparece.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Aviso {
    pub texto: String,
    pub erro: bool,
}

impl Aviso {
    /// **O quê:** um aviso de sucesso. **Onde:** o `update`, depois de uma ação que deu certo.
    pub fn ok(texto: impl Into<String>) -> Aviso {
        Aviso { texto: texto.into(), erro: false }
    }

    /// **O quê:** um aviso de erro. **Onde:** o `update`, quando algo foi recusado ou falhou.
    pub fn falha(texto: impl Into<String>) -> Aviso {
        Aviso { texto: texto.into(), erro: true }
    }

    /// **O quê:** o aviso a partir de `(mensagem, deu_erro)`. **Onde:** o resultado de [`super::ponte`].
    pub fn de(texto: String, erro: bool) -> Aviso {
        Aviso { texto, erro }
    }
}

/// A primeira linha de `t`. **Onde:** os erros do CLI, que vêm com `Usage:` e dicas que não cabem
/// num banner.
pub fn primeira(t: &str) -> String {
    t.lines().next().unwrap_or("").to_string()
}

/// O estado inteiro da janela.
pub struct App {
    /// O projeto: `--dir`, o último escolhido, ou o diretório corrente.
    pub dir: PathBuf,
    /// Onde o app guarda o estado (o último projeto). Injetado: o teste usa um temporário.
    pub dados: PathBuf,
    /// `schematize-overdev-gui x.y.z (sha12)`.
    pub versao: String,
    pub cores: Cores,
    pub tema: Theme,
    pub aba: Aba,

    // ---- cabeçalho: o progresso do run, sempre à vista ----
    pub objetivo: String,
    pub modo: String,
    pub resumo: String,
    pub feitos: u64,
    pub abertos: u64,
    pub iteracoes: u64,
    pub teto: u64,
    /// Erros de leitura do progresso/checklist, uma linha cada.
    pub erro: String,
    /// A mensagem verde do topo (ex.: «terminal aberto — clique em «Reler»…»).
    pub msg: String,
    /// Há run neste diretório? Desliga Executar/Parar/Dividir.
    pub tem_run: bool,

    // ---- perguntas ----
    pub cards: Vec<telas::Card>,
    /// O que a pessoa marcou num `multipla`, entre o clique e o Confirmar.
    pub marcadas: HashMap<String, Vec<String>>,
    /// O texto do campo «outro»/«livre» de cada pergunta.
    pub outros: HashMap<String, String>,
    pub quiz: Aviso,

    // ---- checklist ----
    /// O checklist COMPLETO; só uma página vai à tela.
    pub itens: Vec<telas::Item>,
    pub filtro: i32,
    pub pagina: usize,
    pub item_status: Aviso,
    pub resolver: Option<Resolver>,

    // ---- histórico ----
    pub snaps: Vec<telas::Snapshot>,
    pub snaps_erro: String,

    pub exec: EstadoExec,
    pub caixa: EstadoCaixa,
    pub notas: EstadoNotas,
    pub agentes: EstadoAgentes,
    pub seletor: Seletor,
    pub home: Home,
    pub sino: Sino,
    pub topo: Topo,
}

impl App {
    /// **O quê:** o estado de abertura para `dir`. PURO: não lê nada do disco.
    ///
    /// **Onde:** [`super::iniciar`] e os testes. `dados` é a pasta de estado do app.
    pub fn novo(dir: PathBuf, dados: PathBuf, versao: String, aba: Aba, escuro: bool) -> App {
        let cores = Cores::de(escuro);
        App {
            dir,
            dados,
            versao,
            tema: cores.tema(),
            cores,
            aba,
            objetivo: String::new(),
            modo: String::new(),
            resumo: String::new(),
            feitos: 0,
            abertos: 0,
            iteracoes: 0,
            teto: 0,
            erro: String::new(),
            msg: String::new(),
            tem_run: false,
            cards: Vec::new(),
            marcadas: HashMap::new(),
            outros: HashMap::new(),
            quiz: Aviso::default(),
            itens: Vec::new(),
            filtro: 0,
            pagina: 0,
            item_status: Aviso::default(),
            resolver: None,
            snaps: Vec::new(),
            snaps_erro: String::new(),
            exec: EstadoExec::default(),
            caixa: EstadoCaixa::default(),
            notas: EstadoNotas::default(),
            agentes: EstadoAgentes::default(),
            seletor: Seletor::default(),
            home: Home::default(),
            sino: Sino::default(),
            topo: Topo::default(),
        }
    }

    /// **O quê:** passa a olhar para `dir`, descartando TUDO do projeto anterior.
    ///
    /// **Onde:** [`super::projeto`], depois de validar e persistir a escolha. Mantém o que não é do
    /// projeto: o tema, a aba aberta, a versão e a pasta de estado.
    pub fn trocar_projeto(&mut self, dir: PathBuf) {
        let escuro = self.cores.escuro;
        let mut novo = App::novo(dir, self.dados.clone(), self.versao.clone(), self.aba, escuro);
        // Os relógios do «Acompanhar» do projeto ANTIGO ainda podem chegar: o ciclo segue
        // crescendo para que nenhum deles case com um acompanhamento futuro.
        novo.exec.ciclo = self.exec.ciclo + 1;
        // O que NÃO é do projeto fica: a lista do seletor, o sino e a versão do hub.
        novo.seletor = self.seletor.fechado();
        let atual = novo.dir.clone();
        novo.seletor.refazer(&atual);
        novo.sino = std::mem::take(&mut self.sino);
        novo.topo.versao_hub = std::mem::take(&mut self.topo.versao_hub);
        *self = novo;
    }

    /// **O quê:** quantas perguntas esperam a pessoa. **Onde:** a aba e o cabeçalho da fila.
    pub fn quiz_abertas(&self) -> usize {
        self.cards.iter().filter(|c| c.aberta).count()
    }

    /// **O quê:** quantas perguntas esperam a revisão da máquina. **Onde:** o cabeçalho da fila.
    pub fn quiz_revisar(&self) -> usize {
        self.cards.iter().filter(|c| c.aguarda_revisao).count()
    }

    /// **O quê:** aplica uma [`Carga`] ao estado. **Onde:** `Msg::Carga` e `Msg::CargaTick`.
    ///
    /// **Carga de OUTRO projeto é descartada** (a pessoa trocou no meio da leitura). **Cada erro vai
    /// para o lugar dele e a falha de um documento não apaga os outros**: o do progresso e do
    /// checklist no topo, o da fila na aba Perguntas, etc. — como o `recarregar` do Slint.
    pub fn aplicar_carga(&mut self, c: &Carga) {
        if c.dir != self.dir {
            return;
        }
        let completa = c.escopo.tem(Escopo::CHECKLIST);
        let mut erros: Vec<String> = Vec::new();
        if let Some(r) = &c.progresso {
            if let Err(e) = self.aplicar_progresso(r) {
                erros.push(e);
            }
        }
        if let Some(r) = &c.checklist {
            match r {
                Ok(ck) => {
                    self.objetivo = ck.objetivo.clone().unwrap_or_default();
                    self.itens = ck.itens.clone();
                }
                Err(e) => {
                    self.objetivo.clear();
                    self.itens.clear();
                    erros.push(e.clone());
                }
            }
            self.pagina = self.pagina.min(telas::paginas(self.total_filtrado()) - 1);
        }
        if completa {
            self.erro = erros.join("\n");
        }
        if let Some(r) = &c.fila {
            self.aplicar_fila(r);
        }
        if let Some(r) = &c.historico {
            match r {
                Ok(h) => {
                    self.snaps = h.clone();
                    self.snaps_erro.clear();
                }
                Err(e) => {
                    self.snaps.clear();
                    self.snaps_erro = e.clone();
                }
            }
        }
        if let Some(r) = &c.sistema {
            self.aplicar_sistema(r);
        }
        if let Some(r) = &c.monitor {
            self.aplicar_monitor(r);
        }
        if let Some(r) = &c.caixa {
            self.aplicar_caixa(r);
        }
        if let Some(r) = &c.arquivos {
            self.aplicar_arquivos(r);
        }
        if let Some(r) = &c.notas {
            self.aplicar_notas(r);
        }
        if let Some(h) = &c.home {
            self.aplicar_home(h);
        }
    }

    /// **O quê:** escreve o cabeçalho (resumo, modo, contagens, ciclos) a partir do progresso.
    ///
    /// **Onde:** [`App::aplicar_carga`]. **Sem run (`modo` nulo) `tem_run` fica `false`**, que
    /// desliga Executar/Parar/Dividir. O erro volta ao chamador, e `tem_run` cai para `false`
    /// também: ação sobre um run que não se conseguiu ler não é ação que se deva oferecer.
    pub fn aplicar_progresso(
        &mut self,
        r: &Result<telas::Progresso, String>,
    ) -> Result<(), String> {
        match r {
            Ok(p) => {
                self.resumo = telas::resumo(p);
                self.modo = p.modo.clone().unwrap_or_default();
                self.feitos = p.feitos;
                self.abertos = p.abertos;
                self.iteracoes = p.iteracoes;
                self.teto = p.teto.unwrap_or(0);
                self.tem_run = p.modo.is_some();
                Ok(())
            }
            Err(e) => {
                self.resumo.clear();
                self.modo.clear();
                self.tem_run = false;
                Err(e.clone())
            }
        }
    }

    /// **O quê:** escreve a fila de perguntas. **Onde:** [`App::aplicar_carga`].
    ///
    /// **ERRO APARECE, e a lista não é "vazia calada":** "nenhuma pergunta" sobre uma falha de
    /// leitura diria a quem tem cinco esperando que não há nada.
    fn aplicar_fila(&mut self, r: &Result<Vec<telas::Card>, String>) {
        match r {
            Ok(c) => self.cards = c.clone(),
            Err(e) => {
                self.cards.clear();
                self.quiz = Aviso::falha(e.clone());
            }
        }
    }

    /// **O quê:** quantos itens passam pelo filtro atual. **Onde:** a paginação do checklist.
    pub fn total_filtrado(&self) -> usize {
        self.itens.iter().filter(|i| telas::casa(&i.tipo, self.filtro)).count()
    }

    /// **O quê:** escreve a coluna Sistema (e o plano do split). **Onde:** [`App::aplicar_carga`].
    ///
    /// **Falha aparece** na coluna: o teto 0 de um documento não lido diria "máquina cheia".
    fn aplicar_sistema(&mut self, r: &Result<leituras::Sistema, String>) {
        let x = &mut self.exec;
        match r {
            Ok(s) => {
                x.sis_teto = s.teto;
                x.sis_livre = s.livre;
                x.sis_rodando = s.rodando;
                x.sis_load = s.load.clone();
                x.sis_erro.clear();
                x.split_k = leituras::prender_k(x.split_k, s.teto);
                x.split_plano = s
                    .plano
                    .as_ref()
                    .map(|p| leituras::plano_em_texto(p, s.teto))
                    .unwrap_or_default();
            }
            Err(e) => {
                x.sis_teto = 0;
                x.sis_livre = 0;
                x.sis_rodando = 0;
                x.sis_load = "—".to_string();
                x.sis_erro = primeira(e);
                x.split_plano.clear();
            }
        }
    }

    /// **O quê:** escreve os itens abertos e as conclusões do monitor. **Onde:** a carga.
    /// Falha vira o status da aba Execução, não lista vazia.
    fn aplicar_monitor(&mut self, r: &Result<leituras::Monitor, String>) {
        match r {
            Ok(m) => {
                self.exec.mon_abertos = m.abertos.clone();
                self.exec.mon_conclusoes = m.conclusoes.clone();
            }
            Err(e) => self.exec.status = Aviso::falha(primeira(e)),
        }
    }

    /// **O quê:** escreve as duas listas da caixa. **Onde:** a carga.
    ///
    /// **ERRO APARECE:** uma caixa que não leu NÃO é uma caixa vazia.
    fn aplicar_caixa(&mut self, r: &Result<leituras::Caixa, String>) {
        match r {
            Ok(c) => {
                self.caixa.pendentes = c.pendentes.clone();
                self.caixa.prontas = c.processadas.clone();
            }
            Err(e) => {
                self.caixa.pendentes.clear();
                self.caixa.prontas.clear();
                self.caixa.status = Aviso::falha(primeira(e));
            }
        }
    }

    /// **O quê:** escreve a lista de documentos do run. **Onde:** a carga.
    fn aplicar_arquivos(&mut self, r: &Result<Vec<leituras::Arquivo>, String>) {
        match r {
            Ok(a) => self.notas.arquivos = a.clone(),
            Err(e) => {
                self.notas.arquivos.clear();
                self.notas.status = Aviso::falha(primeira(e));
            }
        }
    }

    /// **O quê:** escreve o texto das notas já anexadas. **Onde:** a carga.
    fn aplicar_notas(&mut self, r: &Result<String, String>) {
        match r {
            Ok(t) => self.notas.texto = t.trim().to_string(),
            Err(e) => {
                self.notas.texto.clear();
                self.notas.status = Aviso::falha(primeira(e));
            }
        }
    }
}
