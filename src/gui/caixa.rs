//! A CAIXA DE ENTRADA: capturar uma demanda, organizar com um agente e fundir no checklist.
//!
//! **O quê:** `add` (captura), `caixa agente` (abre um agente num terminal para organizar) e
//! `caixa merge` (funde as organizadas no checklist), mais a leitura por `caixa list --json`.
//!
//! **Onde:** a aba Caixa. Existe para jogar «isso também precisa» no projeto ENQUANTO um agente
//! trabalha, sem interromper ninguém e sem tocar no checklist.
//!
//! **Quem grava é o CLI** — a captura é atômica e a fusão roda sob trava; a janela só dispara.
//! `caixa agente` é o PRÓPRIO comando do app que abre o terminal (o agente mexe no projeto, e
//! mexer sem a pessoa ver acontecer é o que a casa não faz).

use super::estado::{App, Aviso};
use super::leituras::Demanda;
use super::msg::{Msg, Origem};
use super::tema;
use super::widgets::{btn, cartao, negrito, rolagem, status, txt};
use super::{carga::Escopo, comandos, tarefa};
use iced::widget::{column, row, text_editor};
use iced::{Element, Fill};

/// O estado da aba Caixa.
#[derive(Default)]
pub struct EstadoCaixa {
    /// O campo de captura (várias linhas).
    pub texto: text_editor::Content,
    /// Demandas esperando organização.
    pub pendentes: Vec<Demanda>,
    /// Demandas organizadas, prontas para fundir.
    pub prontas: Vec<Demanda>,
    pub status: Aviso,
}

impl EstadoCaixa {
    /// **O quê:** há demanda escrita (não só espaço)? **Onde:** o botão «Capturar» e o `update`.
    pub fn tem_texto(&self) -> bool {
        !self.texto.text().trim().is_empty()
    }
}

impl App {
    /// **O quê:** trata as mensagens da aba Caixa. **Onde:** [`App::update`].
    pub fn atualizar_caixa(&mut self, m: Msg) -> iced::Task<Msg> {
        match m {
            Msg::CaixaEditar(a) => {
                self.caixa.texto.perform(a);
                iced::Task::none()
            }
            Msg::CaixaAdicionar => {
                if !self.caixa.tem_texto() {
                    self.caixa.status = Aviso::falha("escreva a demanda antes de capturar.");
                    return iced::Task::none();
                }
                let args = comandos::capturar(&self.caixa.texto.text());
                tarefa::cli(self.dir.clone(), args, Origem::CaixaCapturar)
            }
            Msg::CaixaAgente => {
                tarefa::cli(self.dir.clone(), comandos::caixa_agente(), Origem::CaixaAgente)
            }
            Msg::CaixaMesclar => {
                tarefa::cli(self.dir.clone(), comandos::caixa_mesclar(), Origem::CaixaFundir)
            }
            _ => iced::Task::none(),
        }
    }

    /// **O quê:** o resultado de uma ação da Caixa. **Onde:** `Resp(Caixa*, …)`.
    ///
    /// - **Capturar:** só limpa o campo se capturou — se falhou, o texto digitado NÃO se perde.
    /// - **Agente:** sem terminal o CLI sai 0 e imprime o prompt: é uma falha para a pessoa, não
    ///   um sucesso.
    /// - **Fundir:** os itens novos têm de aparecer na hora (relê tudo), senão parece que a fusão
    ///   não fez nada.
    pub fn resposta_da_caixa(&mut self, o: Origem, msg: String, erro: bool) -> iced::Task<Msg> {
        match o {
            Origem::CaixaCapturar => {
                if !erro {
                    self.caixa.texto = text_editor::Content::new();
                }
                self.caixa.status = Aviso::de(msg, erro);
                self.recarregar(Escopo::CAIXA)
            }
            Origem::CaixaAgente => {
                let falhou = erro || msg.starts_with("não consegui");
                self.caixa.status = Aviso::de(msg, falhou);
                iced::Task::none()
            }
            _ => {
                self.caixa.status = Aviso::de(msg, erro);
                self.recarregar(Escopo::TUDO)
            }
        }
    }

    /// **O quê:** a aba Caixa. **Onde:** [`super::view`].
    pub fn ver_caixa(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let x = &self.caixa;
        let editor = text_editor(&x.texto)
            .placeholder("uma demanda: isso também precisa…")
            .on_action(Msg::CaixaEditar)
            .height(92)
            .padding(8)
            .size(13)
            .style(tema::editor(c));
        let (p, f) = (x.pendentes.len(), x.prontas.len());
        let mut col = column![
            txt(
                "Captura uma demanda sem interromper o agente: o checklist só muda quando você funde.",
                12.0,
                c.dim
            )
            .width(Fill),
            editor,
            row![
                btn(c, "Capturar", x.tem_texto().then_some(Msg::CaixaAdicionar), true, true),
                btn(c, format!("Organizar com agente ({p})"), (p > 0).then_some(Msg::CaixaAgente), false, true),
                btn(c, format!("Fundir no checklist ({f})"), (f > 0).then_some(Msg::CaixaMesclar), false, true),
            ]
            .spacing(8),
            status(c, &x.status.texto, x.status.erro, 12.0),
            negrito(format!("A organizar ({p})"), 13.0, c.texto),
        ]
        .spacing(10)
        .width(Fill);
        if p == 0 {
            col = col.push(txt("nenhuma demanda esperando.", 12.0, c.dim));
        }
        for d in &x.pendentes {
            col = col.push(demanda(self, &d.texto, &d.id, c.borda));
        }
        col = col.push(negrito(format!("A fundir ({f})"), 13.0, c.texto));
        if f == 0 {
            col = col.push(txt("nada organizado ainda.", 12.0, c.dim));
        }
        for d in &x.prontas {
            let itens = d.itens.iter().map(|i| format!("• {i}")).collect::<Vec<_>>().join("\n");
            col = col.push(demanda(self, &itens, &d.id, c.accent));
        }
        rolagem(col)
    }
}

/// **O quê:** um cartão de demanda: o texto e o id embaixo. **Onde:** [`App::ver_caixa`].
fn demanda<'a>(app: &'a App, texto: &str, id: &str, borda: iced::Color) -> Element<'a, Msg> {
    let c = app.cores;
    let corpo = column![
        txt(texto.to_string(), 12.5, c.texto).width(Fill),
        txt(id.to_string(), 10.5, c.dim)
    ]
    .spacing(2);
    cartao(c, borda, 8.0, corpo)
}
