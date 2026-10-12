//! O CABEÇALHO: o seletor flutuante de projeto, os cinco ícones, a versão do hub, o sino e o tema.
//!
//! **O quê:** o mesmo conjunto da barra do hub — «Projeto:» com o seletor (fixados primeiro e
//! «Outro caminho…»), pasta, editor, recarregar, gerenciar projetos fixados, nova janela; a versão
//! do hub (`schematize --version`), o sino e o tema. Mais o modal de projetos fixados.
//!
//! **Onde:** [`super::view`] (a tela) e [`super::update`] (as mensagens).
//!
//! ## Omitidos de propósito: «← Voltar» e «Abrir a janela do Overdev»
//!
//! Os dois são navegação do HUB (voltar à Home; abrir esta mesma janela). Dentro da janela do
//! overdev não há para onde voltar nem o que abrir.
//!
//! ## Gerenciar: fixar e desafixar passam pelo CLI
//!
//! `schematize projects add|remove <pasta>` é quem grava a lista de fixados (D6). As pastas de
//! DESENVOLVIMENTO (varredura) o hub edita pela lib e não há subcomando para isso: lacuna do
//! `schematize_cli_rs`, registrada no relatório da paridade.

use super::estado::{App, Aviso};
use super::msg::{Msg, Opcao, Origem, SinoMsg, TopoMsg};
use super::widgets::{btn, btn_icone, modal, negrito, status, txt};
use super::{projeto, tarefa, tema};
use iced::widget::{column, combo_box, container, row, text_input, tooltip, Space};
use iced::{Alignment, Element, Fill, Length};

/// Os glifos dos botões do topo (texto Unicode simples: sem SVG e sem crate de ícones).
pub const G_PASTA: &str = "🗀";
pub const G_CODIGO: &str = "</>";
pub const G_RELER: &str = "↻";
pub const G_FIXAR: &str = "⊞";
pub const G_JANELA: &str = "↗";
pub const G_SINO: &str = "🔔";

/// O estado do topo.
#[derive(Debug, Default)]
pub struct Topo {
    /// «schematize v0.66.1» (do `schematize --version`); vazio até chegar.
    pub versao_hub: String,
    /// O resultado do último ícone (pasta, editor, nova janela).
    pub aviso: Aviso,
    pub gerenciar: bool,
    pub caminho: String,
    pub status: Aviso,
}

impl App {
    /// **O quê:** trata as mensagens do topo. **Onde:** [`App::update`].
    pub fn atualizar_topo(&mut self, m: TopoMsg) -> iced::Task<Msg> {
        match m {
            TopoMsg::ComboAbrir => {
                self.seletor.mostrar_todos();
                iced::Task::none()
            }
            TopoMsg::ComboFechar => {
                let atual = self.dir.clone();
                self.seletor.refazer(&atual);
                iced::Task::none()
            }
            TopoMsg::Escolher(o) if o.caminho.is_empty() => self.update(Msg::AbrirSeletor),
            TopoMsg::Escolher(o) if std::path::Path::new(&o.caminho) == self.dir => {
                iced::Task::none()
            }
            TopoMsg::Escolher(o) => self.trocar_para(&o.caminho),
            TopoMsg::AbrirPasta => {
                tarefa::plataforma(tarefa::Plataforma::Pasta(self.dir.clone()), Origem::Topo)
            }
            TopoMsg::AbrirCodigo => {
                tarefa::plataforma(tarefa::Plataforma::Codigo(self.dir.clone()), Origem::Topo)
            }
            TopoMsg::NovaJanela => {
                tarefa::plataforma(tarefa::Plataforma::NovaJanela(self.dir.clone()), Origem::Topo)
            }
            TopoMsg::Versao(r) => {
                self.topo.versao_hub = r.unwrap_or_else(|_| "schematize: ausente".to_string());
                iced::Task::none()
            }
            TopoMsg::GerenciarAbrir => {
                self.topo.gerenciar = true;
                self.topo.status = Aviso::default();
                iced::Task::none()
            }
            TopoMsg::GerenciarFechar => {
                self.topo.gerenciar = false;
                iced::Task::none()
            }
            TopoMsg::GerenciarCaminho(t) => {
                self.topo.caminho = t;
                self.topo.status = Aviso::default();
                iced::Task::none()
            }
            TopoMsg::Fixar => match projeto::validar(&self.topo.caminho) {
                Ok(p) => self.fixar(&["projects", "add"], p.display().to_string()),
                Err(e) => {
                    self.topo.status = Aviso::falha(e);
                    iced::Task::none()
                }
            },
            TopoMsg::Desafixar(p) => self.fixar(&["projects", "remove"], p),
        }
    }

    /// `schematize projects add|remove <pasta>`; a resposta vira [`Origem::Fixar`].
    fn fixar(&mut self, sub: &[&str], pasta: String) -> iced::Task<Msg> {
        let mut args: Vec<String> = sub.iter().map(|s| s.to_string()).collect();
        args.push(pasta);
        self.topo.status = Aviso::ok("aplicando…");
        tarefa::schematize(self.dir.clone(), args, Origem::Fixar)
    }

    /// **O quê:** o resultado de fixar/desafixar: aviso no modal e relista os projetos. **Onde:**
    /// [`App::resposta`].
    pub fn resposta_de_fixar(&mut self, msg: String, erro: bool) -> iced::Task<Msg> {
        self.topo.status = Aviso::de(msg, erro);
        if !erro {
            self.topo.caminho.clear();
        }
        tarefa::projetos(self.dir.clone())
    }

    /// **O quê:** o resultado de pasta/editor/nova janela: o aviso do topo. **Onde:** [`App::resposta`].
    pub fn resposta_do_topo(&mut self, msg: String, erro: bool) {
        self.topo.aviso = Aviso::de(msg, erro);
    }

    /// **O quê:** o cabeçalho inteiro: título, seletor, ícones, versão, sino, tema e o caminho.
    /// **Onde:** [`App::tela`].
    pub fn cabecalho(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let atual =
            self.seletor.opcoes.iter().find(|o| std::path::Path::new(&o.caminho) == self.dir);
        let seletor = combo_box(&self.seletor.combo, "escolha um projeto", atual, |o: Opcao| {
            Msg::Topo(TopoMsg::Escolher(o))
        })
        .on_open(Msg::Topo(TopoMsg::ComboAbrir))
        .on_close(Msg::Topo(TopoMsg::ComboFechar))
        .width(Length::Fixed(230.0))
        .padding([7, 10])
        .size(13)
        .input_style(tema::campo(c))
        .menu_style(tema::menu_lista(c));
        let icone = |glifo: String, dica: &'static str, m: Msg| {
            tooltip(
                btn_icone(c, glifo, Some(m)),
                txt(dica, 11.5, c.texto),
                tooltip::Position::Bottom,
            )
            .gap(4)
            .padding(6)
            .style(tema::cartao(c, c.borda, 6.0))
        };
        let tema_glifo = if c.escuro { "☾" } else { "☀" };
        let n = self.sino.nao_lidas();
        let sino = if n > 0 { format!("{G_SINO} {n}") } else { G_SINO.to_string() };
        let topo = row![
            negrito("Overdev", 20.0, c.titulo),
            txt("Projeto:", 13.0, c.dim),
            seletor,
            icone(G_PASTA.to_string(), "Abrir a pasta do projeto", Msg::Topo(TopoMsg::AbrirPasta)),
            icone(G_CODIGO.to_string(), "Abrir no VS Code", Msg::Topo(TopoMsg::AbrirCodigo)),
            icone(G_RELER.to_string(), "Recarregar", Msg::Reler),
            icone(G_FIXAR.to_string(), "Projetos fixados", Msg::Topo(TopoMsg::GerenciarAbrir)),
            icone(
                G_JANELA.to_string(),
                "Abrir o projeto em nova janela",
                Msg::Topo(TopoMsg::NovaJanela)
            ),
            Space::new().width(Fill),
            txt(&self.topo.versao_hub, 11.5, c.dim),
            icone(sino, "Notificações", Msg::Sino(SinoMsg::Alternar)),
            btn(c, tema_glifo, Some(Msg::AlternarTema), false, true),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let mut col = column![
            topo,
            row![
                txt(self.dir.display().to_string(), 11.5, c.dim),
                Space::new().width(Fill),
                txt(&self.versao, 11.0, c.dim)
            ]
        ]
        .spacing(6);
        if !self.topo.aviso.texto.is_empty() {
            col = col.push(status(c, &self.topo.aviso.texto, self.topo.aviso.erro, 12.0));
        }
        col.into()
    }

    /// **O quê:** o modal de projetos fixados, ou `None`. **Onde:** [`App::view`].
    pub fn ver_gerenciar(&self) -> Option<Element<'_, Msg>> {
        if !self.topo.gerenciar {
            return None;
        }
        let c = self.cores;
        let mut fixados = column![].spacing(6).width(Fill);
        let lista: Vec<_> = self.seletor.lista.iter().filter(|p| p.marker == "pinned").collect();
        if lista.is_empty() {
            fixados = fixados.push(txt("Nenhuma pasta fixada.", 12.0, c.dim));
        }
        for p in lista {
            fixados = fixados.push(
                row![
                    column![negrito(&p.name, 13.0, c.texto), txt(&p.path, 11.0, c.dim)].width(Fill),
                    btn(
                        c,
                        "Desafixar",
                        Some(Msg::Topo(TopoMsg::Desafixar(p.path.clone()))),
                        false,
                        true
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
        let campo = text_input("caminho da pasta a fixar como projeto", &self.topo.caminho)
            .on_input(|t| Msg::Topo(TopoMsg::GerenciarCaminho(t)))
            .on_submit(Msg::Topo(TopoMsg::Fixar))
            .size(13)
            .padding(8)
            .style(tema::campo(c));
        let corpo = column![
            negrito("Projetos fixados", 16.0, c.texto),
            txt("Uma pasta fixada vira UM projeto no seletor, mesmo sem marcador git (workspace/monorepo). Quem grava a lista é `schematize projects`.", 12.0, c.dim).width(Fill),
            fixados,
            row![campo, btn(c, "Fixar", (!self.topo.caminho.trim().is_empty()).then_some(Msg::Topo(TopoMsg::Fixar)), true, false)].spacing(8),
            txt("As pastas de desenvolvimento (varredura) ainda são editadas só no hub: o CLI não tem comando para isso.", 11.5, c.dim).width(Fill),
            status(c, &self.topo.status.texto, self.topo.status.erro, 12.0),
            row![Space::new().width(Fill), btn(c, "Fechar", Some(Msg::Topo(TopoMsg::GerenciarFechar)), false, false)],
        ]
        .spacing(12);
        Some(modal(c, container(corpo).into(), Msg::Topo(TopoMsg::GerenciarFechar), false))
    }
}

#[cfg(test)]
mod tests {
    use super::super::msg::Aba;
    use super::*;
    use overdev_casa::projects::Project;
    use std::path::PathBuf;

    fn app() -> App {
        App::novo(PathBuf::from("/p/a"), PathBuf::from("/n"), "v".into(), Aba::Projeto, true)
    }

    fn proj(nome: &str, caminho: &str, marker: &str) -> Project {
        Project { name: nome.into(), path: caminho.into(), marker: marker.into() }
    }

    #[test]
    fn os_cinco_icones_disparam_um_efeito_cada_sem_tocar_no_estado_do_projeto() {
        let mut a = app();
        for m in [TopoMsg::AbrirPasta, TopoMsg::AbrirCodigo, TopoMsg::NovaJanela] {
            assert_eq!(a.update(Msg::Topo(m.clone())).units(), 1, "{m:?}");
        }
        assert_eq!(a.update(Msg::Reler).units(), 1);
        assert_eq!(a.update(Msg::Topo(TopoMsg::GerenciarAbrir)).units(), 0);
        assert!(a.topo.gerenciar);
        assert_eq!(a.update(Msg::Topo(TopoMsg::GerenciarFechar)).units(), 0);
        assert!(!a.topo.gerenciar);
    }

    #[test]
    fn a_resposta_dos_icones_vira_o_aviso_do_topo() {
        let mut a = app();
        let _ = a.update(Msg::Resp(Origem::Topo, "não achei `code`".into(), true));
        assert!(a.topo.aviso.erro && a.topo.aviso.texto.contains("code"));
    }

    #[test]
    fn escolher_outro_caminho_abre_o_campo_e_o_mesmo_projeto_nao_faz_nada() {
        let mut a = app();
        let outro = Opcao { rotulo: "Outro caminho…".into(), caminho: String::new() };
        assert_eq!(a.update(Msg::Topo(TopoMsg::Escolher(outro))).units(), 1);
        assert!(a.seletor.aberto);
        let mesmo = Opcao { rotulo: "a".into(), caminho: "/p/a".into() };
        assert_eq!(a.update(Msg::Topo(TopoMsg::Escolher(mesmo))).units(), 0);
    }

    #[test]
    fn abrir_o_seletor_mostra_todos_e_fechar_volta_ao_atual() {
        let mut a = app();
        let _ =
            a.update(Msg::Projetos(vec![proj("a", "/p/a", "pinned"), proj("b", "/p/b", "git")]));
        assert_eq!(a.update(Msg::Topo(TopoMsg::ComboAbrir)).units(), 0);
        assert_eq!(a.seletor.combo.options().len(), 3, "todas as opções, sem filtro");
        assert_eq!(a.update(Msg::Topo(TopoMsg::ComboFechar)).units(), 0);
        assert_eq!(a.seletor.opcoes.len(), 3);
    }

    #[test]
    fn escolher_projeto_inexistente_mostra_o_erro_em_vez_de_trocar() {
        let mut a = app();
        let o = Opcao { rotulo: "x".into(), caminho: "/nao/existe/mesmo/42".into() };
        assert_eq!(a.update(Msg::Topo(TopoMsg::Escolher(o))).units(), 0);
        assert_eq!(a.dir, PathBuf::from("/p/a"), "não trocou");
        assert!(!a.seletor.erro.is_empty());
    }

    #[test]
    fn as_opcoes_trazem_o_atual_a_lista_e_outro_caminho_no_fim() {
        let lista = vec![proj("a", "/p/a", "pinned"), proj("b", "/p/b", "git")];
        let o = projeto::opcoes_de(&lista, std::path::Path::new("/p/a"));
        assert_eq!(
            o.iter().map(|x| x.rotulo.as_str()).collect::<Vec<_>>(),
            ["a", "b", "Outro caminho…"]
        );
        let fora = projeto::opcoes_de(&lista, std::path::Path::new("/z/novo"));
        assert_eq!((fora[0].rotulo.as_str(), fora[0].caminho.as_str()), ("novo", "/z/novo"));
        assert_eq!(fora.last().unwrap().caminho, "", "a última é o campo de caminho");
    }

    #[test]
    fn projetos_chegando_preenche_o_seletor_e_trocar_de_projeto_nao_perde_a_lista() {
        let mut a = app();
        let _ = a.update(Msg::Projetos(vec![proj("a", "/p/a", "pinned")]));
        assert_eq!(a.seletor.opcoes.len(), 2);
        a.trocar_projeto(PathBuf::from("/p/b"));
        assert_eq!(a.seletor.lista.len(), 1, "a lista sobrevive à troca");
        assert_eq!(a.seletor.opcoes.len(), 3, "o /p/b fora da varredura entra no topo");
        assert_eq!(a.seletor.opcoes[0].caminho, "/p/b");
    }

    #[test]
    fn fixar_exige_pasta_valida_e_vai_pelo_cli() {
        let mut a = app();
        let _ = a.update(Msg::Topo(TopoMsg::GerenciarCaminho("/nao/existe/42".into())));
        assert_eq!(a.update(Msg::Topo(TopoMsg::Fixar)).units(), 0);
        assert!(a.topo.status.erro, "pasta inexistente: erro no modal");
        let _ = a.update(Msg::Topo(TopoMsg::GerenciarCaminho(env!("CARGO_MANIFEST_DIR").into())));
        assert_eq!(a.update(Msg::Topo(TopoMsg::Fixar)).units(), 1);
        assert_eq!(a.update(Msg::Topo(TopoMsg::Desafixar("/p/b".into()))).units(), 1);
    }

    #[test]
    fn resposta_de_fixar_relista_e_so_limpa_o_campo_no_sucesso() {
        let mut a = app();
        a.topo.caminho = "/x".into();
        assert_eq!(a.update(Msg::Resp(Origem::Fixar, "recusou".into(), true)).units(), 1);
        assert_eq!(a.topo.caminho, "/x");
        let _ = a.update(Msg::Resp(Origem::Fixar, "ok".into(), false));
        assert!(a.topo.caminho.is_empty());
    }

    #[test]
    fn a_versao_do_hub_chega_ou_vira_ausente_sem_banner() {
        let mut a = app();
        let _ = a.update(Msg::Topo(TopoMsg::Versao(Ok("schematize v0.66.1".into()))));
        assert_eq!(a.topo.versao_hub, "schematize v0.66.1");
        let _ = a.update(Msg::Topo(TopoMsg::Versao(Err("não está instalado".into()))));
        assert_eq!(a.topo.versao_hub, "schematize: ausente");
        assert!(a.topo.aviso.texto.is_empty(), "a janela roda sem o hub: não é erro de quem abriu");
    }
}
