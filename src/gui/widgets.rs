//! Os blocos visuais reutilizados pelas telas — PLATAFORMA, nada de domínio.
//!
//! **O quê:** botão, chip, texto (normal/negrito/dim), faixa de status, cartão e linha separadora,
//! já no tema da janela.
//!
//! **Onde:** todas as telas ([`super::perguntas`], [`super::checklist`], …). Substitui os
//! componentes `Btn`/`Chip`/`Marca` do `tema.slint`.
//!
//! **Genéricos na mensagem:** nenhum helper conhece o `Msg` da janela — recebem o `Option<M>` que
//! o clique dispara (`None` = desabilitado). Assim servem a qualquer tela e testam sem a janela.

use super::tema::{self, Cores};
use iced::font::Weight;
use iced::widget::{
    button, center, container, mouse_area, opaque, scrollable, stack, text, Column, Space,
};
use iced::{Color, Element, Fill, Font, Length};

/// A fonte em negrito (títulos, valores). **Onde:** [`negrito`] e os títulos das telas.
pub const NEGRITO: Font = Font { weight: Weight::Bold, ..Font::DEFAULT };

/// A fonte do meio-termo (rótulos de botão). **Onde:** [`btn`].
pub const SEMI: Font = Font { weight: Weight::Semibold, ..Font::DEFAULT };

/// **O quê:** um texto na cor `cor` e no tamanho `tam`. **Onde:** quase toda linha da janela.
pub fn txt<'a>(s: impl text::IntoFragment<'a>, tam: f32, cor: Color) -> text::Text<'a> {
    text(s).size(tam).color(cor)
}

/// **O quê:** um texto em negrito. **Onde:** títulos de bloco e valores destacados.
pub fn negrito<'a>(s: impl text::IntoFragment<'a>, tam: f32, cor: Color) -> text::Text<'a> {
    txt(s, tam, cor).font(NEGRITO)
}

/// **O quê:** um botão arredondado. `msg = None` o desabilita; `primario` o pinta de laranja;
/// `pequeno` usa o tamanho de aba/ação em linha.
///
/// **Onde:** toda ação da janela. O desabilitado não responde ao clique — a regra de negócio
/// continua sendo conferida de novo no `update` (o botão é conveniência, não guarda).
pub fn btn<'a, M: Clone + 'a>(
    c: Cores,
    rotulo: impl Into<String>,
    msg: Option<M>,
    primario: bool,
    pequeno: bool,
) -> Element<'a, M> {
    let tam = if pequeno { 12.0 } else { 13.5 };
    let pad = if pequeno { [5, 11] } else { [9, 16] };
    button(text(rotulo.into()).size(tam).font(SEMI))
        .padding(pad)
        .on_press_maybe(msg)
        .style(tema::botao(c, primario))
        .into()
}

/// **O quê:** o chip (pílula de filtro) selecionável. **Onde:** o filtro do checklist.
pub fn chip<'a, M: Clone + 'a>(
    c: Cores,
    rotulo: &'a str,
    selecionado: bool,
    msg: M,
) -> Element<'a, M> {
    button(text(rotulo).size(12).font(SEMI))
        .padding([4, 11])
        .on_press(msg)
        .style(tema::chip(c, selecionado))
        .into()
}

/// **O quê:** a faixa de status de uma ação: verde (ok) ou âmbar (erro), vazia se `msg` é vazio.
///
/// **Onde:** o banner de cada aba. Mensagem vazia não ocupa espaço — por isso devolve um
/// `Space` de altura 0 em vez de um texto vazio com margem.
pub fn status<'a, M: 'a>(c: Cores, msg: &'a str, erro: bool, tam: f32) -> Element<'a, M> {
    if msg.is_empty() {
        return Space::new().height(0).into();
    }
    negrito(msg, tam, if erro { c.aviso } else { c.bom }).width(Fill).into()
}

/// **O quê:** envolve `conteudo` num cartão com `padding` e a borda `borda`.
///
/// **Onde:** os blocos das abas (controles, sistema, monitor), os cards de pergunta e as linhas
/// de snapshot/arquivo/demanda.
pub fn cartao<'a, M: 'a>(
    c: Cores,
    borda: Color,
    padding: f32,
    conteudo: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    container(conteudo).padding(padding).width(Fill).style(tema::cartao(c, borda, 12.0)).into()
}

/// **O quê:** a linha fina separadora. **Onde:** entre as abas e o conteúdo.
pub fn separador<'a, M: 'a>(c: Cores) -> Element<'a, M> {
    container(Space::new())
        .width(Fill)
        .height(Length::Fixed(1.0))
        .style(tema::liso(c.borda, 0.0))
        .into()
}

/// **O quê:** um MODAL: `corpo` num cartão centrado, sobre um fundo escurecido que dispara
/// `fechar` ao ser clicado.
///
/// **Onde:** [`super::resolver`] e [`super::projeto`], num `stack` acima do conteúdo da janela.
/// `aviso` pinta o contorno de âmbar (ação destrutiva: recusar). O cartão é `opaque`: clicar nele
/// não vaza para o fundo e não fecha o modal.
pub fn modal<'a, M: Clone + 'a>(
    c: Cores,
    corpo: Element<'a, M>,
    fechar: M,
    aviso: bool,
) -> Element<'a, M> {
    let borda = if aviso { tema::com_alfa(c.aviso, 0.6) } else { c.borda };
    let fundo = mouse_area(
        container(Space::new())
            .width(Fill)
            .height(Fill)
            .style(tema::liso(tema::com_alfa(Color::BLACK, 0.45), 0.0)),
    )
    .on_press(fechar);
    let cartao = container(corpo).padding(22).max_width(560).style(tema::cartao(c, borda, 14.0));
    stack![fundo, center(opaque(cartao)).padding(32)].into()
}

/// **O quê:** uma área ROLÁVEL que ocupa o espaço que sobra, com folga à direita para a barra de
/// rolagem não ficar por cima do conteúdo. **Onde:** o corpo de cada aba.
///
/// Recebe a [`Column`] e não um `Element`: a folga é o `padding` da própria coluna. Embrulhar o
/// conteúdo num `container` fazia a barra aparecer (um traço) mesmo com o conteúdo cabendo.
pub fn rolagem<'a, M: 'a>(conteudo: Column<'a, M>) -> Element<'a, M> {
    scrollable(conteudo.width(Fill).padding(iced::Padding::ZERO.right(12)))
        .height(Fill)
        .width(Fill)
        .into()
}
