//! A aba PERGUNTAS: a fila do quiz, que se responde clicando.
//!
//! **O quê:** o desenho dos cards ([`App::ver_perguntas`]) e a lógica de responder/marcar/recusar
//! ([`App::atualizar_perguntas`]).
//!
//! **Onde:** a aba aberta por padrão; é o que a pessoa vem fazer aqui.
//!
//! ## Quem grava é o CLI, e a janela repassa o motivo
//!
//! Responder é `schematize-overdev reply …`: o CLI GRAVA e VALIDA, e recusa a resposta inválida
//! com o motivo (a primeira linha da mensagem vira o banner). A janela não consegue gravar uma
//! resposta inválida nem se quiser. A forma da pergunta na tela (`kind`) É a guarda: cada kind
//! desenha um controle diferente, e só `livre` e o «outro» do `sim_nao_outro` aceitam texto.

use super::estado::{App, Aviso};
use super::msg::{Msg, Origem};
use super::telas::Card;
use super::tema::{self, Cores};
use super::widgets::{btn, cartao, negrito, rolagem, status, txt};
use super::{carga::Escopo, tarefa};
use iced::widget::{button, column, container, row, text_input, Space};
use iced::{Element, Fill};

impl App {
    /// **O quê:** roda um subcomando da fila (`reply`/`refuse`); a resposta chega como
    /// `Msg::Resp(Origem::Quiz, …)` e RELÊ tudo.
    ///
    /// **Onde:** todas as respostas. Relê depois de CADA resposta — senão o card responderia e
    /// continuaria desenhado como aberto, e a pessoa clicaria de novo.
    fn aplicar_quiz(&self, args: &[&str]) -> iced::Task<Msg> {
        tarefa::cli(self.dir.clone(), args.iter().map(|s| s.to_string()).collect(), Origem::Quiz)
    }

    /// **O quê:** trata as mensagens da aba Perguntas.
    ///
    /// **Onde:** [`App::update`]. A resposta vazia não faz a viagem ao CLI ("escreva a resposta"),
    /// e o `multipla` sem marca idem — dizer aqui evita o round-trip.
    pub fn atualizar_perguntas(&mut self, m: Msg) -> iced::Task<Msg> {
        match m {
            Msg::Responder(qid, op) => self.aplicar_quiz(&["reply", &qid, "--escolha", &op]),
            Msg::Outro(qid, t) => {
                self.outros.insert(qid, t);
                iced::Task::none()
            }
            Msg::ResponderTexto(qid) => {
                let texto = self.outros.get(&qid).cloned().unwrap_or_default();
                if texto.trim().is_empty() {
                    self.quiz = Aviso::falha("escreva a resposta antes de enviar");
                    return iced::Task::none();
                }
                self.aplicar_quiz(&["reply", &qid, "--texto", texto.trim()])
            }
            Msg::Marcar(qid, op) => {
                // Só as marcas: reler o CLI as perderia, que o disco não as tem.
                let v = self.marcadas.entry(qid).or_default();
                match v.iter().position(|x| *x == op) {
                    Some(i) => {
                        v.remove(i);
                    }
                    None => v.push(op),
                }
                iced::Task::none()
            }
            Msg::ConfirmarMultipla(qid) => {
                let escolhas = self.marcadas.remove(&qid).unwrap_or_default();
                if escolhas.is_empty() {
                    self.quiz = Aviso::falha("marque ao menos uma opção");
                    return iced::Task::none();
                }
                let mut args: Vec<&str> = vec!["reply", &qid];
                for e in &escolhas {
                    args.push("--escolha");
                    args.push(e);
                }
                self.aplicar_quiz(&args)
            }
            Msg::MarcarFeito(qid) => self.aplicar_quiz(&["reply", &qid, "--texto", "feito"]),
            // `refuse` CANCELA o item de máquina vinculado, em vez de fingir que foi feito.
            Msg::Recusar(qid) => self.aplicar_quiz(&["refuse", &qid, "recusado pela janela"]),
            _ => iced::Task::none(),
        }
    }

    /// **O quê:** o resultado de um `reply`/`refuse`: mostra a mensagem do CLI e relê tudo.
    ///
    /// **Onde:** `Msg::Resp(Origem::Quiz, …)`. **A mensagem do CLI é o produto:** ela diz POR QUE a
    /// resposta foi recusada. Os campos «outro» são limpos (a resposta já foi, ou foi recusada).
    pub fn resposta_do_quiz(&mut self, msg: String, erro: bool) -> iced::Task<Msg> {
        self.quiz = Aviso::de(super::estado::primeira(&msg), erro);
        self.outros.clear();
        self.recarregar(Escopo::TUDO)
    }

    /// **O quê:** a aba Perguntas. **Onde:** [`super::view`].
    pub fn ver_perguntas(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let mut col = column![].spacing(8).width(Fill);
        let (ab, rev) = (self.quiz_abertas(), self.quiz_revisar());
        let mut contagens = row![].spacing(14);
        if ab > 0 {
            contagens = contagens.push(negrito(format!("{ab} esperando você"), 12.5, c.aviso));
        }
        if rev > 0 {
            contagens =
                contagens.push(negrito(format!("{rev} para a máquina revisar"), 12.5, c.accent));
        }
        col = col.push(contagens);
        col = col.push(status(c, &self.quiz.texto, self.quiz.erro, 11.5));
        if self.cards.is_empty() && !self.quiz.erro {
            col = col.push(txt("Nenhuma pergunta na fila.", 12.5, c.dim));
        }
        for q in &self.cards {
            col = col.push(self.card(q));
        }
        rolagem(col)
    }

    /// **O quê:** o card de UMA pergunta. **Onde:** [`App::ver_perguntas`].
    ///
    /// **Três estados, três cores:** aberta = âmbar (precisa de você); aguardando revisão =
    /// laranja (a bola está com a máquina); revisada = neutra. Juntar os dois últimos esconderia
    /// exatamente o estado que a pessoa pediu para existir.
    fn card<'a>(&'a self, q: &'a Card) -> Element<'a, Msg> {
        let c = self.cores;
        let borda = if q.aberta {
            c.aviso
        } else if q.aguarda_revisao {
            c.accent
        } else {
            c.borda
        };
        let mut col = column![negrito(&q.titulo, 13.5, c.texto).width(Fill)].spacing(8);
        if !q.contexto.is_empty() {
            col = col.push(txt(&q.contexto, 11.5, c.dim).width(Fill));
        }
        if !q.destrava.is_empty() {
            col =
                col.push(negrito(format!("destrava: {}", q.destrava), 11.0, c.accent).width(Fill));
        }
        col = if q.aberta { self.controles_do_card(col, q) } else { resposta_do_card(c, col, q) };
        cartao(c, borda, 12.0, col)
    }

    /// **O quê:** os controles de uma pergunta ABERTA, pelo `kind`. **Onde:** [`App::card`].
    fn controles_do_card<'a>(
        &'a self,
        mut col: iced::widget::Column<'a, Msg>,
        q: &'a Card,
    ) -> iced::widget::Column<'a, Msg> {
        let c = self.cores;
        match q.kind.as_str() {
            "escolha" | "aprovar" | "sim_nao_outro" => {
                let mut ops = column![].spacing(6);
                for o in &q.opcoes {
                    ops = ops.push(opcao(c, q, o));
                }
                col = col.push(ops);
            }
            "multipla" => col = col.push(self.multipla(q)),
            _ => {}
        }
        // O "outro" do sim_nao_outro e o `livre` inteiro: o único lugar em que texto é legítimo
        // (o CLI recusa texto nos kinds de clique).
        if q.kind == "sim_nao_outro" || q.kind == "livre" {
            let dica = if q.kind == "livre" { "sua resposta" } else { "outra resposta" };
            let valor = self.outros.get(&q.id).map(String::as_str).unwrap_or("");
            let id = q.id.clone();
            let campo = text_input(dica, valor)
                .on_input(move |t| Msg::Outro(id.clone(), t))
                .on_submit(Msg::ResponderTexto(q.id.clone()))
                .size(13)
                .padding(7)
                .style(tema::campo(c));
            col = col.push(
                row![campo, btn(c, "Enviar", Some(Msg::ResponderTexto(q.id.clone())), false, true)]
                    .spacing(6),
            );
        }
        col.push(
            row![
                Space::new().width(Fill),
                btn(c, "Marcar feito", Some(Msg::MarcarFeito(q.id.clone())), false, true),
                btn(c, "Recusar", Some(Msg::Recusar(q.id.clone())), false, true),
            ]
            .spacing(8),
        )
    }

    /// **O quê:** o `multipla`: marca sem enviar, e confirma. **Onde:** [`App::controles_do_card`].
    /// Enviar a cada clique fecharia a pergunta no primeiro.
    fn multipla<'a>(&'a self, q: &'a Card) -> Element<'a, Msg> {
        let c = self.cores;
        let marcadas = self.marcadas.get(&q.id);
        let mut ops = column![].spacing(6);
        for o in &q.opcoes {
            let on = marcadas.is_some_and(|m| m.contains(&o.id));
            let caixa = button(txt(if on { "✓" } else { "" }, 13.0, iced::Color::WHITE).center())
                .width(20)
                .height(20)
                .padding(0)
                .on_press(Msg::Marcar(q.id.clone(), o.id.clone()))
                .style(tema::marca(c, on));
            let mut rotulo = column![negrito(&o.rotulo, 13.0, c.texto).width(Fill)].spacing(1);
            if !o.detalhe.is_empty() {
                rotulo = rotulo.push(txt(&o.detalhe, 11.0, c.dim).width(Fill));
            }
            ops = ops.push(row![caixa, rotulo].spacing(8));
        }
        let ok = btn(c, "Confirmar", Some(Msg::ConfirmarMultipla(q.id.clone())), true, true);
        column![ops, row![Space::new().width(Fill), ok]].spacing(8).into()
    }
}

/// **O quê:** o botão de uma opção, com o detalhe embaixo e o destaque de sugestão.
/// **Onde:** [`App::controles_do_card`].
fn opcao<'a>(c: Cores, q: &'a Card, o: &'a super::telas::Opc) -> Element<'a, Msg> {
    let mut titulo = row![negrito(&o.rotulo, 13.0, c.texto)].spacing(6);
    if o.sugerida {
        titulo = titulo.push(negrito("sugerida", 11.0, c.accent));
    }
    let mut corpo = column![titulo].spacing(2);
    if !o.detalhe.is_empty() {
        corpo = corpo.push(txt(&o.detalhe, 11.5, c.dim).width(Fill));
    }
    button(container(corpo).width(Fill))
        .width(Fill)
        .padding(8)
        .on_press(Msg::Responder(q.id.clone(), o.id.clone()))
        .style(tema::opcao(c, o.sugerida))
        .into()
}

/// **O quê:** o que se mostra de uma pergunta JÁ respondida. **Onde:** [`App::card`].
fn resposta_do_card<'a>(
    c: Cores,
    mut col: iced::widget::Column<'a, Msg>,
    q: &'a Card,
) -> iced::widget::Column<'a, Msg> {
    col = col.push(negrito(format!("respondeu: {}", q.resposta_texto), 12.5, c.texto).width(Fill));
    if !q.respondida_em.is_empty() {
        col = col.push(txt(&q.respondida_em, 11.0, c.dim));
    }
    if q.aguarda_revisao {
        col = col.push(negrito("aguardando revisão da máquina", 11.5, c.accent));
    }
    if !q.revisao.is_empty() {
        col = col.push(txt(format!("revisão: {}", q.revisao), 11.5, c.dim).width(Fill));
    }
    col
}
