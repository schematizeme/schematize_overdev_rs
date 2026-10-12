//! A aba EXECUÇÃO: executar, parar, acompanhar, dividir (multiagent) e a coluna Sistema.
//!
//! **O quê:** os controles do run e o monitor — tokens, itens abertos e conclusões recentes.
//!
//! **Onde:** a aba Execução; veio da aba Overdev do hub.
//!
//! ## O que abre TERMINAL e o que é comando rápido (D6)
//!
//! - **Executar** (`run`), **Dividir** e **Dividir + lançar** (`split K [--dispatch]`) → TERMINAL
//!   do sistema. Spawnam o `claude` ou reorganizam o checklist em arquivos; no terminal o comando
//!   aparece ANTES de rodar, o resultado é visível e o `Ctrl-C` funciona. Executar ainda passa por
//!   um guardrail que MOSTRA a linha de comando e só abre o terminal se a pessoa confirmar.
//! - **Parar** (`stop`) → comando rápido: encerra o run (os hooks voltam a ser inertes).
//! - **Reload / Acompanhar** NÃO spawna nada: liga um relógio de 3 s que relê `progresso` e
//!   `monitor` (baratos). Os tokens (`tokens --json`, que varre transcritos de 100 MB+) só rodam
//!   quando a pessoa pede, e SEMPRE numa tarefa à parte — nunca congelam a tela.
//!
//! ## Não há terminal embutido (e nunca houve nesta janela)
//!
//! A janela Slint também abria o terminal do sistema; o `portable-pty` é do SUPERVISOR do binário
//! headless (`agentrun`), não da janela. Esta é a mesma decisão (D6), portada fielmente.

use super::estado::{App, Aviso};
use super::msg::{Msg, Origem};
use super::widgets::{btn, cartao, negrito, rolagem, status, txt};
use super::{carga::Escopo, cli, comandos, leituras, tarefa};
use iced::widget::{column, container, row, Space};
use iced::{Alignment, Color, Element, Fill, Font};

/// O estado da aba Execução.
pub struct EstadoExec {
    pub status: Aviso,
    /// O guardrail de «Executar»: `Some(linha)` = mostra a linha de comando EXATA e pede confirmação.
    pub confirmar: Option<String>,
    pub acompanhando: bool,
    /// O «Acompanhar» já viu o run `active`? Só depois disso um run parado o encerra.
    pub visto_ativo: bool,
    /// `(feitos, abertos)` do último ciclo: mudou, relê o resto.
    pub contagem: (u64, u64),
    /// Identifica o relógio vivo; religar invalida o anterior.
    pub ciclo: u64,
    pub tokens_linha: String,
    pub mon_abertos: Vec<String>,
    pub mon_conclusoes: Vec<String>,
    pub sis_teto: i64,
    pub sis_livre: i64,
    pub sis_rodando: i64,
    pub sis_load: String,
    pub sis_erro: String,
    /// O K do split (número de claudes), preso ao teto da máquina.
    pub split_k: i64,
    pub split_plano: String,
    pub split: Aviso,
}

impl Default for EstadoExec {
    fn default() -> Self {
        EstadoExec {
            status: Aviso::default(),
            confirmar: None,
            acompanhando: false,
            visto_ativo: false,
            contagem: (0, 0),
            ciclo: 0,
            tokens_linha: String::new(),
            mon_abertos: Vec::new(),
            mon_conclusoes: Vec::new(),
            sis_teto: 0,
            sis_livre: 0,
            sis_rodando: 0,
            sis_load: String::new(),
            sis_erro: String::new(),
            split_k: 2,
            split_plano: String::new(),
            split: Aviso::default(),
        }
    }
}

/// A linha mostrada enquanto os tokens são lidos. **Onde:** [`App::ler_tokens`].
const LENDO_TOKENS: &str = "Tokens: lendo os transcritos…";

impl App {
    /// **O quê:** pede a leitura dos tokens numa tarefa e põe o aviso de "lendo". **Onde:** o
    /// arranque do «Acompanhar» e «Atualizar tokens».
    fn ler_tokens(&mut self) -> iced::Task<Msg> {
        self.exec.tokens_linha = LENDO_TOKENS.to_string();
        tarefa::tokens(self.dir.clone())
    }

    /// **O quê:** desliga o «Acompanhar» e invalida o relógio vivo. **Onde:** Parar, fim do run,
    /// e o segundo clique no interruptor.
    fn desligar_acompanhar(&mut self) {
        self.exec.acompanhando = false;
        self.exec.ciclo += 1;
    }

    /// **O quê:** trata as mensagens da aba Execução. **Onde:** [`App::update`].
    pub fn atualizar_execucao(&mut self, m: Msg) -> iced::Task<Msg> {
        match m {
            Msg::ExecutarPedir => {
                // O guardrail MOSTRA o comando real: o mostrado e o executado vêm da mesma função.
                let bin = cli::bin().display().to_string();
                let linha = cli::linha_de_comando(&bin, &comandos::executar());
                self.exec.confirmar = Some(format!("{linha}    (em {})", self.dir.display()));
                iced::Task::none()
            }
            Msg::ExecutarCancelar => {
                self.exec.confirmar = None;
                iced::Task::none()
            }
            Msg::ExecutarConfirmar => {
                self.exec.confirmar = None;
                let args = comandos::executar().iter().map(|s| s.to_string()).collect();
                tarefa::terminal(self.dir.clone(), args, Origem::Executar)
            }
            Msg::Parar => {
                self.desligar_acompanhar();
                let args = comandos::parar().iter().map(|s| s.to_string()).collect();
                tarefa::cli(self.dir.clone(), args, Origem::Parar)
            }
            Msg::Acompanhar => self.alternar_acompanhar(),
            Msg::Tick(ciclo) => {
                if !self.exec.acompanhando || ciclo != self.exec.ciclo {
                    return iced::Task::none();
                }
                tarefa::carregar(self.dir.clone(), Escopo::TICK, self.exec.split_k, true)
            }
            Msg::AtualizarTokens => self.ler_tokens(),
            Msg::Tokens(linha) => {
                self.exec.tokens_linha = linha;
                iced::Task::none()
            }
            Msg::Rechecar => self.recarregar(Escopo::SISTEMA),
            Msg::SplitMudar(delta) => {
                let x = &mut self.exec;
                x.split_k = leituras::prender_k(x.split_k + delta, x.sis_teto);
                self.recarregar(Escopo::SISTEMA)
            }
            Msg::Dividir(dispatch) => self.dividir(dispatch),
            _ => iced::Task::none(),
        }
    }

    /// **O quê:** o interruptor «Reload / Acompanhar». **Onde:** [`App::atualizar_execucao`].
    ///
    /// Segundo clique desliga. Ligar ANEXA a um run que já roda por fora: segue de imediato, sem
    /// esperar `active`.
    fn alternar_acompanhar(&mut self) -> iced::Task<Msg> {
        if self.exec.acompanhando {
            self.desligar_acompanhar();
            self.exec.status = Aviso::ok("acompanhamento desligado.");
            return iced::Task::none();
        }
        self.exec.visto_ativo = true;
        self.exec.contagem = (u64::MAX, u64::MAX);
        self.exec.acompanhando = true;
        self.exec.ciclo += 1;
        self.exec.status = Aviso::ok("acompanhando o overdev deste projeto…");
        let tokens = self.ler_tokens();
        let tick = tarefa::carregar(self.dir.clone(), Escopo::TICK, self.exec.split_k, true);
        iced::Task::batch([tick, tokens])
    }

    /// **O quê:** «Dividir» / «Dividir + lançar»: confere o governador e abre o terminal.
    ///
    /// **Onde:** [`App::atualizar_execucao`]. Sem teto lido não dá para checar nada — e dividir às
    /// cegas é o que o governador evita. O bloqueio é uma função PURA (`bloqueio_do_split`).
    fn dividir(&mut self, dispatch: bool) -> iced::Task<Msg> {
        let x = &mut self.exec;
        let bloqueio = if x.sis_teto <= 0 {
            Some("não consegui ler o teto da máquina — clique em «Rechecar».".to_string())
        } else {
            leituras::bloqueio_do_split(x.split_k, dispatch, x.sis_teto, x.sis_livre)
        };
        if let Some(b) = bloqueio {
            x.split = Aviso::falha(b);
            return iced::Task::none();
        }
        let args = comandos::dividir(x.split_k, dispatch);
        tarefa::terminal(self.dir.clone(), args, Origem::Dividir)
    }

    /// **O quê:** o resultado de «Abrir no terminal». **Onde:** `Resp(Executar, …)`.
    ///
    /// Sucesso: passa a acompanhar o run que acabou de abrir, esperando ele ficar `active`.
    pub fn resposta_de_executar(&mut self, msg: String, erro: bool) -> iced::Task<Msg> {
        self.exec.status = Aviso::de(msg, erro);
        if erro {
            return iced::Task::none();
        }
        self.exec.visto_ativo = false;
        self.exec.acompanhando = true;
        self.exec.ciclo += 1;
        let ciclo = self.exec.ciclo;
        iced::Task::batch([self.ler_tokens(), tarefa::espera_tick(ciclo)])
    }

    /// **O quê:** o resultado de «Parar». **Onde:** `Resp(Parar, …)`.
    pub fn resposta_de_parar(&mut self, msg: String, erro: bool) -> iced::Task<Msg> {
        self.exec.status = Aviso::de(msg, erro);
        self.recarregar(Escopo::TUDO)
    }

    /// **O quê:** o resultado de «Dividir». **Onde:** `Resp(Dividir, …)`.
    pub fn resposta_de_dividir(&mut self, msg: String, erro: bool) {
        self.exec.split = Aviso::de(msg, erro);
    }

    /// **O quê:** aplica a leitura leve de um ciclo do «Acompanhar» e decide o próximo passo.
    ///
    /// **Onde:** `Msg::CargaTick`. **Para sozinho** quando o run termina — mas só DEPOIS de ter
    /// visto o run `active` uma vez: um `state.json` velho ("stopped") não pode encerrar o
    /// acompanhamento antes de o overdev sequer arrancar no terminal que acabou de abrir. Se algo
    /// andou (feitos/abertos mudaram), relê o resto; senão agenda o próximo ciclo.
    pub fn aplicar_tick(&mut self, c: &super::carga::Carga) -> iced::Task<Msg> {
        if c.dir != self.dir {
            return iced::Task::none();
        }
        self.aplicar_carga(c);
        if !self.exec.acompanhando {
            return iced::Task::none();
        }
        let ciclo = self.exec.ciclo;
        // Falha de leitura NÃO encerra: o ciclo seguinte tenta de novo (o erro já está no cabeçalho).
        let Some(Ok(p)) = &c.progresso else { return tarefa::espera_tick(ciclo) };
        let ativo = p.modo.as_deref() == Some("active");
        if ativo {
            self.exec.visto_ativo = true;
        }
        let agora = (p.feitos, p.abertos);
        let mudou = std::mem::replace(&mut self.exec.contagem, agora) != agora;
        if self.exec.visto_ativo && (!ativo || p.abertos == 0) {
            self.desligar_acompanhar();
            self.exec.status = Aviso::ok("o run terminou — acompanhamento encerrado.");
            return self.recarregar(Escopo::TUDO);
        }
        let proximo = tarefa::espera_tick(ciclo);
        if mudou {
            // Algo andou: o checklist, a fila e a caixa mudaram junto.
            return iced::Task::batch([self.recarregar(Escopo::TUDO), proximo]);
        }
        proximo
    }

    /// **O quê:** a aba Execução. **Onde:** [`super::view`].
    pub fn ver_execucao(&self) -> Element<'_, Msg> {
        let sistema_e_split = row![self.ver_sistema(), self.ver_split()].spacing(10);
        let col = column![self.ver_controles(), sistema_e_split, self.ver_monitor()]
            .spacing(10)
            .width(Fill);
        rolagem(col)
    }

    /// **O quê:** o bloco «Controles do run». **Onde:** [`App::ver_execucao`].
    fn ver_controles(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let x = &self.exec;
        let pode = self.tem_run;
        let acompanhar = if x.acompanhando { "Acompanhando…" } else { "Reload / Acompanhar" };
        let mut col = column![
            negrito("Controles do run", 13.5, c.texto),
            row![
                btn(
                    c,
                    "Executar",
                    (pode && x.confirmar.is_none()).then_some(Msg::ExecutarPedir),
                    true,
                    true
                ),
                btn(c, "Parar", pode.then_some(Msg::Parar), false, true),
                btn(c, acompanhar, pode.then_some(Msg::Acompanhar), x.acompanhando, true),
                btn(c, "Atualizar tokens", Some(Msg::AtualizarTokens), false, true),
            ]
            .spacing(8),
        ]
        .spacing(8);
        if !pode {
            col = col.push(
                txt(
                    "Sem run neste diretório. Inicie com `schematize-overdev start \"<objetivo>\"`.",
                    12.0,
                    c.dim,
                )
                .width(Fill),
            );
        }
        if let Some(cmd) = &x.confirmar {
            col = col.push(self.guardrail(cmd));
        }
        col = col.push(status(c, &x.status.texto, x.status.erro, 12.0));
        cartao(c, c.borda, 12.0, col)
    }

    /// **O quê:** o guardrail de Executar: o comando que o terminal vai rodar, e só roda se
    /// confirmar. **Onde:** [`App::ver_controles`].
    fn guardrail<'a>(&'a self, cmd: &'a str) -> Element<'a, Msg> {
        let c = self.cores;
        let corpo = column![
            negrito(
                "O agente `claude` vai mexer neste projeto. Abre num TERMINAL, com este comando:",
                12.0,
                c.aviso
            )
            .width(Fill),
            txt(cmd, 11.5, c.texto).font(Font::MONOSPACE).width(Fill),
            row![
                Space::new().width(Fill),
                btn(c, "Cancelar", Some(Msg::ExecutarCancelar), false, true),
                btn(c, "Abrir no terminal", Some(Msg::ExecutarConfirmar), true, true),
            ]
            .spacing(8),
        ]
        .spacing(6);
        container(corpo).padding(10).width(Fill).style(super::tema::cartao(c, c.aviso, 9.0)).into()
    }

    /// **O quê:** a coluna Sistema (teto/livre/rodando/load). **Onde:** [`App::ver_execucao`].
    fn ver_sistema(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let x = &self.exec;
        let metrica = |valor: String, rotulo: &'static str, cor: Color| -> Element<'_, Msg> {
            column![negrito(valor, 20.0, cor), txt(rotulo, 11.0, c.dim),]
                .align_x(Alignment::Center)
                .spacing(1)
                .into()
        };
        let livre = if x.sis_livre > 0 { c.bom } else { c.aviso };
        let mut col = column![
            row![
                negrito("Sistema", 13.5, c.texto),
                Space::new().width(Fill),
                btn(c, "Rechecar", Some(Msg::Rechecar), false, true),
            ]
            .align_y(Alignment::Center),
            row![
                metrica(x.sis_teto.to_string(), "teto", c.texto),
                metrica(x.sis_livre.to_string(), "livre", livre),
                metrica(x.sis_rodando.to_string(), "rodando", c.texto),
                metrica(x.sis_load.clone(), "load", c.texto),
            ]
            .spacing(14),
        ]
        .spacing(8);
        if !x.sis_erro.is_empty() {
            col = col.push(txt(&x.sis_erro, 11.5, c.aviso).width(Fill));
        }
        cartao(c, c.borda, 12.0, col)
    }

    /// **O quê:** o bloco «Dividir em N claudes». **Onde:** [`App::ver_execucao`].
    fn ver_split(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let x = &self.exec;
        let pode = self.tem_run;
        let mut col = column![
            negrito("Dividir em N claudes (multiagent)", 13.5, c.texto),
            row![
                btn(c, "−", Some(Msg::SplitMudar(-1)), false, true),
                negrito(x.split_k.to_string(), 16.0, c.texto).width(28).center(),
                btn(c, "+", Some(Msg::SplitMudar(1)), false, true),
                btn(c, "Dividir", pode.then_some(Msg::Dividir(false)), false, true),
                btn(c, "Dividir + lançar", pode.then_some(Msg::Dividir(true)), true, true),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(8);
        if !x.split_plano.is_empty() {
            col = col.push(txt(&x.split_plano, 11.5, c.dim).width(Fill));
        }
        col = col.push(status(c, &x.split.texto, x.split.erro, 11.5));
        cartao(c, c.borda, 12.0, col)
    }

    /// **O quê:** o Monitor: tokens, próximos itens abertos e conclusões. **Onde:** [`App::ver_execucao`].
    fn ver_monitor(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let x = &self.exec;
        let tokens = if x.tokens_linha.is_empty() {
            "Tokens: — (clique em «Atualizar tokens»)"
        } else {
            &x.tokens_linha
        };
        let mut col = column![
            negrito("Monitor", 13.5, c.texto),
            txt(tokens, 12.0, c.dim).width(Fill),
            negrito("Próximos itens abertos", 12.0, c.texto),
        ]
        .spacing(6);
        if x.mon_abertos.is_empty() {
            col = col.push(txt("nenhum item de máquina aberto", 11.5, c.dim));
        }
        for a in &x.mon_abertos {
            col = col.push(txt(format!("○ {a}"), 12.0, c.texto).width(Fill));
        }
        col = col.push(negrito("Concluídos (mais recentes)", 12.0, c.texto));
        if x.mon_conclusoes.is_empty() {
            col = col.push(txt("nenhuma conclusão registrada", 11.5, c.dim));
        }
        for k in &x.mon_conclusoes {
            col = col.push(txt(format!("✓ {k}"), 12.0, c.bom).width(Fill));
        }
        cartao(c, c.borda, 12.0, col)
    }
}
