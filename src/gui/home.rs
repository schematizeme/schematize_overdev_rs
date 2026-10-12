//! A aba PROJETO: a tela de Overdev do hub (ações de skills, atalhos, skills atrasadas, commits) e o
//! botão que o hub não tem — **Iniciar overdev**.
//!
//! **O quê:** o estado, o `update` e o `view` da aba; mais [`ler_dados`], que consulta os três
//! binários irmãos em paralelo (`schematize-skills acoes`, `schematize-skills applied`,
//! `schematize-git log`) — o mesmo contrato do hub, sem copiar domínio.
//!
//! **Onde:** [`super::carga`] (a leitura), [`super::update`] (as mensagens) e [`super::view`].
//!
//! ## «Iniciar overdev» e o run que já existe
//!
//! `schematize-overdev start "<objetivo>"` REABRE o estado do run (modo ativo, ciclos a zero) no
//! diretório, mantendo o checklist. Por isso, com um run ativo a janela pede CONFIRMAÇÃO antes — e
//! sem objetivo escrito nem chega a chamar o CLI. Quem grava é o CLI (D6).
//!
//! ## Não há push, e não foi inventado
//!
//! O hub só LISTA os commits (marca os já enviados e mostra o upstream); `schematize-git` não tem
//! subcomando de push. Um botão de push seria domínio novo escrito na casca: fica como lacuna do
//! repo `schematize_git_rs` (ver o relatório da paridade).

use super::estado::{App, Aviso};
use super::externos::{self, Acao, Aplicadas, Commit, Upstream};
use super::msg::{HomeMsg, Msg, Origem};
use super::widgets::{btn, cartao, negrito, rolagem, separador, status, txt};
use super::{carga::Escopo, tarefa, tema};
use iced::widget::{column, container, row, text_input, Space};
use iced::{Alignment, Element, Fill, Length};
use std::path::Path;

/// Commits por página (o mesmo `PAGE` do hub).
pub const POR_PAGINA: usize = 20;

/// O que os três binários irmãos disseram sobre o projeto. Cada leitura falha sozinha.
#[derive(Debug, Clone)]
pub struct DadosHome {
    pub acoes: Result<Vec<Acao>, String>,
    pub aplicadas: Result<Aplicadas, String>,
    pub commits: Result<(Option<Upstream>, Vec<Commit>), String>,
}

/// O estado da aba.
#[derive(Debug, Default)]
pub struct Home {
    pub acoes: Vec<Acao>,
    pub acoes_erro: String,
    pub aplicadas: Aplicadas,
    pub aplicadas_erro: String,
    pub commits: Vec<Commit>,
    pub upstream: Option<Upstream>,
    pub commits_erro: String,
    pub pagina: usize,
    pub status: Aviso,
    /// O objetivo digitado em «Iniciar overdev».
    pub objetivo: String,
    /// Há um run ativo e a pessoa já clicou em Iniciar: espera o «Confirmar».
    pub confirmar_iniciar: bool,
}

/// **O quê:** consulta os três binários em paralelo. **Onde:** [`super::carga::carregar`].
pub fn ler_dados(dir: &Path) -> DadosHome {
    std::thread::scope(|s| {
        let acoes = s.spawn(|| {
            externos::rodar("schematize-skills", dir, &["acoes", "--json"])
                .and_then(|t| externos::ler_acoes(&t))
        });
        let aplicadas = s.spawn(|| {
            externos::rodar("schematize-skills", dir, &["applied", "--json"])
                .and_then(|t| externos::ler_aplicadas(&t))
        });
        let commits = s.spawn(|| {
            externos::rodar("schematize-git", dir, &["log", "--limite", "50", "--json"])
                .and_then(|t| externos::ler_commits(&t))
        });
        let falhou = || "a leitura falhou por dentro (pânico na thread)".to_string();
        DadosHome {
            acoes: acoes.join().unwrap_or_else(|_| Err(falhou())),
            aplicadas: aplicadas.join().unwrap_or_else(|_| Err(falhou())),
            commits: commits.join().unwrap_or_else(|_| Err(falhou())),
        }
    })
}

/// **O quê:** quantas páginas de commits (no mínimo 1). **Onde:** o paginador.
pub fn paginas(total: usize) -> usize {
    total.div_ceil(POR_PAGINA).max(1)
}

impl App {
    /// **O quê:** escreve a leitura na aba. **Onde:** [`App::aplicar_carga`]. Cada erro vai ao lugar
    /// dele; a falha de um app não apaga o que os outros disseram.
    pub fn aplicar_home(&mut self, d: &DadosHome) {
        let h = &mut self.home;
        match &d.acoes {
            Ok(a) => (h.acoes, h.acoes_erro) = (a.clone(), String::new()),
            Err(e) => (h.acoes, h.acoes_erro) = (Vec::new(), e.clone()),
        }
        match &d.aplicadas {
            Ok(a) => (h.aplicadas, h.aplicadas_erro) = (a.clone(), String::new()),
            Err(e) => (h.aplicadas, h.aplicadas_erro) = (Aplicadas::default(), e.clone()),
        }
        match &d.commits {
            Ok((up, cs)) => {
                (h.upstream, h.commits, h.commits_erro) = (up.clone(), cs.clone(), String::new());
            }
            Err(e) => (h.upstream, h.commits, h.commits_erro) = (None, Vec::new(), e.clone()),
        }
        h.pagina = h.pagina.min(paginas(h.commits.len()) - 1);
    }

    /// **O quê:** existe um run ATIVO neste projeto? **Onde:** o «Iniciar overdev».
    pub fn run_ativo(&self) -> bool {
        self.tem_run && self.modo == "active"
    }

    /// **O quê:** trata as mensagens da aba. **Onde:** [`App::update`].
    pub fn atualizar_home(&mut self, m: HomeMsg) -> iced::Task<Msg> {
        match m {
            HomeMsg::Objetivo(t) => {
                self.home.objetivo = t;
                self.home.confirmar_iniciar = false;
                iced::Task::none()
            }
            HomeMsg::Iniciar => self.pedir_iniciar(),
            HomeMsg::IniciarConfirmar => {
                self.home.confirmar_iniciar = false;
                self.iniciar_run()
            }
            HomeMsg::IniciarCancelar => {
                self.home.confirmar_iniciar = false;
                self.home.status = Aviso::ok("início cancelado — o run ativo segue como estava.");
                iced::Task::none()
            }
            HomeMsg::Skill(i) => match self.home.acoes.get(i).cloned() {
                Some(a) => self.abrir_no_terminal(&a.rotulo, &["--prompt", &a.comando]),
                None => {
                    self.home.status =
                        Aviso::falha("essa ação de skill não existe mais — recarregue.");
                    iced::Task::none()
                }
            },
            HomeMsg::GerarAfazeres => {
                self.abrir_no_terminal("Afazeres do archive", &["--receita", "archive-todos"])
            }
            HomeMsg::Terminal => self.abrir_no_terminal("Terminal", &[]),
            HomeMsg::Rerodar => {
                let prompt = self.home.aplicadas.prompt.clone();
                if prompt.is_empty() {
                    self.home.status = Aviso::falha("não há skill atrasada para reaplicar.");
                    return iced::Task::none();
                }
                self.abrir_no_terminal("Rerodar skills", &["--prompt", &prompt])
            }
            HomeMsg::Navegador => {
                tarefa::cli(self.dir.clone(), vec!["panel".to_string()], Origem::Home)
            }
            HomeMsg::Recarregar => {
                self.home.status = Aviso::default();
                self.recarregar(Escopo::TUDO)
            }
            HomeMsg::Pagina(p) => {
                self.home.pagina = p.min(paginas(self.home.commits.len()) - 1);
                iced::Task::none()
            }
        }
    }

    /// `terminal --json [extra…]` no projeto; o resultado volta como [`Origem::Home`].
    fn abrir_no_terminal(&mut self, rotulo: &str, extra: &[&str]) -> iced::Task<Msg> {
        let mut args = vec!["terminal".to_string(), "--json".to_string()];
        args.extend(extra.iter().map(|s| s.to_string()));
        self.home.status = Aviso::ok(format!("{rotulo}: abrindo o terminal…"));
        tarefa::terminal_json(self.dir.clone(), args, rotulo.to_string(), Origem::Home)
    }

    /// O clique em «Iniciar overdev»: valida, e com run ativo pede confirmação.
    fn pedir_iniciar(&mut self) -> iced::Task<Msg> {
        if self.home.objetivo.trim().is_empty() {
            self.home.status = Aviso::falha("escreva o objetivo antes de iniciar o overdev.");
            return iced::Task::none();
        }
        if self.run_ativo() {
            self.home.confirmar_iniciar = true;
            self.home.status = Aviso::default();
            return iced::Task::none();
        }
        self.iniciar_run()
    }

    /// `schematize-overdev start -- "<objetivo>"` — o `--` impede que um objetivo começado em `-`
    /// vire flag do CLI.
    fn iniciar_run(&mut self) -> iced::Task<Msg> {
        let obj = self.home.objetivo.trim().to_string();
        self.home.status = Aviso::ok("iniciando o overdev…");
        let args = vec!["start".to_string(), "--".to_string(), obj];
        tarefa::cli(self.dir.clone(), args, Origem::Iniciar)
    }

    /// **O quê:** o resultado das ações da aba. **Onde:** [`App::resposta`].
    pub fn resposta_da_home(&mut self, msg: String, erro: bool) {
        let msg = if msg.is_empty() && !erro { "feito.".to_string() } else { msg };
        self.home.status = Aviso::de(msg, erro);
    }

    /// **O quê:** o resultado do `start`: com sucesso limpa o campo e relê tudo (o run novo aparece
    /// no progresso). **Onde:** [`App::resposta`].
    pub fn resposta_de_iniciar(&mut self, msg: String, erro: bool) -> iced::Task<Msg> {
        self.home.status = Aviso::de(msg, erro);
        if erro {
            return iced::Task::none();
        }
        self.home.objetivo.clear();
        self.recarregar(Escopo::TUDO)
    }

    /// **O quê:** a aba Projeto. **Onde:** [`App::tela`].
    pub fn ver_home(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let h = &self.home;
        let mut col =
            column![self.ver_iniciar(), self.ver_acoes_de_skills()].spacing(12).width(Fill);
        col = col.push(
            row![
                btn(
                    c,
                    "Gerar afazeres do archive",
                    Some(Msg::Home(HomeMsg::GerarAfazeres)),
                    true,
                    true
                ),
                btn(c, "abrir no navegador", Some(Msg::Home(HomeMsg::Navegador)), false, true),
                btn(c, "abrir no terminal", Some(Msg::Home(HomeMsg::Terminal)), false, true),
                btn(c, "Recarregar", Some(Msg::Home(HomeMsg::Recarregar)), false, true),
            ]
            .spacing(8),
        );
        col = col.push(status(c, &h.status.texto, h.status.erro, 12.0));
        if !h.aplicadas.atrasadas.is_empty() {
            col = col.push(self.ver_skills_atrasadas());
        } else if !h.aplicadas_erro.is_empty() {
            col = col.push(txt(format!("Skills: {}", h.aplicadas_erro), 11.5, c.dim).width(Fill));
        }
        col = col.push(separador(c)).push(self.ver_commits());
        rolagem(col)
    }

    /// O cartão «Iniciar overdev»: objetivo + botão (+ a confirmação quando há run ativo).
    fn ver_iniciar(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let h = &self.home;
        let campo =
            text_input("objetivo do overdev (ex.: migrar o auth para o serviço novo)", &h.objetivo)
                .on_input(|t| Msg::Home(HomeMsg::Objetivo(t)))
                .on_submit(Msg::Home(HomeMsg::Iniciar))
                .size(13)
                .padding(9)
                .style(tema::campo(c));
        let mut col = column![
            negrito("Iniciar overdev", 13.5, c.texto),
            txt(
                "Começa um run novo neste projeto: `schematize-overdev start \"<objetivo>\"`.",
                12.0,
                c.dim
            )
            .width(Fill),
            row![campo, btn(c, "Iniciar overdev", Some(Msg::Home(HomeMsg::Iniciar)), true, false)]
                .spacing(8)
                .align_y(Alignment::Center),
        ]
        .spacing(8);
        if self.run_ativo() && !h.confirmar_iniciar {
            col = col.push(
                txt(
                    format!(
                        "Há um run ATIVO aqui: «{}». Iniciar de novo pede confirmação.",
                        self.objetivo
                    ),
                    12.0,
                    c.aviso,
                )
                .width(Fill),
            );
        }
        if h.confirmar_iniciar {
            let aviso = column![
                negrito("Já existe um run ativo neste projeto.", 12.5, c.aviso),
                txt(format!("Objetivo atual: «{}». Iniciar reabre o run com o objetivo novo (ciclos voltam a zero; o checklist é mantido).", self.objetivo), 12.0, c.texto).width(Fill),
                row![
                    btn(c, "Confirmar: iniciar de novo", Some(Msg::Home(HomeMsg::IniciarConfirmar)), true, true),
                    btn(c, "Cancelar", Some(Msg::Home(HomeMsg::IniciarCancelar)), false, true),
                ]
                .spacing(8),
            ]
            .spacing(6);
            col = col.push(
                container(aviso).padding(10).width(Fill).style(tema::cartao(c, c.aviso, 10.0)),
            );
        }
        cartao(c, c.borda, 12.0, col)
    }

    /// «Ações de skills»: um botão por ação declarada, ou o motivo de não haver nenhum.
    fn ver_acoes_de_skills(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let h = &self.home;
        let mut col = column![negrito("Ações de skills", 12.0, c.dim)].spacing(8);
        if !h.acoes_erro.is_empty() {
            col = col.push(txt(&h.acoes_erro, 12.0, c.aviso).width(Fill));
        } else if h.acoes.is_empty() {
            col = col.push(txt("Nenhuma skill instalada declara ações.", 12.0, c.dim));
        } else {
            let botoes = h.acoes.iter().enumerate().map(|(i, a)| {
                btn(c, a.rotulo.clone(), Some(Msg::Home(HomeMsg::Skill(i))), false, true)
            });
            col = col.push(row(botoes).spacing(8).wrap());
        }
        col.into()
    }

    /// O aviso âmbar «N skills desatualizadas» + «Rerodar skills».
    fn ver_skills_atrasadas(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let a = &self.home.aplicadas;
        let resumo = a
            .atrasadas
            .iter()
            .map(|(s, de, para)| format!("{s} v{de} → v{para}"))
            .collect::<Vec<_>>()
            .join(" · ");
        let corpo = row![
            column![
                negrito(
                    format!("{} skill(s) desatualizada(s) neste projeto", a.atrasadas.len()),
                    12.5,
                    c.aviso
                ),
                txt(resumo, 11.5, c.dim).width(Fill),
            ]
            .spacing(3)
            .width(Fill),
            btn(c, "Rerodar skills", Some(Msg::Home(HomeMsg::Rerodar)), true, true),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        container(corpo)
            .padding(12)
            .width(Fill)
            .style(tema::cartao(c, tema::com_alfa(c.aviso, 0.6), 10.0))
            .into()
    }

    /// «Commits e push»: a linha de upstream, a página de commits e o paginador.
    fn ver_commits(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let h = &self.home;
        let mut col = column![negrito("Commits e push", 14.0, c.texto)].spacing(8).width(Fill);
        if let Some(u) = &h.upstream {
            col = col.push(negrito(u.linha(), 12.0, c.accent));
        }
        if !h.commits_erro.is_empty() {
            col = col.push(
                txt(format!("Não consegui ler os commits: {}", h.commits_erro), 12.0, c.aviso)
                    .width(Fill),
            );
        } else if h.commits.is_empty() {
            col = col.push(txt("Sem commits (ou não é um repositório git).", 12.0, c.dim));
        }
        for cm in h.commits.iter().skip(h.pagina * POR_PAGINA).take(POR_PAGINA) {
            let (marca, cor) = if cm.pushed { ("●", c.bom) } else { ("○", c.dim) };
            let linha = row![
                txt(marca, 13.0, cor).width(16),
                txt(&cm.short, 12.0, c.accent).width(70),
                txt(&cm.date, 11.5, c.dim).width(90),
                // `clip`: um assunto comprido é cortado na coluna, não escorre sobre o autor.
                container(
                    txt(&cm.subject, 12.5, c.texto).wrapping(iced::widget::text::Wrapping::None)
                )
                .width(Fill)
                .clip(true),
                container(
                    txt(&cm.author, 11.5, c.dim).wrapping(iced::widget::text::Wrapping::None)
                )
                .width(110)
                .clip(true),
            ]
            .spacing(10)
            .align_y(Alignment::Center);
            col = col.push(
                container(linha).padding([10, 12]).width(Fill).style(tema::cartao(c, c.borda, 8.0)),
            );
        }
        if h.commits.len() > POR_PAGINA {
            let n = paginas(h.commits.len());
            col = col.push(
                row![
                    btn(
                        c,
                        "‹ anterior",
                        (h.pagina > 0)
                            .then(|| Msg::Home(HomeMsg::Pagina(h.pagina.saturating_sub(1)))),
                        false,
                        true
                    ),
                    txt(format!("página {}/{}", h.pagina + 1, n), 12.0, c.dim),
                    btn(
                        c,
                        "próxima ›",
                        (h.pagina + 1 < n).then(|| Msg::Home(HomeMsg::Pagina(h.pagina + 1))),
                        false,
                        true
                    ),
                    Space::new().width(Length::Fill),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            );
        }
        col.into()
    }
}

#[cfg(test)]
mod tests {
    use super::super::estado::Aviso;
    use super::super::msg::Aba;
    use super::*;
    use std::path::PathBuf;

    fn app() -> App {
        App::novo(PathBuf::from("/p/a"), PathBuf::from("/n"), "v".into(), Aba::Projeto, true)
    }

    fn commit(n: usize) -> Commit {
        Commit {
            short: format!("c{n}"),
            author: "a".into(),
            date: "d".into(),
            subject: "s".into(),
            pushed: n.is_multiple_of(2),
        }
    }

    #[test]
    fn iniciar_sem_objetivo_nao_chama_o_cli() {
        let mut a = app();
        assert_eq!(a.update(Msg::Home(HomeMsg::Iniciar)).units(), 0);
        assert!(a.home.status.erro && a.home.status.texto.contains("objetivo"));
        let _ = a.update(Msg::Home(HomeMsg::Objetivo("   ".into())));
        assert_eq!(a.update(Msg::Home(HomeMsg::Iniciar)).units(), 0, "só espaço também é vazio");
    }

    #[test]
    fn iniciar_sem_run_vai_direto_ao_cli() {
        let mut a = app();
        let _ = a.update(Msg::Home(HomeMsg::Objetivo(" migrar o auth ".into())));
        assert_eq!(a.update(Msg::Home(HomeMsg::Iniciar)).units(), 1);
        assert!(!a.home.confirmar_iniciar);
    }

    #[test]
    fn com_run_ativo_pede_confirmacao_e_so_o_confirmar_chama_o_cli() {
        let mut a = app();
        (a.tem_run, a.modo, a.objetivo) = (true, "active".into(), "o velho".into());
        let _ = a.update(Msg::Home(HomeMsg::Objetivo("novo".into())));
        assert_eq!(a.update(Msg::Home(HomeMsg::Iniciar)).units(), 0, "ainda não chamou");
        assert!(a.home.confirmar_iniciar);
        assert_eq!(a.update(Msg::Home(HomeMsg::IniciarCancelar)).units(), 0);
        assert!(!a.home.confirmar_iniciar && !a.home.status.erro);
        let _ = a.update(Msg::Home(HomeMsg::Iniciar));
        assert_eq!(a.update(Msg::Home(HomeMsg::IniciarConfirmar)).units(), 1);
        assert!(!a.home.confirmar_iniciar);
    }

    #[test]
    fn run_parado_nao_pede_confirmacao() {
        let mut a = app();
        (a.tem_run, a.modo) = (true, "stopped".into());
        let _ = a.update(Msg::Home(HomeMsg::Objetivo("x".into())));
        assert_eq!(a.update(Msg::Home(HomeMsg::Iniciar)).units(), 1);
    }

    #[test]
    fn mexer_no_objetivo_desfaz_a_confirmacao() {
        let mut a = app();
        a.home.confirmar_iniciar = true;
        let _ = a.update(Msg::Home(HomeMsg::Objetivo("outro".into())));
        assert!(!a.home.confirmar_iniciar, "o objetivo mudou: a confirmação era de outro texto");
    }

    #[test]
    fn start_ok_limpa_o_campo_e_rele_start_com_erro_mostra_o_motivo() {
        let mut a = app();
        a.home.objetivo = "x".into();
        let t = a.update(Msg::Resp(Origem::Iniciar, "overdev ATIVO. Objetivo: x".into(), false));
        assert!(a.home.objetivo.is_empty() && !a.home.status.erro);
        assert_eq!(t.units(), 1, "relê tudo");
        a.home.objetivo = "y".into();
        let t = a.update(Msg::Resp(Origem::Iniciar, "recusado: HOME".into(), true));
        assert_eq!(a.home.objetivo, "y", "falhou: o texto digitado fica");
        assert!(a.home.status.erro && a.home.status.texto == "recusado: HOME");
        assert_eq!(t.units(), 0);
    }

    #[test]
    fn skill_fora_do_indice_e_erro_e_a_valida_abre_o_terminal() {
        let mut a = app();
        assert_eq!(a.update(Msg::Home(HomeMsg::Skill(3))).units(), 0);
        assert!(a.home.status.erro);
        a.home.acoes =
            vec![Acao { rotulo: "Q.A.".into(), comando: "/qa-plan".into(), precisa_projeto: true }];
        assert_eq!(a.update(Msg::Home(HomeMsg::Skill(0))).units(), 1);
        assert!(a.home.status.texto.starts_with("Q.A."));
    }

    #[test]
    fn rerodar_sem_prompt_nao_abre_nada() {
        let mut a = app();
        assert_eq!(a.update(Msg::Home(HomeMsg::Rerodar)).units(), 0);
        assert!(a.home.status.erro);
        a.home.aplicadas = Aplicadas {
            atrasadas: vec![("qa".into(), "1".into(), "2".into())],
            prompt: "p".into(),
        };
        assert_eq!(a.update(Msg::Home(HomeMsg::Rerodar)).units(), 1);
    }

    #[test]
    fn atalhos_do_projeto_disparam_um_efeito_cada() {
        let mut a = app();
        for m in [HomeMsg::GerarAfazeres, HomeMsg::Terminal, HomeMsg::Navegador] {
            assert_eq!(a.update(Msg::Home(m.clone())).units(), 1, "{m:?}");
        }
        assert_eq!(a.update(Msg::Home(HomeMsg::Recarregar)).units(), 1);
    }

    #[test]
    fn resposta_das_acoes_vira_status_e_vazio_ok_vira_feito() {
        let mut a = app();
        let _ = a.update(Msg::Resp(Origem::Home, String::new(), false));
        assert_eq!(a.home.status, Aviso::ok("feito."));
        let _ = a.update(Msg::Resp(Origem::Home, "não achei um terminal".into(), true));
        assert!(a.home.status.erro);
    }

    #[test]
    fn a_carga_preenche_cada_lugar_e_o_erro_de_um_nao_apaga_o_outro() {
        let mut a = app();
        a.aplicar_home(&DadosHome {
            acoes: Err("o app `schematize-skills` não está instalado".into()),
            aplicadas: Ok(Aplicadas::default()),
            commits: Ok((None, (0..45).map(commit).collect())),
        });
        assert!(a.home.acoes.is_empty() && a.home.acoes_erro.contains("não está instalado"));
        assert_eq!(a.home.commits.len(), 45);
        assert_eq!(paginas(45), 3);
        a.aplicar_home(&DadosHome {
            acoes: Ok(vec![]),
            aplicadas: Err("x".into()),
            commits: Err("not a repo".into()),
        });
        assert!(a.home.commits.is_empty() && a.home.commits_erro == "not a repo");
        assert!(a.home.acoes_erro.is_empty() && a.home.aplicadas_erro == "x");
    }

    #[test]
    fn a_pagina_de_commits_fica_presa_nos_limites() {
        let mut a = app();
        a.home.commits = (0..45).map(commit).collect();
        let _ = a.update(Msg::Home(HomeMsg::Pagina(99)));
        assert_eq!(a.home.pagina, 2);
        let _ = a.update(Msg::Home(HomeMsg::Pagina(0)));
        assert_eq!(a.home.pagina, 0);
        assert_eq!(paginas(0), 1);
        assert_eq!(paginas(20), 1);
        assert_eq!(paginas(21), 2);
    }
}
