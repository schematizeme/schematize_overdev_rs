//! O SINO de notificações: o mesmo painel do hub (Globais, Pessoais, Histórico), com a mesma fonte.
//!
//! **O quê:** lê `schematize notifications --historico` (o cache do hub, via o binário `schematize`),
//! conta as não lidas para o selo, mostra o painel em modal, marca como lidas ao abrir
//! (`--lidas`), conclui uma (`--concluir <id>`) e age por tipo (notícia → navegador; versão nova do
//! app → `schematize upgrade` num terminal, onde o comando aparece antes de rodar).
//!
//! **Onde:** o cabeçalho ([`super::view`]) e o `update` ([`super::update`]).
//!
//! ## GAP cross-repo: o comando ainda não tem `--json`
//!
//! O hub lê o cache pela LIB (`schematize::notificacoes`), que a janela não pode importar (seria
//! embutir uma versão). O binário só imprime TEXTO, então [`ler_notificacoes`] lê esse texto, e
//! o formato é amarrado por teste: se mudar, o parser devolve `Err` (e a tela mostra), nunca uma
//! lista vazia calada. Pedido registrado: `schematize notifications --json` em `schematize_cli_rs`.
//!
//! ## Nada aqui grava arquivo
//!
//! Marcar lida e concluir são subcomandos do `schematize` (D6): quem escreve o cache é o CLI.

use super::estado::{App, Aviso};
use super::msg::{Msg, Origem, SinoMsg};
use super::widgets::{btn, modal, negrito, txt};
use super::{tarefa, tema};
use iced::widget::{column, container, row, scrollable, Space};
use iced::{Element, Fill, Length};

/// O estado de uma notificação no cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Estado {
    Nova,
    Lida,
    Concluida,
}

/// Uma notificação. `escopo` é `global` ou `personal`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notif {
    pub id: String,
    pub estado: Estado,
    pub kind: String,
    pub titulo: String,
    pub corpo: String,
    pub acao: String,
    pub escopo: String,
}

/// O estado do sino na janela.
#[derive(Debug, Default)]
pub struct Sino {
    pub aberto: bool,
    pub itens: Vec<Notif>,
    /// Falha de leitura (aparece no painel; o selo some em vez de dizer «0»).
    pub erro: String,
    pub status: Aviso,
    /// Já pedimos a sincronização de rede da abertura?
    pub sincronizou: bool,
}

impl Sino {
    /// **O quê:** quantas notificações NÃO foram lidas. **Onde:** o selo do sino.
    pub fn nao_lidas(&self) -> usize {
        self.itens.iter().filter(|n| n.estado == Estado::Nova).count()
    }
}

/// **O quê:** tira as sequências ANSI de cor (`ESC [ … m`). **Onde:** [`ler_notificacoes`] —
/// o filho roda com `NO_COLOR`, mas uma versão que ignore a variável não pode quebrar o parser.
fn sem_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\u{1b}' && it.peek() == Some(&'[') {
            for d in it.by_ref() {
                if d.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// **O quê:** o texto de `schematize notifications --historico` → as notificações. **Onde:**
/// [`tarefa::sino`]. PURA.
///
/// O formato (de `cli/conta.rs`): `Globais (N):` / `Pessoais (N):`, e por item `  ● [kind] título`
/// (`●` nova, `○` lida, `✓` concluída), `      corpo`, `      → ação`, `      id <id>`. As
/// mensagens de cache vazio são `Ok(vec![])`; texto que não é nenhum dos dois é `Err`.
pub fn ler_notificacoes(texto: &str) -> Result<Vec<Notif>, String> {
    let limpo = sem_ansi(texto);
    let mut itens: Vec<Notif> = Vec::new();
    let mut escopo = String::new();
    let mut viu_secao = false;
    for linha in limpo.lines() {
        let t = linha.trim();
        if t.is_empty() {
            continue;
        }
        if !linha.starts_with(' ') {
            match t.split(" (").next().unwrap_or("") {
                "Globais" => (escopo, viu_secao) = ("global".into(), true),
                "Pessoais" => (escopo, viu_secao) = ("personal".into(), true),
                _ => {} // rodapé: «N não lida(s)…», «concluir uma: …»
            }
            continue;
        }
        if let Some(n) = abre_item(t, &escopo) {
            itens.push(n);
        } else if let Some(atual) = itens.last_mut() {
            if let Some(a) = t.strip_prefix("→ ") {
                atual.acao = a.trim().to_string();
            } else if let Some(id) = t.strip_prefix("id ") {
                atual.id = id.trim().to_string();
            } else if atual.corpo.is_empty() {
                atual.corpo = t.to_string();
            }
        }
    }
    let vazio_conhecido = limpo.trim_start().starts_with("sem notificações")
        || limpo.trim_start().starts_with("nada pendente");
    if !viu_secao && !vazio_conhecido && !limpo.trim().is_empty() {
        return Err(format!(
            "não entendi a lista de notificações (formato novo?): {}",
            limpo.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim()
        ));
    }
    if let Some(sem_id) = itens.iter().find(|n| n.id.is_empty()) {
        return Err(format!("notificação sem id no texto do CLI: {}", sem_id.titulo));
    }
    Ok(itens)
}

/// A primeira linha de um item (`● [kind] título`), ou `None` se a linha é outra coisa.
fn abre_item(t: &str, escopo: &str) -> Option<Notif> {
    let mut cs = t.chars();
    let estado = match cs.next()? {
        '●' => Estado::Nova,
        '○' => Estado::Lida,
        '✓' => Estado::Concluida,
        _ => return None,
    };
    let resto = cs.as_str().trim_start().strip_prefix('[')?;
    let (kind, titulo) = resto.split_once(']')?;
    Some(Notif {
        id: String::new(),
        estado,
        kind: kind.to_string(),
        titulo: titulo.trim().to_string(),
        corpo: String::new(),
        acao: String::new(),
        escopo: escopo.to_string(),
    })
}

/// **O quê:** só `https://` sem espaço nem controle. **Onde:** a ação de notícia — o texto vem da
/// rede, e o que vai ao `xdg-open` não pode ser outra coisa.
pub fn url_segura(u: &str) -> bool {
    u.starts_with("https://") && u.len() > 8 && !u.chars().any(|c| c.is_control() || c == ' ')
}

impl App {
    /// **O quê:** trata as mensagens do sino. **Onde:** [`App::update`].
    pub fn atualizar_sino(&mut self, m: SinoMsg) -> iced::Task<Msg> {
        match m {
            SinoMsg::Alternar => {
                self.sino.aberto = !self.sino.aberto;
                self.sino.status = Aviso::default();
                if self.sino.aberto {
                    // Viu, então leu: o CLI marca (nada é apagado) e a lista volta relida.
                    return tarefa::sino(self.dir.clone(), false, true);
                }
                iced::Task::none()
            }
            SinoMsg::Carregado(r) => {
                match r {
                    Ok(itens) => {
                        self.sino.itens = itens;
                        self.sino.erro.clear();
                    }
                    Err(e) => {
                        self.sino.itens.clear();
                        self.sino.erro = e;
                    }
                }
                if self.sino.sincronizou {
                    return iced::Task::none();
                }
                self.sino.sincronizou = true;
                tarefa::sino(self.dir.clone(), true, false)
            }
            SinoMsg::Tick => iced::Task::batch([
                tarefa::sino(self.dir.clone(), true, false),
                tarefa::espera_sino(),
            ]),
            SinoMsg::Concluir(id) => {
                let args = vec!["notifications".to_string(), "--concluir".to_string(), id];
                tarefa::schematize(self.dir.clone(), args, Origem::Sino)
            }
            SinoMsg::Acao(kind, acao) => self.agir_no_sino(&kind, &acao),
        }
    }

    /// **O quê:** a ação do item por TIPO (nunca pelo rótulo livre do servidor). **Onde:**
    /// [`App::atualizar_sino`]. Tipo desconhecido não faz nada além de concluir (inerte de propósito).
    fn agir_no_sino(&mut self, kind: &str, acao: &str) -> iced::Task<Msg> {
        match kind {
            "news" if url_segura(acao) => {
                tarefa::plataforma(tarefa::Plataforma::Url(acao.to_string()), Origem::Sino)
            }
            "news" => {
                self.sino.status = Aviso::falha("o link da notícia não é um https seguro.");
                iced::Task::none()
            }
            "app_update" => {
                let bin = super::externos::bin_irmao("schematize").display().to_string();
                tarefa::terminal_de(
                    bin,
                    self.dir.clone(),
                    vec!["upgrade".to_string()],
                    Origem::Sino,
                )
            }
            _ => iced::Task::none(),
        }
    }

    /// **O quê:** o resultado de `--concluir`: aviso no painel e relê a lista. **Onde:**
    /// [`App::resposta`].
    pub fn resposta_do_sino(&mut self, msg: String, erro: bool) -> iced::Task<Msg> {
        self.sino.status = Aviso::de(msg, erro);
        tarefa::sino(self.dir.clone(), false, false)
    }

    /// **O quê:** o painel do sino (modal), ou `None` se fechado. **Onde:** [`App::view`].
    pub fn ver_sino(&self) -> Option<Element<'_, Msg>> {
        if !self.sino.aberto {
            return None;
        }
        let c = self.cores;
        let mut lista = column![].spacing(8).width(Fill);
        if !self.sino.erro.is_empty() {
            lista = lista.push(txt(&self.sino.erro, 12.5, c.aviso).width(Fill));
        }
        let pend = |n: &&Notif| n.estado != Estado::Concluida;
        let mut tem = false;
        for (titulo, escopo) in [("Globais", "global"), ("Pessoais", "personal")] {
            let ns: Vec<&Notif> =
                self.sino.itens.iter().filter(pend).filter(|n| n.escopo == escopo).collect();
            if ns.is_empty() {
                continue;
            }
            tem = true;
            lista = lista.push(negrito(format!("{titulo} ({})", ns.len()), 12.0, c.dim));
            for n in ns {
                lista = lista.push(self.cartao_notif(n));
            }
        }
        let feitas: Vec<&Notif> =
            self.sino.itens.iter().filter(|n| n.estado == Estado::Concluida).collect();
        if !feitas.is_empty() {
            lista = lista.push(negrito(format!("Histórico ({})", feitas.len()), 12.0, c.dim));
            for n in feitas {
                lista = lista.push(txt(format!("✓ {}", n.titulo), 12.0, c.dim).width(Fill));
            }
        }
        if !tem && self.sino.erro.is_empty() {
            lista = lista.push(txt("Sem notificações", 13.0, c.dim));
        }
        let corpo = column![
            row![
                negrito("Notificações", 16.0, c.texto),
                Space::new().width(Fill),
                btn(c, "Fechar", Some(Msg::Sino(SinoMsg::Alternar)), false, true),
            ],
            scrollable(lista.padding([0, 8])).height(Length::Fixed(380.0)),
            super::widgets::status(c, &self.sino.status.texto, self.sino.status.erro, 12.0),
        ]
        .spacing(10);
        Some(modal(c, corpo.into(), Msg::Sino(SinoMsg::Alternar), false))
    }

    /// Um item pendente: título, corpo e os botões (ação por tipo + concluir).
    fn cartao_notif<'a>(&'a self, n: &'a Notif) -> Element<'a, Msg> {
        let c = self.cores;
        let marca = if n.estado == Estado::Nova { "●" } else { "○" };
        let mut botoes = row![Space::new().width(Fill)].spacing(6);
        let rotulo = match n.kind.as_str() {
            "news" => Some("Abrir"),
            "app_update" => Some("Atualizar"),
            _ => None,
        };
        if let Some(r) = rotulo {
            let m = Msg::Sino(SinoMsg::Acao(n.kind.clone(), n.acao.clone()));
            botoes = botoes.push(btn(c, r, Some(m), false, true));
        }
        botoes = botoes.push(btn(
            c,
            "✓ concluir",
            Some(Msg::Sino(SinoMsg::Concluir(n.id.clone()))),
            false,
            true,
        ));
        let mut col =
            column![negrito(format!("{marca} {}", n.titulo), 13.0, c.texto).width(Fill)].spacing(4);
        if !n.corpo.is_empty() {
            col = col.push(txt(&n.corpo, 12.0, c.dim).width(Fill));
        }
        container(col.push(botoes))
            .padding(10)
            .width(Fill)
            .style(tema::cartao(c, c.borda, 10.0))
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::super::msg::Aba;
    use super::*;
    use std::path::PathBuf;

    const TXT: &str = "\nGlobais (2):\n  ● [app_update] Nova versão v0.66.0\n      O schematize v0.66.0 está disponível.\n      → upgrade\n      id aaa\n  ✓ [news] Post velho\n      → https://blog.schematize.net/x\n      id bbb\n\nPessoais (1):\n  ○ [skill_outdated] Skill qa desatualizada\n      → skills update qa\n      id ccc\n\n1 não lida(s). `x`\nconcluir uma:  schematize notifications --concluir <id>\n";

    fn app() -> App {
        App::novo(PathBuf::from("/p/a"), PathBuf::from("/n"), "v".into(), Aba::Perguntas, true)
    }

    #[test]
    fn le_o_formato_do_cli() {
        let n = ler_notificacoes(TXT).unwrap();
        assert_eq!(n.len(), 3);
        assert_eq!(
            (n[0].estado, n[0].kind.as_str(), n[0].escopo.as_str()),
            (Estado::Nova, "app_update", "global")
        );
        assert_eq!((n[0].acao.as_str(), n[0].id.as_str()), ("upgrade", "aaa"));
        assert!(n[0].corpo.starts_with("O schematize"));
        assert_eq!(n[1].estado, Estado::Concluida);
        assert_eq!((n[2].escopo.as_str(), n[2].estado), ("personal", Estado::Lida));
        let s = Sino { itens: n, ..Sino::default() };
        assert_eq!(s.nao_lidas(), 1);
    }

    #[test]
    fn ansi_nao_quebra_e_cache_vazio_e_ok() {
        let colorido =
            TXT.replace("●", "\x1b[33m●\x1b[0m").replace("Globais", "\x1b[1mGlobais\x1b[0m");
        assert_eq!(ler_notificacoes(&colorido).unwrap().len(), 3);
        for vazio in
            ["sem notificações no cache — rode `x`.\n", "nada pendente (3 no histórico).", ""]
        {
            assert_eq!(ler_notificacoes(vazio).unwrap(), vec![], "{vazio:?}");
        }
    }

    #[test]
    fn formato_desconhecido_e_erro_e_nao_lista_vazia() {
        assert!(ler_notificacoes("error: unexpected argument").is_err());
        assert!(ler_notificacoes("Globais (1):\n  ● [k] sem id\n").is_err(), "item sem id");
        let fundo = "\u{0}".repeat(100);
        assert!(ler_notificacoes(&fundo).is_err());
    }

    #[test]
    fn so_https_sem_espaco_vai_ao_navegador() {
        assert!(url_segura("https://blog.schematize.net/p"));
        for ruim in [
            "http://x",
            "file:///etc/passwd",
            "https://",
            "https://a b",
            "https://a\nb",
            "javascript:x",
            "",
        ] {
            assert!(!url_segura(ruim), "{ruim:?}");
        }
    }

    #[test]
    fn abrir_marca_lidas_fechar_nao_faz_efeito() {
        let mut a = app();
        assert_eq!(a.update(Msg::Sino(SinoMsg::Alternar)).units(), 1, "abre: --lidas + relê");
        assert!(a.sino.aberto);
        assert_eq!(a.update(Msg::Sino(SinoMsg::Alternar)).units(), 0);
        assert!(!a.sino.aberto);
    }

    #[test]
    fn carregado_guarda_ou_mostra_o_erro_e_sincroniza_uma_vez() {
        let mut a = app();
        let n = ler_notificacoes(TXT).unwrap();
        assert_eq!(a.update(Msg::Sino(SinoMsg::Carregado(Ok(n)))).units(), 1, "1ª: pede o sync");
        assert_eq!(a.sino.itens.len(), 3);
        assert_eq!(a.update(Msg::Sino(SinoMsg::Carregado(Err("boom".into())))).units(), 0);
        assert!(a.sino.itens.is_empty() && a.sino.erro == "boom", "erro aparece, lista não finge");
    }

    #[test]
    fn concluir_vai_ao_cli_e_a_resposta_rele() {
        let mut a = app();
        assert_eq!(a.update(Msg::Sino(SinoMsg::Concluir("aaa".into()))).units(), 1);
        assert_eq!(a.update(Msg::Sino(SinoMsg::Tick)).units(), 2, "sincroniza e agenda o próximo");
        let t = a.update(Msg::Resp(Origem::Sino, "falhou".into(), true));
        assert!(a.sino.status.erro && a.sino.status.texto == "falhou");
        assert_eq!(t.units(), 1);
    }

    #[test]
    fn noticia_com_link_ruim_e_recusada_e_tipo_desconhecido_e_inerte() {
        let mut a = app();
        let _ = a.update(Msg::Sino(SinoMsg::Acao("news".into(), "file:///x".into())));
        assert!(a.sino.status.erro);
        a.sino.status = Aviso::default();
        assert_eq!(
            a.update(Msg::Sino(SinoMsg::Acao("rotulo-livre".into(), "rm -rf".into()))).units(),
            0
        );
        assert!(a.sino.status.texto.is_empty());
    }
}
