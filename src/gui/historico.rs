//! A aba HISTÓRICO: os snapshots do run, e «Restaurar».
//!
//! **O quê:** lista as versões do CHECKLIST, do PLAN e do resto guardadas no banco local, e
//! manda o `restore N` ao TERMINAL.
//!
//! **Onde:** a terceira aba.
//!
//! **Restaurar REGRAVA arquivos do projeto** (D6): por isso vai ao terminal, onde o comando
//! aparece ANTES de rodar e o `Ctrl-C` funciona. A janela nunca escreve no disco.

use super::estado::App;
use super::msg::{Msg, Origem};
use super::tarefa;
use super::telas;
use super::widgets::{btn, cartao, negrito, rolagem, txt};
use iced::widget::{column, row};
use iced::{Alignment, Element, Fill};

impl App {
    /// **O quê:** trata «Restaurar» de um snapshot. **Onde:** [`App::update`].
    pub fn atualizar_historico(&mut self, m: Msg) -> iced::Task<Msg> {
        match m {
            Msg::Restaurar(id) => tarefa::terminal(
                self.dir.clone(),
                vec!["restore".to_string(), id.to_string()],
                Origem::Restaurar,
            ),
            _ => iced::Task::none(),
        }
    }

    /// **O quê:** o resultado de «Restaurar». **Onde:** `Resp(Restaurar, …)`.
    ///
    /// **Sem terminal, a mensagem traz o COMANDO** (§37.48): quem não tem emulador ainda consegue
    /// fazer a coisa colando. Ela aparece na própria aba, em âmbar; o sucesso vai ao topo.
    pub fn resposta_de_restaurar(&mut self, msg: String, erro: bool) {
        if erro {
            self.snaps_erro = msg;
        } else {
            self.msg = "terminal aberto — clique em «Reler» quando ele terminar.".to_string();
        }
    }

    /// **O quê:** a aba Histórico. **Onde:** [`super::view`].
    pub fn ver_historico(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let mut col = column![txt(
            "Versões do CHECKLIST, do PLAN e do resto guardadas no banco local. Restaurar abre o terminal.",
            12.0,
            c.dim
        )
        .width(Fill)]
        .spacing(6)
        .width(Fill);
        if !self.snaps_erro.is_empty() {
            col = col.push(txt(&self.snaps_erro, 12.0, c.aviso).width(Fill));
        }
        if self.snaps.is_empty() && self.snaps_erro.is_empty() {
            col = col.push(txt(
                "Nenhum snapshot ainda — rode `schematize-overdev snapshot`.",
                12.0,
                c.dim,
            ));
        }
        for s in &self.snaps {
            let linha = row![
                negrito(&s.arquivo, 12.5, c.texto).width(Fill),
                txt(telas::quando(s.em), 11.5, c.dim),
                txt(telas::tamanho(s.bytes), 11.5, c.dim),
                btn(c, "Restaurar", Some(Msg::Restaurar(s.id)), false, true),
            ]
            .spacing(12)
            .align_y(Alignment::Center);
            col = col.push(cartao(c, c.borda, 8.0, linha));
        }
        rolagem(col)
    }
}
