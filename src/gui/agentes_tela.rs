//! A aba AGENTES: quem faz o quê (papéis), em qual motor/modelo, a escada e o roteamento.
//!
//! **O quê:** o glue com o mundo (ler/gravar o `agentes.toml`, checar os motores em segundo plano)
//! e a view. Toda a lógica de edição mora em [`super::agentes_estado`] (pura e testada).
//!
//! **Onde:** o `update` ([`App::garantir_agentes`], [`App::atualizar_agentes`]) e o `view`
//! ([`App::ver_agentes`]). `disponibilidade` cria processos (`codex login status`): roda SEMPRE em
//! `Task::perform`, nunca no `view`/`update`.

use super::agentes_estado::{sugestoes, AgMsg, EscopoAg, EstadoAgentes};
use super::estado::App;
use super::msg::Msg;
use super::tema;
use super::widgets::{btn, cartao, negrito, rolagem, status, txt};
use iced::widget::{column, pick_list, row, text_input, toggler, Space};
use iced::{Alignment, Color, Element, Fill, Task};
use overdev_casa::agentes::{self, Agentes, Disponibilidade, Motor, Papel};

/// O vermelho do erro inline (o âmbar da paleta é aviso, não falha). **Onde:** [`App::ver_agentes`].
fn vermelho(escuro: bool) -> Color {
    if escuro {
        Color::from_rgb8(0xff, 0x6b, 0x6b)
    } else {
        Color::from_rgb8(0xb4, 0x1d, 0x1d)
    }
}

/// **O quê:** o texto de ajuda de um motor que não está pronto. **Onde:** a faixa de motores.
pub fn dica_do_motor(m: Motor, d: &Disponibilidade) -> Option<&'static str> {
    match (m, d) {
        (Motor::Codex, Disponibilidade::SemLogin(_)) => Some("rode `codex login`"),
        (Motor::Cursor, Disponibilidade::SemBinario) => Some("instale pelo schematize-market"),
        (Motor::Claude, Disponibilidade::SemLogin(_)) => Some("rode `claude` e faça o login"),
        (_, Disponibilidade::SemBinario) => Some("instale o binário e deixe no PATH"),
        _ => None,
    }
}

/// **O quê:** o rótulo de um motor (✓ / sem binário / sem login: …). **Onde:** a faixa de motores.
pub fn rotulo_do_motor(d: &Option<Disponibilidade>) -> String {
    match d {
        None => "checando…".to_string(),
        Some(Disponibilidade::Ok) => "✓".to_string(),
        Some(Disponibilidade::SemBinario) => "sem binário".to_string(),
        Some(Disponibilidade::SemLogin(x)) => format!("sem login: {x}"),
    }
}

/// **O quê:** a checagem assíncrona de UM motor. **Onde:** [`App::verificar_motores`]; um `Task`
/// por motor para que o `codex login status` lento não segure os outros.
fn checar(m: Motor) -> Task<Msg> {
    Task::perform(async move { agentes::disponibilidade(m) }, move |d| {
        Msg::Agentes(AgMsg::Motores(m, d))
    })
}

impl App {
    /// **O quê:** o projeto está disponível para o escopo «Este projeto»? **Onde:** o seletor.
    fn projeto_disponivel(&self) -> bool {
        self.dir.is_dir()
    }

    /// **O quê:** lê o arquivo do `escopo` para o editor. Erro de leitura/validação vai INLINE e o
    /// editor abre com o padrão (a pessoa conserta e salva). **Onde:** abrir a aba, trocar escopo.
    fn carregar_agentes(&mut self, escopo: EscopoAg) {
        let proj = (escopo == EscopoAg::Projeto).then_some(self.dir.as_path());
        let motores = std::mem::take(&mut self.agentes.motores);
        match agentes::carregar(proj) {
            Ok(a) => self.agentes = EstadoAgentes::com(escopo, a),
            Err(e) => {
                self.agentes = EstadoAgentes::com(escopo, Agentes::padrao());
                self.agentes.erro =
                    format!("não consegui ler a config atual ({e}); mostrando o padrão");
            }
        }
        if !motores.is_empty() {
            self.agentes.motores = motores;
        }
    }

    /// **O quê:** garante que a aba Agentes esteja carregada e com os motores checados quando ela
    /// está aberta. **Onde:** o fim de [`App::update`] (cobre `--aba agentes` e a troca de projeto).
    pub fn garantir_agentes(&mut self) -> Task<Msg> {
        if self.aba != super::msg::Aba::Agentes || self.agentes.carregado {
            return Task::none();
        }
        let escopo = self.agentes.escopo;
        self.carregar_agentes(escopo);
        self.verificar_motores()
    }

    /// **O quê:** marca todos como «checando…» e dispara a checagem. **Onde:** o botão «Verificar».
    fn verificar_motores(&mut self) -> Task<Msg> {
        for slot in &mut self.agentes.motores {
            slot.1 = None;
        }
        Task::batch(Motor::TODOS.map(checar))
    }

    /// **O quê:** trata as mensagens da aba. **Onde:** [`App::update`].
    pub fn atualizar_agentes(&mut self, m: AgMsg) -> Task<Msg> {
        match m {
            AgMsg::VerificarMotores => return self.verificar_motores(),
            AgMsg::Escopo(novo) => self.trocar_escopo(novo),
            AgMsg::Salvar => {
                let (escopo, dir) = (self.agentes.escopo, self.dir.clone());
                self.agentes.salvar_com(|a| match escopo {
                    EscopoAg::Global => agentes::salvar_global(a),
                    EscopoAg::Projeto => agentes::salvar_projeto(&dir, a),
                });
            }
            outra => self.agentes.aplicar(outra),
        }
        Task::none()
    }

    /// **O quê:** troca Global/Projeto; com alteração pendente RECUSA (nada se perde calado).
    /// **Onde:** o seletor de escopo.
    fn trocar_escopo(&mut self, novo: EscopoAg) {
        if novo == self.agentes.escopo {
            return;
        }
        if novo == EscopoAg::Projeto && !self.projeto_disponivel() {
            self.agentes.erro = "nenhum projeto selecionado: só o escopo Global.".into();
        } else if self.agentes.alterado() {
            self.agentes.erro =
                "há alterações não salvas: salve ou descarte antes de trocar de escopo.".into();
        } else {
            self.carregar_agentes(novo);
        }
    }

    /// **O quê:** a aba Agentes. **Onde:** [`super::view`].
    pub fn ver_agentes(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let x = &self.agentes;
        let mut col = column![self.faixa_de_escopo(), self.faixa_de_motores(), self.acoes()]
            .spacing(10)
            .width(Fill);
        // O erro e o aviso ficam LOGO abaixo dos botões: embaixo da lista sairiam da tela.
        if !x.erro.is_empty() {
            col = col.push(negrito(format!("✗ {}", x.erro), 12.5, vermelho(c.escuro)).width(Fill));
        }
        col = col.push(status(c, &x.aviso.texto, x.aviso.erro, 12.0));
        col = col.push(negrito("Papéis", 13.0, c.texto));
        for (nome, p) in &x.editado.papeis {
            col = col.push(self.linha_do_papel(nome, p));
        }
        col = col.push(self.novo_papel());
        col = col.push(self.bloco_escada());
        col = col.push(self.bloco_roteamento());
        rolagem(col)
    }

    /// **O quê:** «Global / Este projeto» + o arquivo editado + se o projeto tem override.
    fn faixa_de_escopo(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let x = &self.agentes;
        let (caminho, extra) = match x.escopo {
            EscopoAg::Global => (agentes::caminho_global(), String::new()),
            EscopoAg::Projeto => {
                let p = agentes::caminho_projeto(&self.dir);
                let existe = p.is_file();
                (
                    p,
                    if existe {
                        "· override existe".to_string()
                    } else {
                        "· sem override ainda (usa o global)".to_string()
                    },
                )
            }
        };
        let chip = |rot: &'static str, e: EscopoAg, ligado: bool| {
            btn(c, rot, ligado.then_some(Msg::Agentes(AgMsg::Escopo(e))), x.escopo == e, true)
        };
        let proj = self.projeto_disponivel();
        let linha = row![
            negrito("Editando", 13.0, c.texto),
            chip("Global", EscopoAg::Global, x.escopo != EscopoAg::Global),
            chip("Este projeto", EscopoAg::Projeto, proj && x.escopo != EscopoAg::Projeto),
            Space::new().width(Fill),
            if x.alterado() {
                negrito("● alterado", 12.0, c.aviso)
            } else {
                txt("sem alterações", 12.0, c.dim)
            },
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        column![linha, txt(format!("{} {extra}", caminho.display()), 11.0, c.dim).width(Fill)]
            .spacing(4)
            .into()
    }

    /// **O quê:** a faixa dos três motores + «Verificar». **Onde:** [`App::ver_agentes`].
    fn faixa_de_motores(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let mut linha =
            row![negrito("Motores", 13.0, c.texto)].spacing(14).align_y(Alignment::Center);
        for (m, d) in &self.agentes.motores {
            let ok = matches!(d, Some(Disponibilidade::Ok));
            let cor = if ok {
                c.bom
            } else if d.is_none() {
                c.dim
            } else {
                c.aviso
            };
            let mut bloco =
                column![row![negrito(m.nome(), 12.5, c.texto), txt(rotulo_do_motor(d), 12.0, cor)]
                    .spacing(6)]
                .spacing(2);
            if let Some(h) = d.as_ref().and_then(|d| dica_do_motor(*m, d)) {
                bloco = bloco.push(txt(h, 11.0, c.dim));
            }
            linha = linha.push(cartao(c, c.borda, 7.0, bloco));
        }
        linha = linha.push(btn(
            c,
            "Verificar",
            Some(Msg::Agentes(AgMsg::VerificarMotores)),
            false,
            true,
        ));
        linha.into()
    }

    /// **O quê:** a linha de um papel (nome, motor, modelo, ativo, remover + descrição).
    /// **Onde:** [`App::ver_agentes`].
    fn linha_do_papel<'a>(&'a self, nome: &'a str, p: &'a Papel) -> Element<'a, Msg> {
        let c = self.cores;
        let n = nome.to_string();
        let motores: Vec<String> = Motor::TODOS.iter().map(|m| m.nome().to_string()).collect();
        let sug = sugestoes(p.motor);
        let escolhido = sug.contains(&p.modelo).then(|| p.modelo.clone());
        let (n1, n2, n3, n4, n5) = (n.clone(), n.clone(), n.clone(), n.clone(), n.clone());
        let motor = pick_list(motores, Some(p.motor.nome().to_string()), move |m| {
            Msg::Agentes(AgMsg::Motor(n1.clone(), m))
        })
        .width(96)
        .text_size(13)
        .style(estilo_lista(c))
        .menu_style(tema::menu_lista(c));
        let sugerido =
            pick_list(sug, escolhido, move |m| Msg::Agentes(AgMsg::Modelo(n2.clone(), m)))
                .placeholder("sugestões")
                .width(130)
                .text_size(13)
                .style(estilo_lista(c))
                .menu_style(tema::menu_lista(c));
        let modelo = text_input("modelo", &p.modelo)
            .on_input(move |t| Msg::Agentes(AgMsg::Modelo(n3.clone(), t)))
            .size(13)
            .padding(6)
            .width(150)
            .style(tema::campo(c));
        let ativo =
            toggler(p.ativo).on_toggle(move |v| Msg::Agentes(AgMsg::Ativo(n4.clone(), v))).size(18);
        let remover = (nome != "principal").then(|| Msg::Agentes(AgMsg::RemoverPapel(n5.clone())));
        let topo = row![
            negrito(nome, 13.0, c.texto).width(120),
            motor,
            modelo,
            sugerido,
            txt("ativo", 11.5, c.dim),
            ativo,
            Space::new().width(Fill),
            btn(c, "✕", remover, false, true),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let desc = text_input("descrição", &p.descricao)
            .on_input(move |t| Msg::Agentes(AgMsg::Descricao(n.clone(), t)))
            .size(12)
            .padding(6)
            .style(tema::campo(c));
        cartao(c, c.borda, 8.0, column![topo, desc].spacing(6))
    }

    /// **O quê:** o campo + botão «Adicionar papel». **Onde:** [`App::ver_agentes`].
    fn novo_papel(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let campo = text_input("novo papel (a-z, 0-9, -)", &self.agentes.novo_nome)
            .on_input(|t| Msg::Agentes(AgMsg::NovoNome(t)))
            .on_submit(Msg::Agentes(AgMsg::AdicionarPapel))
            .size(13)
            .padding(7)
            .style(tema::campo(c));
        let ok = !self.agentes.novo_nome.trim().is_empty();
        row![
            campo,
            btn(
                c,
                "Adicionar papel",
                ok.then_some(Msg::Agentes(AgMsg::AdicionarPapel)),
                false,
                true
            )
        ]
        .spacing(6)
        .into()
    }

    /// **O quê:** a escada ordenada (↑ ↓ ✕) e o «adicionar à escada». **Onde:** [`App::ver_agentes`].
    fn bloco_escada(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let x = &self.agentes;
        let ultimo = x.editado.escada.len().saturating_sub(1);
        let mut col = column![
            negrito("Escada de escalonamento", 13.0, c.texto),
            txt("tarefa de código sobe a escada de cima para baixo; esgotou → park.", 11.0, c.dim),
        ]
        .spacing(6);
        for (i, papel) in x.editado.escada.iter().enumerate() {
            let linha = row![
                txt(format!("{}.", i + 1), 12.0, c.dim).width(24),
                negrito(papel, 12.5, c.texto).width(Fill),
                btn(c, "↑", (i > 0).then_some(Msg::Agentes(AgMsg::Subir(i))), false, true),
                btn(c, "↓", (i < ultimo).then_some(Msg::Agentes(AgMsg::Descer(i))), false, true),
                btn(c, "✕", Some(Msg::Agentes(AgMsg::RemoverDaEscada(i))), false, true),
            ]
            .spacing(6)
            .align_y(Alignment::Center);
            col = col.push(cartao(c, c.borda, 6.0, linha));
        }
        let seletor = pick_list(x.nomes(), x.para_escada.clone(), |p| {
            Msg::Agentes(AgMsg::EscolherParaEscada(p))
        })
        .placeholder("adicionar papel à escada")
        .width(240)
        .text_size(13)
        .style(estilo_lista(c))
        .menu_style(tema::menu_lista(c));
        let add = btn(
            c,
            "Adicionar",
            x.para_escada.is_some().then_some(Msg::Agentes(AgMsg::AdicionarNaEscada)),
            false,
            true,
        );
        col.push(row![seletor, add].spacing(6).align_y(Alignment::Center)).into()
    }

    /// **O quê:** o roteamento por tipo de tarefa (visual, imagem). **Onde:** [`App::ver_agentes`].
    fn bloco_roteamento(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let x = &self.agentes;
        let mut col = column![negrito("Roteamento (pula a escada)", 13.0, c.texto)].spacing(6);
        for tipo in ["visual", "imagem"] {
            let atual = x.editado.roteamento.get(tipo).cloned();
            let t = tipo.to_string();
            let sel =
                pick_list(x.nomes(), atual, move |p| Msg::Agentes(AgMsg::Rotear(t.clone(), p)))
                    .width(200)
                    .text_size(13)
                    .style(estilo_lista(c))
                    .menu_style(tema::menu_lista(c));
            col = col.push(
                row![txt(tipo, 12.5, c.texto).width(70), txt("→", 12.5, c.dim), sel]
                    .spacing(8)
                    .align_y(Alignment::Center),
            );
        }
        col.into()
    }

    /// **O quê:** Salvar / Restaurar padrão / Descartar. **Onde:** [`App::ver_agentes`].
    fn acoes(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let sujo = self.agentes.alterado();
        row![
            btn(c, "Salvar", Some(Msg::Agentes(AgMsg::Salvar)), true, false),
            btn(c, "Restaurar padrão", Some(Msg::Agentes(AgMsg::Padrao)), false, false),
            btn(
                c,
                "Descartar alterações",
                sujo.then_some(Msg::Agentes(AgMsg::Descartar)),
                false,
                false
            ),
        ]
        .spacing(8)
        .into()
    }
}

/// **O quê:** o estilo do `pick_list` na paleta da janela. **Onde:** todos os seletores da aba.
fn estilo_lista(c: tema::Cores) -> impl Fn(&iced::Theme, pick_list::Status) -> pick_list::Style {
    move |_, st| pick_list::Style {
        text_color: c.texto,
        placeholder_color: c.dim,
        handle_color: c.dim,
        background: iced::Background::Color(c.bg),
        border: iced::Border {
            color: if matches!(st, pick_list::Status::Opened { .. }) { c.accent } else { c.borda },
            width: 1.0,
            radius: iced::border::radius(8),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// As dicas e rótulos da faixa de motores seguem o combinado com o usuário.
    #[test]
    fn rotulos_e_dicas_dos_motores() {
        assert_eq!(rotulo_do_motor(&Some(Disponibilidade::Ok)), "✓");
        assert_eq!(rotulo_do_motor(&Some(Disponibilidade::SemBinario)), "sem binário");
        assert_eq!(rotulo_do_motor(&Some(Disponibilidade::SemLogin("x".into()))), "sem login: x");
        let login = Disponibilidade::SemLogin("x".into());
        assert_eq!(dica_do_motor(Motor::Codex, &login), Some("rode `codex login`"));
        assert_eq!(
            dica_do_motor(Motor::Cursor, &Disponibilidade::SemBinario),
            Some("instale pelo schematize-market")
        );
        assert_eq!(dica_do_motor(Motor::Claude, &Disponibilidade::Ok), None);
    }
}
