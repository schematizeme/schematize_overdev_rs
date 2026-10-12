//! RESOLVER um item humano: responder (libera a máquina) ou recusar (cancela) — o modal.
//!
//! **O quê:** o estado do modal ([`Resolver`]), os comandos que ele manda ao CLI
//! (`answer N -- texto` / `refuse N -- texto`) e o desenho dele ([`App::ver_resolver`]).
//!
//! **Onde:** a aba Checklist, nos botões «Responder»/«Recusar» da linha de um item humano aberto.
//!
//! ## Por que o modal existe
//!
//! Responder LIBERA o item de máquina que o humano travava; recusar o CANCELA. Resposta vazia
//! liberaria a máquina sem critério registrado — por isso o botão só habilita com texto, e o
//! `update` confere de novo (o botão desabilitado não é a regra, é a conveniência).
//!
//! ## Quem grava é o CLI
//!
//! O CLI faz a máquina de estados do checklist sob trava e registra a decisão no `DECISOES.md`.
//! A janela só manda o comando e mostra a mensagem dele — o motivo de uma recusa do CLI fica NO
//! modal, onde a pessoa está olhando.

use super::estado::{App, Aviso};
use super::msg::{Msg, Origem};
use super::tema;
use super::widgets::{btn, negrito, status, txt};
use super::{carga::Escopo, comandos, tarefa};
use iced::widget::{column, row, text_editor, Space};
use iced::{Element, Fill};

/// O modal aberto.
pub struct Resolver {
    /// A posição entre os humanos abertos (1-based); 0 = sem número (não confirma).
    pub n: usize,
    /// O texto do item sendo resolvido.
    pub item: String,
    /// `true` = responder (libera); `false` = recusar (cancela).
    pub responder: bool,
    pub texto: text_editor::Content,
    pub status: Aviso,
}

impl Resolver {
    /// **O quê:** um modal novo, de texto vazio. **Onde:** `Msg::ResolverPedir`.
    pub fn novo(n: usize, item: String, responder: bool) -> Resolver {
        Resolver {
            n,
            item,
            responder,
            texto: text_editor::Content::new(),
            status: Aviso::default(),
        }
    }

    /// **O quê:** há texto de verdade (não só espaço)? **Onde:** o botão e a confirmação.
    pub fn tem_texto(&self) -> bool {
        !self.texto.text().trim().is_empty()
    }
}

impl App {
    /// **O quê:** trata abrir, editar, cancelar e confirmar o modal. **Onde:** [`App::update`].
    pub fn atualizar_resolver(&mut self, m: Msg) -> iced::Task<Msg> {
        match m {
            Msg::ResolverPedir(n, item, responder) => {
                self.resolver = Some(Resolver::novo(n, item, responder));
            }
            Msg::ResolverEditar(a) => {
                if let Some(r) = self.resolver.as_mut() {
                    r.texto.perform(a);
                }
            }
            Msg::ResolverCancelar => self.resolver = None,
            Msg::ResolverConfirmar => return self.confirmar_resolver(),
            _ => {}
        }
        iced::Task::none()
    }

    /// **O quê:** confirma o modal: confere o texto e o número, e manda `answer`/`refuse`.
    ///
    /// **Onde:** [`App::atualizar_resolver`]. Sem texto ou sem número NÃO vai ao CLI: o erro fica
    /// no modal.
    fn confirmar_resolver(&mut self) -> iced::Task<Msg> {
        let dir = self.dir.clone();
        let Some(r) = self.resolver.as_mut() else { return iced::Task::none() };
        if !r.tem_texto() || r.n < 1 {
            r.status = Aviso::falha(if r.n < 1 {
                "item sem número — feche o modal e clique em «Reler»."
            } else {
                "escreva antes de confirmar."
            });
            return iced::Task::none();
        }
        let args = comandos::resolver(r.n as i64, r.responder, &r.texto.text());
        tarefa::cli(dir, args, Origem::Resolver)
    }

    /// **O quê:** o resultado do `answer`/`refuse`. **Onde:** `Resp(Resolver, …)`.
    ///
    /// Erro: o motivo do CLI fica no modal. Sucesso: fecha, vira faixa da aba e relê.
    pub fn resposta_do_resolver(&mut self, msg: String, erro: bool) -> iced::Task<Msg> {
        if erro {
            if let Some(r) = self.resolver.as_mut() {
                r.status = Aviso::falha(msg);
            }
            return iced::Task::none();
        }
        self.resolver = None;
        self.item_status = Aviso::ok(msg);
        self.recarregar(Escopo::TUDO)
    }

    /// **O quê:** o modal, por cima da janela (fundo escurecido que cancela ao clicar fora).
    ///
    /// **Onde:** [`super::view`], num `stack` acima do conteúdo. `None` = fechado.
    pub fn ver_resolver(&self) -> Option<Element<'_, Msg>> {
        let r = self.resolver.as_ref()?;
        let c = self.cores;
        let (titulo, dica) = if r.responder {
            (
                "Responder item humano",
                "A resposta vira o critério do item e LIBERA o item de máquina que ele travava.",
            )
        } else {
            (
                "Recusar item humano",
                "Recusar CANCELA o item de máquina vinculado — não é o mesmo que fazer. Diga por quê.",
            )
        };
        let editor = text_editor(&r.texto)
            .on_action(Msg::ResolverEditar)
            .height(120)
            .padding(8)
            .size(13)
            .style(tema::editor(c));
        let confirmar = btn(
            c,
            if r.responder { "Responder" } else { "Recusar" },
            r.tem_texto().then_some(Msg::ResolverConfirmar),
            true,
            false,
        );
        let corpo = column![
            negrito(titulo, 16.0, if r.responder { c.texto } else { c.aviso }),
            negrito(&r.item, 13.0, c.texto).width(Fill),
            txt(dica, 12.0, c.dim).width(Fill),
            editor,
            status(c, &r.status.texto, r.status.erro, 12.0),
            row![
                Space::new().width(Fill),
                btn(c, "Cancelar", Some(Msg::ResolverCancelar), false, false),
                confirmar
            ]
            .spacing(10),
        ]
        .spacing(14);
        Some(super::widgets::modal(c, corpo.into(), Msg::ResolverCancelar, r.responder))
    }
}
