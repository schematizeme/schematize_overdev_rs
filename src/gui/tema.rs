//! O TEMA da janela (escuro/claro) e os estilos dos controles — PLATAFORMA, nada de domínio.
//!
//! **O quê:** a paleta ([`Cores`]), o [`iced::Theme`] derivado dela e as funções de estilo
//! (botão, chip, cartão, campo de texto) que as telas usam.
//!
//! **Onde:** [`super::widgets`] e todas as telas. Substitui o `ui/tema.slint`: os MESMOS valores
//! de cor, para a janela nova ficar igual à que ela substitui.
//!
//! **Sem crate de tema:** `Theme::custom` + closures de estilo do próprio iced. A paleta é
//! `Copy` e entra por valor nas closures, então nenhum estilo segura referência ao `App`.

use iced::widget::{button, container, text_editor, text_input};
use iced::{border, Background, Border, Color, Theme};

/// Constrói uma cor a partir de `0xRRGGBB`. **Onde:** a tabela das duas paletas.
const fn rgb(hex: u32) -> Color {
    Color {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

/// A paleta da janela. Os campos têm o nome que tinham no `Th` do Slint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cores {
    /// Verdadeiro no tema escuro.
    pub escuro: bool,
    pub bg: Color,
    pub surface: Color,
    pub surface2: Color,
    pub borda: Color,
    pub texto: Color,
    pub dim: Color,
    pub accent: Color,
    pub accent_soft: Color,
    pub bom: Color,
    pub aviso: Color,
    pub titulo: Color,
}

impl Cores {
    /// **O quê:** a paleta do tema escuro. **Onde:** [`Cores::de`].
    pub const ESCURO: Cores = Cores {
        escuro: true,
        bg: rgb(0x14161c),
        surface: rgb(0x1b1e27),
        surface2: rgb(0x222634),
        borda: rgb(0x2b3040),
        texto: rgb(0xe6e9f2),
        dim: rgb(0x8a90a2),
        accent: rgb(0xf08a3c),
        accent_soft: rgb(0x4a3320),
        bom: rgb(0x3ecf8e),
        aviso: rgb(0xe6b23a),
        titulo: rgb(0xffb677),
    };

    /// **O quê:** a paleta do tema claro. **Onde:** [`Cores::de`]. O verde e o âmbar são mais
    /// escuros que no escuro: o `#3ecf8e` sobre branco não chega a contraste legível como TEXTO.
    pub const CLARO: Cores = Cores {
        escuro: false,
        bg: rgb(0xe8ecf4),
        surface: rgb(0xffffff),
        surface2: rgb(0xdce3f0),
        borda: rgb(0xbfc9dc),
        texto: rgb(0x12151d),
        dim: rgb(0x475062),
        accent: rgb(0xf08a3c),
        accent_soft: rgb(0xfde3cc),
        bom: rgb(0x1a8f5c),
        aviso: rgb(0x9a6a00),
        titulo: rgb(0xb4531a),
    };

    /// **O quê:** a paleta do tema pedido. **Onde:** [`super::app::App`], ao abrir e no alternar.
    pub fn de(escuro: bool) -> Cores {
        if escuro {
            Self::ESCURO
        } else {
            Self::CLARO
        }
    }

    /// **O quê:** o [`Theme`] do iced para esta paleta — o que dá a cara certa aos widgets que
    /// NÃO têm estilo próprio aqui (a barra de rolagem, por exemplo).
    pub fn tema(&self) -> Theme {
        let nome = if self.escuro { "Overdev escuro" } else { "Overdev claro" };
        Theme::custom(
            nome,
            iced::theme::Palette {
                background: self.bg,
                text: self.texto,
                primary: self.accent,
                success: self.bom,
                warning: self.aviso,
                danger: self.aviso,
            },
        )
    }
}

/// Cor `c` com a opacidade `a`. **Onde:** o fundo do modal e o botão desabilitado.
pub fn com_alfa(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

/// **O quê:** o estilo de um botão. `primario` = laranja cheio (aba ativa, ação principal).
///
/// **Onde:** [`super::widgets::btn`]. Desabilitado fica no `surface2` e meio transparente, como
/// no Slint; o hover do secundário usa o `accent_soft`.
pub fn botao(c: Cores, primario: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let (fundo, texto) = match (status, primario) {
            (button::Status::Disabled, _) => (c.surface2, com_alfa(c.texto, 0.5)),
            (button::Status::Hovered | button::Status::Pressed, true) => {
                (rgb(0xf59d5a), Color::WHITE)
            }
            (_, true) => (c.accent, Color::WHITE),
            (button::Status::Hovered | button::Status::Pressed, false) => (c.accent_soft, c.texto),
            (_, false) => (c.surface, c.texto),
        };
        button::Style {
            background: Some(Background::Color(fundo)),
            text_color: texto,
            border: Border {
                color: if primario { Color::TRANSPARENT } else { c.borda },
                width: if primario { 0.0 } else { 1.0 },
                radius: border::radius(9),
            },
            ..button::Style::default()
        }
    }
}

/// **O quê:** o estilo de um chip de filtro (pílula pequena). **Onde:** o filtro do checklist.
pub fn chip(c: Cores, selecionado: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let fundo = if selecionado || matches!(status, button::Status::Hovered) {
            c.accent_soft
        } else {
            c.surface
        };
        button::Style {
            background: Some(Background::Color(fundo)),
            text_color: c.texto,
            border: Border {
                color: if selecionado { c.accent } else { c.borda },
                width: 1.0,
                radius: border::radius(13),
            },
            ..button::Style::default()
        }
    }
}

/// **O quê:** o estilo de um botão de OPÇÃO de pergunta (linha larga com rótulo e detalhe).
///
/// **Onde:** [`super::perguntas`]. A sugestão do agente é DESTAQUE (borda grossa laranja), nunca
/// pré-seleção: resposta que a pessoa não deu não é resposta dela.
pub fn opcao(c: Cores, sugerida: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: Some(Background::Color(if matches!(status, button::Status::Hovered) {
            c.accent_soft
        } else {
            c.surface
        })),
        text_color: c.texto,
        border: Border {
            color: if sugerida { c.accent } else { c.borda },
            width: if sugerida { 2.0 } else { 1.0 },
            radius: border::radius(8),
        },
        ..button::Style::default()
    }
}

/// **O quê:** o estilo da caixinha de marcar do `multipla` (20x20). **Onde:** [`super::perguntas`].
pub fn marca(c: Cores, marcada: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, _| button::Style {
        background: Some(Background::Color(if marcada { c.accent } else { c.surface })),
        text_color: Color::WHITE,
        border: Border {
            color: if marcada { c.accent } else { c.borda },
            width: 1.0,
            radius: border::radius(5),
        },
        ..button::Style::default()
    }
}

/// **O quê:** o estilo de um cartão (fundo `surface`, borda `borda`, cantos redondos).
///
/// **Onde:** os blocos das abas e os cards de pergunta. `borda` é a cor do contorno: âmbar para
/// "precisa de você", laranja para "a máquina revisa", neutra para o resto.
pub fn cartao(c: Cores, borda: Color, raio: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(c.surface)),
        border: Border { color: borda, width: 1.0, radius: border::radius(raio) },
        ..container::Style::default()
    }
}

/// **O quê:** um container liso com a cor `fundo` e cantos `raio`. **Onde:** a barra de progresso,
/// a linha separadora, o fundo escurecido do modal.
pub fn liso(fundo: Color, raio: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(fundo)),
        border: border::rounded(raio),
        ..container::Style::default()
    }
}

/// **O quê:** o estilo do campo de texto de uma linha. **Onde:** o "outro" da pergunta, as notas e
/// o caminho do seletor de projeto.
pub fn campo(c: Cores) -> impl Fn(&Theme, text_input::Status) -> text_input::Style {
    move |_, status| text_input::Style {
        background: Background::Color(c.bg),
        border: Border {
            color: if matches!(status, text_input::Status::Focused { .. }) {
                c.accent
            } else {
                c.borda
            },
            width: 1.0,
            radius: border::radius(8),
        },
        icon: c.dim,
        placeholder: c.dim,
        value: c.texto,
        selection: com_alfa(c.accent, 0.35),
    }
}

/// **O quê:** o estilo do editor de várias linhas. **Onde:** a caixa de captura e o modal.
pub fn editor(c: Cores) -> impl Fn(&Theme, text_editor::Status) -> text_editor::Style {
    move |_, status| text_editor::Style {
        background: Background::Color(c.bg),
        border: Border {
            color: if matches!(status, text_editor::Status::Focused { .. }) {
                c.accent
            } else {
                c.borda
            },
            width: 1.0,
            radius: border::radius(8),
        },
        placeholder: c.dim,
        value: c.texto,
        selection: com_alfa(c.accent, 0.35),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// As duas paletas existem e são DIFERENTES — o alternar de tema muda de fato as cores.
    #[test]
    fn escuro_e_claro_sao_paletas_distintas() {
        assert_ne!(Cores::de(true), Cores::de(false));
        assert!(Cores::de(true).escuro && !Cores::de(false).escuro);
        assert_ne!(Cores::de(true).bg, Cores::de(false).bg);
    }

    /// O fundo escuro é o do Slint (`#14161c`) e o acento é o laranja `#f08a3c`: a janela nova
    /// tem de parecer a antiga.
    #[test]
    fn as_cores_batem_com_o_tema_antigo() {
        assert_eq!(Cores::ESCURO.bg.into_rgba8(), [0x14, 0x16, 0x1c, 0xff]);
        assert_eq!(Cores::ESCURO.accent.into_rgba8(), [0xf0, 0x8a, 0x3c, 0xff]);
        assert_eq!(Cores::ESCURO.bom.into_rgba8(), [0x3e, 0xcf, 0x8e, 0xff]);
    }

    /// O `Theme` derivado carrega o fundo da paleta (widgets sem estilo próprio herdam).
    #[test]
    fn o_theme_usa_o_fundo_da_paleta() {
        assert_eq!(Cores::CLARO.tema().palette().background, Cores::CLARO.bg);
    }
}
