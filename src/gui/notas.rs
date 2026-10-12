//! NOTAS e ARQUIVOS do run: abrir PLAN.md/CHECKLIST.md/NOTAS.md no editor e anexar notas.
//!
//! **O quê:** lista os documentos (`arquivos --json`), mostra as notas (`notas --json`), abre um
//! documento no editor do usuário e anexa um ponto por task ou um prompt de correção
//! (`note --kind … -- texto`).
//!
//! **Onde:** a aba Notas. O editor acoplado do hub gravava o PLAN/CHECKLIST de dentro da janela;
//! aqui a janela NÃO escreve arquivo (D6, e há teste que lê o fonte para cobrar): o arquivo
//! abre no editor da pessoa, e a nota entra pelo CLI.

use super::estado::{App, Aviso};
use super::leituras::Arquivo;
use super::msg::{Msg, Origem};
use super::tema;
use super::widgets::{btn, cartao, negrito, rolagem, status, txt};
use super::{carga::Escopo, comandos, tarefa};
use iced::widget::{column, row, text_input};
use iced::{Alignment, Element, Fill};

/// O estado da aba Notas.
#[derive(Default)]
pub struct EstadoNotas {
    pub arquivos: Vec<Arquivo>,
    /// As notas já anexadas, como o CLI as imprime.
    pub texto: String,
    /// O campo «ponto por task».
    pub task: String,
    /// O campo «prompt de correção».
    pub correcao: String,
    pub status: Aviso,
}

impl App {
    /// **O quê:** trata as mensagens da aba Notas. **Onde:** [`App::update`].
    pub fn atualizar_notas(&mut self, m: Msg) -> iced::Task<Msg> {
        match m {
            Msg::AbrirArquivo(caminho) => tarefa::abrir_arquivo(caminho),
            Msg::NotaTask(t) => {
                self.notas.task = t;
                iced::Task::none()
            }
            Msg::NotaCorrecao(t) => {
                self.notas.correcao = t;
                iced::Task::none()
            }
            Msg::NotaAdicionar(correcao) => {
                let texto = if correcao { &self.notas.correcao } else { &self.notas.task };
                if texto.trim().is_empty() {
                    self.notas.status = Aviso::falha("escreva a nota antes de anexar.");
                    return iced::Task::none();
                }
                let args = comandos::nota(correcao, texto);
                tarefa::cli(self.dir.clone(), args, Origem::Nota(correcao))
            }
            _ => iced::Task::none(),
        }
    }

    /// **O quê:** o resultado de «Anexar» ou «Abrir no editor». **Onde:** `Resp(Nota|Arquivo, …)`.
    ///
    /// Só limpa o campo se a nota entrou: falhou, o texto digitado fica.
    pub fn resposta_das_notas(&mut self, o: Origem, msg: String, erro: bool) -> iced::Task<Msg> {
        self.notas.status = Aviso::de(msg, erro);
        match o {
            Origem::Nota(correcao) if !erro => {
                if correcao {
                    self.notas.correcao.clear();
                } else {
                    self.notas.task.clear();
                }
                self.recarregar(Escopo::DE_NOTAS)
            }
            Origem::Nota(_) => self.recarregar(Escopo::DE_NOTAS),
            _ => iced::Task::none(),
        }
    }

    /// **O quê:** a aba Notas. **Onde:** [`super::view`].
    pub fn ver_notas(&self) -> Element<'_, Msg> {
        let c = self.cores;
        let x = &self.notas;
        let mut col = column![negrito("Arquivos do run", 13.0, c.texto)].spacing(10).width(Fill);
        for a in &x.arquivos {
            let onde = if a.existe { a.caminho.as_str() } else { "ainda não existe" };
            let linha = row![
                negrito(&a.nome, 12.5, c.texto).width(110),
                txt(onde, 11.0, c.dim).width(Fill),
                btn(
                    c,
                    "Abrir no editor",
                    a.existe.then(|| Msg::AbrirArquivo(a.caminho.clone())),
                    false,
                    true
                ),
            ]
            .spacing(12)
            .align_y(Alignment::Center);
            col = col.push(cartao(c, c.borda, 8.0, linha));
        }
        col = col
            .push(negrito("Ponto por task", 13.0, c.texto))
            .push(self.campo_de_nota("um ponto que o agente deve tratar numa task", &x.task, false))
            .push(negrito("Prompt de correção", 13.0, c.texto))
            .push(self.campo_de_nota(
                "o que o agente fez errado e como corrigir",
                &x.correcao,
                true,
            ))
            .push(status(c, &x.status.texto, x.status.erro, 12.0))
            .push(negrito("Notas já anexadas", 13.0, c.texto));
        if x.texto.is_empty() {
            col = col.push(txt("nenhuma nota ainda.", 12.0, c.dim));
        } else {
            col = col.push(txt(&x.texto, 12.0, c.texto).width(Fill));
        }
        rolagem(col)
    }

    /// **O quê:** a linha «campo + Anexar» de uma nota. **Onde:** [`App::ver_notas`].
    fn campo_de_nota<'a>(
        &'a self,
        dica: &'a str,
        valor: &'a str,
        correcao: bool,
    ) -> Element<'a, Msg> {
        let c = self.cores;
        let mudou = move |t: String| if correcao { Msg::NotaCorrecao(t) } else { Msg::NotaTask(t) };
        let campo = text_input(dica, valor)
            .on_input(mudou)
            .on_submit(Msg::NotaAdicionar(correcao))
            .size(13)
            .padding(7)
            .style(tema::campo(c));
        row![
            campo,
            btn(
                c,
                "Anexar",
                (!valor.trim().is_empty()).then_some(Msg::NotaAdicionar(correcao)),
                false,
                true
            )
        ]
        .spacing(6)
        .into()
    }
}
