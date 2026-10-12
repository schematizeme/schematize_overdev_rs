//! O `view` da janela: o cabeçalho, as abas, o progresso e o conteúdo da aba aberta.
//!
//! **O quê:** monta a tela inteira a partir do [`App`]; cada aba desenha o próprio corpo.
//!
//! **Onde:** o `iced::application` ([`super::main`]). Função PURA do estado: nada aqui muda nada.
//!
//! **Layout (igual ao do Slint):** título + versão à esquerda e os botões à direita; o caminho do
//! projeto; as abas em pílulas (a ativa em laranja); uma linha; o erro/mensagem; o objetivo e o
//! resumo do run com a barra de progresso verde; e então o corpo da aba.

use super::estado::App;
use super::msg::{Aba, Msg};
use super::tema;
use super::widgets::{btn, negrito, separador, txt};
use iced::widget::{column, container, progress_bar, row, stack, Space};
use iced::{Alignment, Element, Fill, Length};

impl App {
    /// **O quê:** o título da janela. **Onde:** o `iced::application`.
    pub fn titulo(&self) -> String {
        "schematize · overdev".to_string()
    }

    /// **O quê:** a tela inteira, com o modal por cima quando há um aberto.
    ///
    /// **Onde:** o `iced::application`. Dois modais possíveis (resolver item, trocar projeto); o
    /// do seletor tem a vez se os dois estiverem abertos (não acontece pela UI, mas a ordem é
    /// definida em vez de ficar ao acaso).
    pub fn view(&self) -> Element<'_, Msg> {
        let base = self.tela();
        match self.ver_seletor().or_else(|| self.ver_resolver()) {
            Some(modal) => stack![base, modal].into(),
            None => base,
        }
    }

    /// **O quê:** o conteúdo da janela, sem modal. **Onde:** [`App::view`].
    fn tela(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let mut col = column![self.cabecalho(), self.abas(), separador(c)].spacing(10);
        if !self.erro.is_empty() {
            col = col.push(
                container(txt(&self.erro, 12.5, c.aviso).width(Fill))
                    .padding(8)
                    .width(Fill)
                    .style(tema::cartao(c, c.aviso, 9.0)),
            );
        }
        if !self.msg.is_empty() {
            col = col.push(txt(&self.msg, 12.0, c.bom).width(Fill));
        }
        col = col.push(self.progresso());
        let corpo = match self.aba {
            Aba::Perguntas => self.ver_perguntas(),
            Aba::Checklist => self.ver_checklist(),
            Aba::Historico => self.ver_historico(),
            Aba::Execucao => self.ver_execucao(),
            Aba::Caixa => self.ver_caixa(),
            Aba::Notas => self.ver_notas(),
        };
        container(col.push(corpo)).padding(18).width(Fill).height(Fill).into()
    }

    /// **O quê:** título, versão, caminho do projeto e os botões do topo. **Onde:** [`App::tela`].
    fn cabecalho(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let glifo = if c.escuro { "☾" } else { "☀" };
        let topo = row![
            column![negrito("Overdev", 20.0, c.titulo), txt(&self.versao, 11.5, c.dim),],
            Space::new().width(Fill),
            btn(c, "Trocar projeto", Some(Msg::AbrirSeletor), false, false),
            btn(c, "Reler", Some(Msg::Reler), false, false),
            btn(c, glifo, Some(Msg::AlternarTema), false, false),
        ]
        .spacing(10)
        .align_y(Alignment::Start);
        column![topo, txt(self.dir.display().to_string(), 11.5, c.dim)].spacing(8).into()
    }

    /// **O quê:** as seis abas em pílulas; a ativa em laranja. **Onde:** [`App::tela`].
    fn abas(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let contagem = |base: &str, n: usize| {
            if n > 0 {
                format!("{base} ({n})")
            } else {
                base.to_string()
            }
        };
        let itens = [
            (Aba::Perguntas, contagem("Perguntas", self.quiz_abertas())),
            (Aba::Checklist, "Checklist".to_string()),
            (Aba::Historico, "Histórico".to_string()),
            (Aba::Execucao, "Execução".to_string()),
            (Aba::Caixa, contagem("Caixa", self.caixa.pendentes.len())),
            (Aba::Notas, "Notas".to_string()),
        ];
        let mut linha = row![].spacing(6);
        for (aba, rotulo) in itens {
            linha = linha.push(btn(c, rotulo, Some(Msg::Aba(aba)), self.aba == aba, true));
        }
        linha.into()
    }

    /// **O quê:** o objetivo, o resumo do run e a barra de progresso — sempre à vista.
    /// **Onde:** [`App::tela`]. Sem run (`modo` vazio) não há barra nem ciclos.
    fn progresso(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let mut col = column![].spacing(10);
        if !self.objetivo.is_empty() {
            col = col.push(negrito(&self.objetivo, 16.0, c.texto).width(Fill));
        }
        let mut linha = row![negrito(&self.resumo, 12.5, c.texto), Space::new().width(Fill)];
        if !self.modo.is_empty() {
            let ciclos = if self.teto > 0 {
                format!("ciclos {}/{}", self.iteracoes, self.teto)
            } else {
                format!("ciclos {}", self.iteracoes)
            };
            linha = linha.push(txt(ciclos, 12.0, c.dim));
        }
        col = col.push(linha);
        if !self.modo.is_empty() {
            let total = (self.feitos + self.abertos).max(1) as f32;
            col = col.push(
                progress_bar(0.0..=1.0, self.feitos as f32 / total)
                    .girth(Length::Fixed(8.0))
                    .style(move |_| progress_bar::Style {
                        background: iced::Background::Color(c.surface2),
                        bar: iced::Background::Color(c.bom),
                        border: iced::border::rounded(4),
                    }),
            );
        }
        col.into()
    }
}
