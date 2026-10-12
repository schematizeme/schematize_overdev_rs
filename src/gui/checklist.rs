//! A aba CHECKLIST: os itens do run, com filtro, paginação e as ações do item humano.
//!
//! **O quê:** a lista paginada ([`App::ver_checklist`]), o filtro/paginação e o «Feito» do item
//! humano ([`App::atualizar_checklist`]).
//!
//! **Onde:** a segunda aba. As ações «Responder»/«Recusar» abrem o modal de [`super::resolver`].
//!
//! ## Por que paginar
//!
//! Só `POR_PAGINA` itens são DESENHADOS, qualquer que seja o tamanho do projeto: a árvore de
//! widgets do iced não é virtualizada, e 4000 linhas por quadro seriam software de massa
//! falhando num projeto grande. O checklist inteiro fica em memória; só a fatia vai à tela.

use super::estado::{App, Aviso};
use super::msg::{Msg, Origem};
use super::widgets::{btn, chip, negrito, rolagem, status, txt};
use super::{carga::Escopo, comandos, tarefa, telas};
use iced::font::Style;
use iced::widget::{column, row, Space};
use iced::{Element, Fill, Font};

/// O glifo e se é "positivo" (verde), "apagado" (cinza) ou "em espera" (âmbar) de um tipo de item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tom {
    Bom,
    Apagado,
    Espera,
    Aberto,
}

/// **O quê:** o glifo e o tom de um tipo de item. PURA. **Onde:** [`App::linha_do_item`].
///
/// `maquina` distingue o `○` de uma tarefa de máquina (cinza) da pendência humana (laranja).
fn glifo(tipo: &str, maquina: bool) -> (&'static str, Tom) {
    match tipo {
        "done" | "hdone" => ("✓", Tom::Bom),
        "hresp" => ("=", Tom::Bom),
        "hrec" | "cancel" => ("×", Tom::Apagado),
        "hold" => ("~", Tom::Espera),
        _ if maquina => ("○", Tom::Apagado),
        _ => ("○", Tom::Aberto),
    }
}

impl App {
    /// **O quê:** trata filtro, página e «Feito» do item humano. **Onde:** [`App::update`].
    pub fn atualizar_checklist(&mut self, m: Msg) -> iced::Task<Msg> {
        match m {
            Msg::Filtro(f) => {
                self.filtro = f;
                self.pagina = 0;
                iced::Task::none()
            }
            Msg::Pagina(p) => {
                self.pagina = p.min(telas::paginas(self.total_filtrado()) - 1);
                iced::Task::none()
            }
            Msg::ItemFeito(n) => {
                if n < 1 {
                    self.item_status = Aviso::falha(
                        "este item não tem número de humano aberto — clique em «Reler».",
                    );
                    return iced::Task::none();
                }
                tarefa::cli(self.dir.clone(), comandos::feito(n as i64), Origem::Item)
            }
            _ => iced::Task::none(),
        }
    }

    /// **O quê:** o resultado do «Feito»: a faixa da aba e a releitura. **Onde:** `Resp(Item, …)`.
    ///
    /// O CLI imprime o que fez; vazio vira o fato mínimo, não silêncio.
    pub fn resposta_do_item(&mut self, msg: String, erro: bool) -> iced::Task<Msg> {
        let msg = if msg.is_empty() && !erro { "item humano fechado.".to_string() } else { msg };
        self.item_status = Aviso::de(msg, erro);
        self.recarregar(Escopo::TUDO)
    }

    /// **O quê:** a aba Checklist. **Onde:** [`super::view`].
    pub fn ver_checklist(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let (fatia, total) = telas::fatia(&self.itens, self.filtro, self.pagina);
        let filtros =
            [("Todos", 0), ("Abertos", 1), ("Feitos", 2), ("Em espera", 3), ("Humanos", 4)];
        let mut chips = row![].spacing(6);
        for (rotulo, f) in filtros {
            chips = chips.push(chip(c, rotulo, self.filtro == f, Msg::Filtro(f)));
        }
        let mut lista = column![].spacing(5).width(Fill);
        if total == 0 {
            lista = lista.push(txt("Nenhum item neste filtro.", 12.5, c.dim));
        }
        for i in &fatia {
            lista = lista.push(self.linha_do_item(i));
        }
        let mut col = column![
            status(c, &self.item_status.texto, self.item_status.erro, 12.0),
            chips,
            rolagem(lista),
        ]
        .spacing(8);
        let paginas = telas::paginas(total);
        if paginas > 1 {
            let (ant, prox) = (
                self.pagina.checked_sub(1),
                (self.pagina + 1 < paginas).then_some(self.pagina + 1),
            );
            col = col.push(
                row![
                    btn(c, "‹", ant.map(Msg::Pagina), false, true),
                    txt(format!("{} / {paginas} · {total} itens", self.pagina + 1), 12.5, c.dim),
                    btn(c, "›", prox.map(Msg::Pagina), false, true),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
            );
        }
        col.into()
    }

    /// **O quê:** uma linha do checklist. **Onde:** [`App::ver_checklist`].
    ///
    /// Item HUMANO ABERTO ganha as ações: fechar, responder (libera a máquina) ou recusar
    /// (cancela). O número é a posição entre os humanos abertos.
    fn linha_do_item<'a>(&'a self, i: &'a telas::Item) -> Element<'a, Msg> {
        let c = self.cores;
        let recuo: Element<Msg> = Space::new().width(if i.sub { 24 } else { 0 }).into();
        if i.tipo == "nota" {
            let it = Font { style: Style::Italic, ..Font::DEFAULT };
            return row![recuo, txt(&i.texto, 12.0, c.dim).font(it).width(Fill)].spacing(8).into();
        }
        let (g, tom) = glifo(&i.tipo, i.maquina);
        let cor_g = match tom {
            Tom::Bom => c.bom,
            Tom::Apagado => c.dim,
            Tom::Espera => c.aviso,
            Tom::Aberto => c.accent,
        };
        let riscado = i.tipo == "hrec" || i.tipo == "cancel";
        // O texto do que JÁ acabou (feito, respondido, recusado) fica apagado: só o glifo é verde.
        // Uma lista de 200 itens feitos em verde cheio cansa e esconde o que ainda está aberto.
        let cor_t = if matches!(tom, Tom::Bom) || riscado { c.dim } else { c.texto };
        let mut texto = txt(&i.texto, 13.0, cor_t).width(Fill);
        if riscado {
            texto = texto.font(Font { style: Style::Italic, ..Font::DEFAULT });
        }
        let tag = negrito(
            if i.maquina { "máq." } else { "humano" },
            10.0,
            if i.maquina { c.dim } else { c.accent },
        );
        // Centro vertical: o texto de 13 px, a etiqueta de 10 px e os botões (mais altos) ficam na
        // mesma linha de olhar — sem isto os botões desciam alguns pixels sobre o texto.
        let mut linha = row![recuo, negrito(g, 13.0, cor_g), tag, texto]
            .spacing(8)
            .align_y(iced::Alignment::Center);
        if i.tipo == "hopen" {
            let n = i.humano_n;
            linha = linha.push(
                row![
                    btn(c, "Feito", Some(Msg::ItemFeito(n)), false, true),
                    btn(
                        c,
                        "Responder",
                        Some(Msg::ResolverPedir(n, i.texto.clone(), true)),
                        false,
                        true
                    ),
                    btn(
                        c,
                        "Recusar",
                        Some(Msg::ResolverPedir(n, i.texto.clone(), false)),
                        false,
                        true
                    ),
                ]
                .spacing(4),
            );
        }
        linha.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cada tipo de item tem o glifo que o Slint desenhava, e a máquina é cinza, o humano laranja.
    #[test]
    fn o_glifo_e_o_tom_de_cada_tipo() {
        assert_eq!(glifo("done", true), ("✓", Tom::Bom));
        assert_eq!(glifo("hdone", false), ("✓", Tom::Bom));
        assert_eq!(glifo("hresp", false), ("=", Tom::Bom));
        assert_eq!(glifo("hrec", false), ("×", Tom::Apagado));
        assert_eq!(glifo("cancel", true), ("×", Tom::Apagado));
        assert_eq!(glifo("hold", true), ("~", Tom::Espera));
        assert_eq!(glifo("open", true), ("○", Tom::Apagado), "tarefa de máquina aberta é cinza");
        assert_eq!(glifo("hopen", false), ("○", Tom::Aberto), "pendência humana é laranja");
    }
}
