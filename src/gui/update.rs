//! O `update` da janela: recebe uma [`Msg`], muda o [`App`] e devolve a [`Task`] do efeito.
//!
//! **O quê:** o despacho de TODAS as mensagens para o tratador da aba certa.
//!
//! **Onde:** o `iced::application` ([`super::main`]); os testes chamam `App::update` direto.
//!
//! ## Elm: o `update` não faz efeito
//!
//! Ele só altera o estado e DEVOLVE a tarefa (rodar o CLI, abrir o terminal, esperar 3 s). O
//! teste afirma o estado e `Task::units()` — 0 = nenhum efeito pedido. É por isso que "resposta
//! vazia não vai ao CLI" é afirmável sem binário instalado.

use super::carga::Escopo;
use super::estado::App;
use super::msg::{Msg, Origem};
use super::tarefa;
use super::tema::Cores;
use iced::Task;

impl App {
    /// **O quê:** relê o `escopo` do projeto atual. **Onde:** a abertura, o «Reler», a troca de
    /// projeto e depois de cada ação.
    pub fn recarregar(&self, escopo: Escopo) -> Task<Msg> {
        tarefa::carregar(self.dir.clone(), escopo, self.exec.split_k, false)
    }

    /// **O quê:** trata uma mensagem. **Onde:** o runtime do iced, a cada evento.
    pub fn update(&mut self, m: Msg) -> Task<Msg> {
        let efeito = self.despachar(m);
        // A aba Agentes lê o disco ao abrir (também com `--aba agentes` e depois de trocar de projeto).
        let agentes = self.garantir_agentes();
        Task::batch([efeito, agentes])
    }

    /// **O quê:** encaminha a mensagem ao tratador da aba. **Onde:** [`App::update`].
    fn despachar(&mut self, m: Msg) -> Task<Msg> {
        match m {
            Msg::Aba(a) => {
                self.aba = a;
                Task::none()
            }
            Msg::Reler => {
                self.msg.clear();
                self.quiz = Default::default();
                self.recarregar(Escopo::TUDO)
            }
            Msg::AlternarTema => {
                self.cores = Cores::de(!self.cores.escuro);
                self.tema = self.cores.tema();
                Task::none()
            }
            Msg::Carga(c) => {
                self.aplicar_carga(&c);
                Task::none()
            }
            Msg::CargaTick(c) => self.aplicar_tick(&c),
            Msg::Resp(o, msg, erro) => self.resposta(o, msg, erro),
            Msg::Responder(..)
            | Msg::Outro(..)
            | Msg::ResponderTexto(_)
            | Msg::Marcar(..)
            | Msg::ConfirmarMultipla(_)
            | Msg::MarcarFeito(_)
            | Msg::Recusar(_) => self.atualizar_perguntas(m),
            Msg::Filtro(_) | Msg::Pagina(_) | Msg::ItemFeito(_) => self.atualizar_checklist(m),
            Msg::ResolverPedir(..)
            | Msg::ResolverEditar(_)
            | Msg::ResolverCancelar
            | Msg::ResolverConfirmar => self.atualizar_resolver(m),
            Msg::Restaurar(_) => self.atualizar_historico(m),
            Msg::ExecutarPedir
            | Msg::ExecutarCancelar
            | Msg::ExecutarConfirmar
            | Msg::Parar
            | Msg::Acompanhar
            | Msg::Tick(_)
            | Msg::AtualizarTokens
            | Msg::Tokens(_)
            | Msg::Rechecar
            | Msg::SplitMudar(_)
            | Msg::Dividir(_) => self.atualizar_execucao(m),
            Msg::CaixaEditar(_) | Msg::CaixaAdicionar | Msg::CaixaAgente | Msg::CaixaMesclar => {
                self.atualizar_caixa(m)
            }
            Msg::AbrirArquivo(_)
            | Msg::NotaTask(_)
            | Msg::NotaCorrecao(_)
            | Msg::NotaAdicionar(_) => self.atualizar_notas(m),
            Msg::Agentes(m) => self.atualizar_agentes(m),
            Msg::Topo(m) => self.atualizar_topo(m),
            Msg::Home(m) => self.atualizar_home(m),
            Msg::Sino(m) => self.atualizar_sino(m),
            Msg::AbrirSeletor
            | Msg::FecharSeletor
            | Msg::Projetos(_)
            | Msg::SeletorCaminho(_)
            | Msg::SeletorAbrirCaminho
            | Msg::SeletorEscolher(_) => self.atualizar_seletor(m),
        }
    }

    /// **O quê:** encaminha a resposta de um comando do CLI à aba que o pediu. **Onde:**
    /// [`App::update`], para `Msg::Resp`.
    fn resposta(&mut self, o: Origem, msg: String, erro: bool) -> Task<Msg> {
        match o {
            Origem::Quiz => self.resposta_do_quiz(msg, erro),
            Origem::Item => self.resposta_do_item(msg, erro),
            Origem::Resolver => self.resposta_do_resolver(msg, erro),
            Origem::Executar => self.resposta_de_executar(msg, erro),
            Origem::Parar => self.resposta_de_parar(msg, erro),
            Origem::Dividir => {
                self.resposta_de_dividir(msg, erro);
                Task::none()
            }
            Origem::CaixaCapturar | Origem::CaixaAgente | Origem::CaixaFundir => {
                self.resposta_da_caixa(o, msg, erro)
            }
            Origem::Nota(_) | Origem::Arquivo => self.resposta_das_notas(o, msg, erro),
            Origem::Restaurar => {
                self.resposta_de_restaurar(msg, erro);
                Task::none()
            }
            Origem::Home => {
                self.resposta_da_home(msg, erro);
                Task::none()
            }
            Origem::Iniciar => self.resposta_de_iniciar(msg, erro),
            Origem::Topo => {
                self.resposta_do_topo(msg, erro);
                Task::none()
            }
            Origem::Fixar => self.resposta_de_fixar(msg, erro),
            Origem::Sino => self.resposta_do_sino(msg, erro),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::carga::Carga;
    use super::super::msg::Aba;
    use super::super::resolver::Resolver;
    use super::super::telas::{Card, Checklist, Item, Opc, Progresso};
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    fn app() -> App {
        App::novo(
            PathBuf::from("/p/a"),
            PathBuf::from("/nao/usado"),
            "v".into(),
            Aba::Perguntas,
            true,
        )
    }

    fn item(tipo: &str, n: usize) -> Item {
        Item { tipo: tipo.into(), texto: "t".into(), maquina: false, sub: false, humano_n: n }
    }

    fn progresso(modo: Option<&str>, feitos: u64, abertos: u64) -> Progresso {
        Progresso {
            modo: modo.map(String::from),
            feitos,
            abertos,
            teto: Some(10),
            ..Progresso::default()
        }
    }

    /// Uma carga de `dir` só com o progresso. **Onde:** os testes do tick.
    fn carga_de(dir: &Path, escopo: Escopo, p: Result<Progresso, String>) -> Carga {
        let mut c = Carga::vazia(dir.to_path_buf(), escopo);
        c.progresso = Some(p);
        c
    }

    // ---------- janela ----------

    #[test]
    fn trocar_de_aba_e_o_tema_so_mudam_o_estado() {
        let mut a = app();
        assert_eq!(a.update(Msg::Aba(Aba::Notas)).units(), 0);
        assert_eq!(a.aba, Aba::Notas);
        let escuro = a.cores;
        let _ = a.update(Msg::AlternarTema);
        assert!(!a.cores.escuro && a.cores != escuro, "alternou para o claro");
        assert_eq!(a.tema.palette().background, a.cores.bg);
        let _ = a.update(Msg::AlternarTema);
        assert_eq!(a.cores, escuro, "duas vezes volta ao escuro");
    }

    #[test]
    fn reler_pede_a_leitura_completa_e_limpa_a_mensagem_do_topo() {
        let mut a = app();
        a.msg = "terminal aberto".into();
        a.quiz = super::super::estado::Aviso::falha("velho");
        assert_eq!(a.update(Msg::Reler).units(), 1);
        assert!(a.msg.is_empty() && a.quiz.texto.is_empty());
    }

    // ---------- carga → estado ----------

    #[test]
    fn cada_erro_de_leitura_vai_para_o_lugar_dele_e_nao_apaga_os_outros() {
        let mut a = app();
        a.itens.push(item("open", 0));
        let mut c = Carga::vazia(a.dir.clone(), Escopo::TUDO);
        c.progresso = Some(Err("progresso quebrou".into()));
        c.checklist = Some(Err("checklist quebrou".into()));
        c.fila = Some(Err("fila quebrou\nUsage: …".into()));
        c.historico = Some(Err("historico quebrou".into()));
        c.caixa = Some(Err("caixa quebrou\nmais".into()));
        let _ = a.update(Msg::Carga(Arc::new(c)));
        assert_eq!(a.erro, "progresso quebrou\nchecklist quebrou");
        assert!(!a.tem_run && a.itens.is_empty());
        assert!(a.quiz.erro && a.quiz.texto.starts_with("fila quebrou"));
        assert_eq!(a.snaps_erro, "historico quebrou");
        assert_eq!(a.caixa.status.texto, "caixa quebrou", "só a primeira linha vai ao banner");
    }

    #[test]
    fn carga_boa_preenche_o_cabecalho_o_checklist_e_a_fila() {
        let mut a = app();
        let mut c = Carga::vazia(a.dir.clone(), Escopo::TUDO);
        c.progresso = Some(Ok(progresso(Some("active"), 3, 1)));
        c.checklist = Some(Ok(Checklist {
            modo: None,
            objetivo: Some("o objetivo".into()),
            itens: vec![item("done", 0), item("hopen", 1)],
        }));
        c.fila = Some(Ok(vec![Card {
            id: "q1".into(),
            titulo: "t".into(),
            contexto: String::new(),
            kind: "escolha".into(),
            opcoes: vec![Opc {
                id: "a".into(),
                rotulo: "A".into(),
                detalhe: String::new(),
                sugerida: false,
            }],
            destrava: String::new(),
            aberta: true,
            aguarda_revisao: false,
            resposta_texto: String::new(),
            respondida_em: String::new(),
            revisao: String::new(),
        }]));
        let _ = a.update(Msg::Carga(Arc::new(c)));
        assert!(a.tem_run && a.resumo.starts_with("ativo · 3 feitos"));
        assert_eq!((a.feitos, a.abertos, a.teto), (3, 1, 10));
        assert_eq!(a.objetivo, "o objetivo");
        assert_eq!(a.itens.len(), 2);
        assert_eq!((a.quiz_abertas(), a.quiz_revisar()), (1, 0));
        assert!(a.erro.is_empty());
    }

    /// A carga de OUTRO projeto (a troca aconteceu no meio da leitura) é descartada.
    #[test]
    fn carga_de_outro_projeto_e_descartada() {
        let mut a = app();
        let c = carga_de(Path::new("/p/outro"), Escopo::TUDO, Ok(progresso(Some("active"), 9, 9)));
        let _ = a.update(Msg::Carga(Arc::new(c)));
        assert_eq!(a.feitos, 0);
        assert!(!a.tem_run);
    }

    #[test]
    fn a_pagina_fica_presa_quando_o_filtro_encolhe_a_lista() {
        let mut a = app();
        a.itens = (0..350).map(|_| item("open", 0)).collect();
        let _ = a.update(Msg::Pagina(3));
        assert_eq!(a.pagina, 3);
        let _ = a.update(Msg::Pagina(99));
        assert_eq!(a.pagina, 3, "além do fim: presa na última (350 itens = 4 páginas)");
        let _ = a.update(Msg::Filtro(4));
        assert_eq!(a.pagina, 0, "trocar o filtro volta à primeira página");
        assert_eq!(a.total_filtrado(), 0);
    }

    // ---------- perguntas ----------

    #[test]
    fn resposta_vazia_nao_vai_ao_cli_e_a_com_texto_vai() {
        let mut a = app();
        assert_eq!(a.update(Msg::ResponderTexto("q1".into())).units(), 0);
        assert_eq!(a.quiz.texto, "escreva a resposta antes de enviar");
        assert!(a.quiz.erro);
        let _ = a.update(Msg::Outro("q1".into(), "   ".into()));
        assert_eq!(
            a.update(Msg::ResponderTexto("q1".into())).units(),
            0,
            "só espaço também é vazio"
        );
        let _ = a.update(Msg::Outro("q1".into(), "  feito assim ".into()));
        assert_eq!(a.update(Msg::ResponderTexto("q1".into())).units(), 1);
    }

    #[test]
    fn o_campo_outro_e_de_cada_pergunta() {
        let mut a = app();
        let _ = a.update(Msg::Outro("q1".into(), "um".into()));
        let _ = a.update(Msg::Outro("q2".into(), "dois".into()));
        assert_eq!(a.outros["q1"], "um");
        assert_eq!(a.outros["q2"], "dois");
    }

    #[test]
    fn multipla_marca_desmarca_e_so_confirma_com_alguma_marca() {
        let mut a = app();
        assert_eq!(a.update(Msg::ConfirmarMultipla("q".into())).units(), 0);
        assert_eq!(a.quiz.texto, "marque ao menos uma opção");
        let _ = a.update(Msg::Marcar("q".into(), "a".into()));
        let _ = a.update(Msg::Marcar("q".into(), "b".into()));
        let _ = a.update(Msg::Marcar("q".into(), "a".into()));
        assert_eq!(a.marcadas["q"], ["b"], "a segunda vez desmarca");
        assert_eq!(a.update(Msg::ConfirmarMultipla("q".into())).units(), 1);
        assert!(!a.marcadas.contains_key("q"), "as marcas saem ao confirmar");
    }

    #[test]
    fn a_resposta_do_cli_vira_o_banner_limpa_os_campos_e_rele() {
        let mut a = app();
        let _ = a.update(Msg::Outro("q".into(), "x".into()));
        let t = a.update(Msg::Resp(Origem::Quiz, "recusada: motivo\nUsage: …".into(), true));
        assert_eq!(a.quiz.texto, "recusada: motivo");
        assert!(a.quiz.erro && a.outros.is_empty());
        assert_eq!(t.units(), 1);
    }

    // ---------- checklist e resolver ----------

    #[test]
    fn item_sem_numero_nao_vai_ao_cli() {
        let mut a = app();
        assert_eq!(a.update(Msg::ItemFeito(0)).units(), 0);
        assert!(a.item_status.erro && a.item_status.texto.contains("Reler"));
        assert_eq!(a.update(Msg::ItemFeito(2)).units(), 1);
    }

    #[test]
    fn o_resultado_do_feito_vira_a_faixa_e_rele() {
        let mut a = app();
        let t = a.update(Msg::Resp(Origem::Item, String::new(), false));
        assert_eq!(a.item_status.texto, "item humano fechado.", "vazio vira o fato mínimo");
        assert_eq!(t.units(), 1);
    }

    #[test]
    fn o_modal_de_resolver_exige_texto_e_numero() {
        let mut a = app();
        let _ = a.update(Msg::ResolverPedir(2, "item".into(), true));
        assert!(a.resolver.as_ref().is_some_and(|r| r.n == 2 && r.responder));
        assert_eq!(a.update(Msg::ResolverConfirmar).units(), 0);
        assert_eq!(a.resolver.as_ref().unwrap().status.texto, "escreva antes de confirmar.");
        let r: &mut Resolver = a.resolver.as_mut().unwrap();
        r.texto = iced::widget::text_editor::Content::with_text("  vale a opção A ");
        assert!(r.tem_texto());
        assert_eq!(a.update(Msg::ResolverConfirmar).units(), 1, "com texto vai ao CLI");
        // Sem número, nem com texto.
        let _ = a.update(Msg::ResolverPedir(0, "item".into(), false));
        a.resolver.as_mut().unwrap().texto = iced::widget::text_editor::Content::with_text("x");
        assert_eq!(a.update(Msg::ResolverConfirmar).units(), 0);
        assert!(a.resolver.as_ref().unwrap().status.texto.contains("sem número"));
    }

    #[test]
    fn recusa_do_cli_fica_no_modal_e_sucesso_fecha_e_rele() {
        let mut a = app();
        let _ = a.update(Msg::ResolverPedir(1, "item".into(), true));
        let t = a.update(Msg::Resp(Origem::Resolver, "o CLI recusou".into(), true));
        assert_eq!(t.units(), 0);
        assert!(a.resolver.as_ref().is_some_and(|r| r.status.texto == "o CLI recusou"));
        let t = a.update(Msg::Resp(Origem::Resolver, "liberado".into(), false));
        assert!(a.resolver.is_none());
        assert_eq!(a.item_status.texto, "liberado");
        assert_eq!(t.units(), 1);
        let _ = a.update(Msg::ResolverPedir(1, "item".into(), true));
        let _ = a.update(Msg::ResolverCancelar);
        assert!(a.resolver.is_none());
    }

    // ---------- execução ----------

    #[test]
    fn executar_mostra_o_comando_exato_e_so_abre_o_terminal_ao_confirmar() {
        let mut a = app();
        assert_eq!(a.update(Msg::ExecutarPedir).units(), 0);
        let cmd = a.exec.confirmar.clone().expect("o guardrail abriu");
        assert!(
            cmd.contains(" run") && cmd.contains("(em /p/a)") && !cmd.contains("--yes"),
            "{cmd}"
        );
        let _ = a.update(Msg::ExecutarCancelar);
        assert!(a.exec.confirmar.is_none());
        let _ = a.update(Msg::ExecutarPedir);
        assert_eq!(a.update(Msg::ExecutarConfirmar).units(), 1);
        assert!(a.exec.confirmar.is_none());
    }

    #[test]
    fn terminal_aberto_passa_a_acompanhar_e_falha_nao() {
        let mut a = app();
        let t = a.update(Msg::Resp(Origem::Executar, "terminal aberto".into(), false));
        assert!(a.exec.acompanhando && !a.exec.visto_ativo);
        assert_eq!(t.units(), 2, "tokens + o primeiro relógio");
        let mut b = app();
        let t =
            b.update(Msg::Resp(Origem::Executar, "não achei um terminal. Rode: x".into(), true));
        assert!(!b.exec.acompanhando && b.exec.status.erro);
        assert_eq!(t.units(), 0);
    }

    #[test]
    fn o_interruptor_de_acompanhar_liga_e_desliga() {
        let mut a = app();
        let t = a.update(Msg::Acompanhar);
        assert!(a.exec.acompanhando && a.exec.visto_ativo);
        assert_eq!(t.units(), 2, "uma leitura leve + os tokens");
        let ciclo = a.exec.ciclo;
        let t = a.update(Msg::Acompanhar);
        assert!(!a.exec.acompanhando && a.exec.ciclo > ciclo);
        assert_eq!(t.units(), 0);
        assert_eq!(a.exec.status.texto, "acompanhamento desligado.");
    }

    #[test]
    fn tick_de_relogio_velho_ou_desligado_e_ignorado() {
        let mut a = app();
        assert_eq!(a.update(Msg::Tick(0)).units(), 0, "desligado");
        let _ = a.update(Msg::Acompanhar);
        let vivo = a.exec.ciclo;
        assert_eq!(a.update(Msg::Tick(vivo - 1)).units(), 0, "relógio de um ciclo anterior");
        assert_eq!(a.update(Msg::Tick(vivo)).units(), 1);
    }

    #[test]
    fn o_tick_para_sozinho_quando_o_run_termina_mas_so_depois_de_ver_o_ativo() {
        let mut a = app();
        let _ = a.update(Msg::Resp(Origem::Executar, "ok".into(), false));
        // Run velho "stopped" ANTES de ter visto `active`: segue acompanhando (só reagenda).
        let c = carga_de(&a.dir, Escopo::TICK, Ok(progresso(Some("stopped"), 5, 3)));
        let t = a.update(Msg::CargaTick(Arc::new(c)));
        assert!(a.exec.acompanhando && !a.exec.visto_ativo);
        assert_eq!(t.units(), 2, "a leitura completa (algo mudou) + o próximo relógio");
        // Vê o ativo: continua; depois de ativo, abertos == 0 encerra.
        let c = carga_de(&a.dir, Escopo::TICK, Ok(progresso(Some("active"), 5, 3)));
        let _ = a.update(Msg::CargaTick(Arc::new(c)));
        assert!(a.exec.visto_ativo && a.exec.acompanhando);
        let c = carga_de(&a.dir, Escopo::TICK, Ok(progresso(Some("done"), 8, 0)));
        let t = a.update(Msg::CargaTick(Arc::new(c)));
        assert!(!a.exec.acompanhando);
        assert_eq!(a.exec.status.texto, "o run terminou — acompanhamento encerrado.");
        assert_eq!(t.units(), 1, "só a releitura final");
    }

    #[test]
    fn falha_de_leitura_no_tick_nao_encerra_o_acompanhamento() {
        let mut a = app();
        let _ = a.update(Msg::Acompanhar);
        let c = carga_de(&a.dir, Escopo::TICK, Err("binário sumiu".into()));
        let t = a.update(Msg::CargaTick(Arc::new(c)));
        assert!(a.exec.acompanhando, "o ciclo seguinte tenta de novo");
        assert_eq!(t.units(), 1);
        assert!(!a.tem_run, "sem leitura não se oferece Executar/Parar");
    }

    #[test]
    fn dividir_confere_o_governador_antes_de_abrir_o_terminal() {
        let mut a = app();
        assert_eq!(a.update(Msg::Dividir(false)).units(), 0);
        assert!(a.exec.split.texto.contains("Rechecar"), "sem teto lido não divide às cegas");
        a.exec.sis_teto = 8;
        a.exec.sis_livre = 3;
        a.exec.split_k = 4;
        assert_eq!(
            a.update(Msg::Dividir(true)).units(),
            0,
            "lançar 4 com 3 livres trava a máquina"
        );
        assert!(a.exec.split.erro && a.exec.split.texto.contains("3 slot"));
        assert_eq!(a.update(Msg::Dividir(false)).units(), 1, "dividir sem lançar não ocupa slot");
    }

    #[test]
    fn o_k_do_split_fica_preso_ao_teto() {
        let mut a = app();
        a.exec.sis_teto = 4;
        for _ in 0..10 {
            let _ = a.update(Msg::SplitMudar(1));
        }
        assert_eq!(a.exec.split_k, 4);
        for _ in 0..10 {
            let _ = a.update(Msg::SplitMudar(-1));
        }
        assert_eq!(a.exec.split_k, 2);
    }

    #[test]
    fn parar_desliga_o_acompanhamento_na_hora() {
        let mut a = app();
        let _ = a.update(Msg::Acompanhar);
        assert_eq!(a.update(Msg::Parar).units(), 1);
        assert!(!a.exec.acompanhando);
    }

    // ---------- caixa e notas ----------

    #[test]
    fn captura_vazia_nao_vai_ao_cli() {
        let mut a = app();
        assert_eq!(a.update(Msg::CaixaAdicionar).units(), 0);
        assert!(a.caixa.status.erro && a.caixa.status.texto.contains("escreva a demanda"));
        a.caixa.texto = iced::widget::text_editor::Content::with_text("  isso também precisa ");
        assert_eq!(a.update(Msg::CaixaAdicionar).units(), 1);
    }

    #[test]
    fn captura_so_limpa_o_campo_se_entrou() {
        let mut a = app();
        a.caixa.texto = iced::widget::text_editor::Content::with_text("demanda");
        let _ = a.update(Msg::Resp(Origem::CaixaCapturar, "recusada".into(), true));
        assert!(a.caixa.tem_texto(), "falhou: o texto digitado não se perde");
        let t = a.update(Msg::Resp(Origem::CaixaCapturar, "capturada".into(), false));
        assert!(!a.caixa.tem_texto());
        assert_eq!(t.units(), 1);
    }

    #[test]
    fn organizar_sem_terminal_e_falha_mesmo_com_exit_zero() {
        let mut a = app();
        let _ =
            a.update(Msg::Resp(Origem::CaixaAgente, "não consegui abrir terminal".into(), false));
        assert!(a.caixa.status.erro);
        let _ = a.update(Msg::Resp(Origem::CaixaAgente, "agente aberto".into(), false));
        assert!(!a.caixa.status.erro);
    }

    #[test]
    fn nota_vazia_nao_vai_ao_cli_e_so_limpa_o_campo_certo_se_entrou() {
        let mut a = app();
        assert_eq!(a.update(Msg::NotaAdicionar(false)).units(), 0);
        assert!(a.notas.status.erro);
        let _ = a.update(Msg::NotaTask("um ponto".into()));
        let _ = a.update(Msg::NotaCorrecao("uma correção".into()));
        assert_eq!(a.update(Msg::NotaAdicionar(true)).units(), 1);
        let _ = a.update(Msg::Resp(Origem::Nota(true), "anexada".into(), false));
        assert!(a.notas.correcao.is_empty() && a.notas.task == "um ponto");
        let _ = a.update(Msg::Resp(Origem::Nota(false), "recusada".into(), true));
        assert_eq!(a.notas.task, "um ponto", "falhou: o texto fica");
    }

    // ---------- histórico ----------

    #[test]
    fn restaurar_vai_ao_terminal_e_a_falha_traz_o_comando() {
        let mut a = app();
        assert_eq!(a.update(Msg::Restaurar(7)).units(), 1);
        let _ = a.update(Msg::Resp(Origem::Restaurar, "ok".into(), false));
        assert!(a.msg.contains("Reler"));
        let _ = a.update(Msg::Resp(
            Origem::Restaurar,
            "não achei um terminal. Rode: x restore 7".into(),
            true,
        ));
        assert!(a.snaps_erro.contains("Rode:"));
    }

    // ---------- self-check ----------

    /// O varredor de `units()` enxerga uma tarefa de verdade: se `Task::none()` e uma `perform`
    /// dessem o mesmo número, todos os testes acima estariam cegos.
    #[test]
    fn o_contador_de_tarefas_distingue_nenhuma_de_uma() {
        assert_eq!(Task::<Msg>::none().units(), 0);
        assert_eq!(app().recarregar(Escopo::CAIXA).units(), 1);
    }
}
