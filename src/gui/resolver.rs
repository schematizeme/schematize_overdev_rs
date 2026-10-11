//! Ações sobre ITEM HUMANO do checklist: fechar, responder e recusar.
//!
//! **O quê:** «Feito» na linha → `human --done N`; «Responder»/«Recusar» abrem o modal e
//! confirmam com `answer N -- texto` / `refuse N -- texto`.
//!
//! **Onde:** a aba Checklist (botões na linha do item `hopen`) e o `modal_resolver.slint`.
//!
//! ## Por que o modal existe
//!
//! Responder LIBERA o item de máquina que o humano travava; recusar o CANCELA. Resposta vazia
//! liberaria a máquina sem critério registrado — por isso o botão do modal só habilita com texto,
//! e esta camada confere de novo (o `enabled` do Slint não é a regra, é a conveniência).
//!
//! ## Quem grava é o CLI
//!
//! O CLI faz a máquina de estados do checklist sob trava e registra a decisão no `DECISOES.md`.
//! A janela só manda o comando e mostra a mensagem dele.

use super::{comandos, ponte, Estado, MainWindow, Ov};
use slint::{ComponentHandle, SharedString};
use std::rc::Rc;

/// **O quê:** escreve a mensagem do último ato sobre um item na faixa da aba Checklist.
///
/// **Onde:** «Feito» e a confirmação do modal.
fn faixa(w: &MainWindow, msg: String, erro: bool) {
    w.global::<Ov>().set_item_status(msg.into());
    w.global::<Ov>().set_item_erro(erro);
}

/// **O quê:** registra os callbacks de item humano. **Onde:** [`super::main`], uma vez.
pub fn ligar(w: &MainWindow, e: &Rc<Estado>) {
    com_ov!(w, e, on_item_feito, |w, e, n| {
        if n < 1 {
            faixa(
                &w,
                "este item não tem número de humano aberto — clique em «Reler».".into(),
                true,
            );
        } else {
            let (msg, erro) = ponte::rodar(e, &comandos::feito(n as i64));
            // O CLI imprime o que fez; vazio vira o fato mínimo, não silêncio.
            let msg = if msg.is_empty() { format!("item humano {n} fechado.") } else { msg };
            faixa(&w, msg, erro);
            crate::recarregar(&w, e);
        }
    });
    com_ov!(w, e, on_resolver_pedir, |w, e, n, texto, responder| {
        let _ = e;
        let o = w.global::<Ov>();
        o.set_resolve_n(n);
        o.set_resolve_item(texto);
        o.set_resolve_responder(responder);
        o.set_resolve_texto(SharedString::new());
        o.set_resolve_status(SharedString::new());
        o.set_resolve_erro(false);
        o.set_resolve_open(true);
    });
    com_ov!(w, e, on_resolver_cancelar, |w, e| {
        let _ = e;
        w.global::<Ov>().set_resolve_open(false);
    });
    com_ov!(w, e, on_resolver_confirmar, |w, e| {
        let o = w.global::<Ov>();
        let texto = o.get_resolve_texto().to_string();
        let n = o.get_resolve_n();
        let responder = o.get_resolve_responder();
        if texto.trim().is_empty() || n < 1 {
            o.set_resolve_erro(true);
            o.set_resolve_status(
                if n < 1 {
                    "item sem número — feche o modal e clique em «Reler»."
                } else {
                    "escreva antes de confirmar."
                }
                .into(),
            );
        } else {
            let (msg, erro) = ponte::rodar(e, &comandos::resolver(n as i64, responder, &texto));
            if erro {
                // O motivo da recusa do CLI fica NO modal, onde a pessoa está olhando.
                o.set_resolve_erro(true);
                o.set_resolve_status(msg.into());
            } else {
                o.set_resolve_open(false);
                faixa(&w, msg, false);
                crate::recarregar(&w, e);
            }
        }
    });
}
