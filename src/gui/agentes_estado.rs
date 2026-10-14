//! O ESTADO PURO da aba Agentes: a config em edição, a validação e as operações sobre ela.
//!
//! **O quê:** [`EstadoAgentes`] guarda a cópia em edição ([`Agentes`]) ao lado da original (para o
//! indicador de «alterado»), e [`EstadoAgentes::aplicar`] trata cada [`AgMsg`] SEM tocar disco,
//! processo ou janela — por isso é testável sem abrir nada. A gravação recebe a função que grava
//! ([`EstadoAgentes::salvar_com`]); o teste injeta uma falsa.
//!
//! **Onde:** [`super::agentes_tela`] (a view e o glue com o disco/processos) e [`super::update`].

use super::estado::Aviso;
use crate::agentes::{Agentes, Disponibilidade, ErroAgentes, Motor, Papel};

/// Qual arquivo a aba edita. **Onde:** o seletor «Global / Este projeto».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EscopoAg {
    #[default]
    Global,
    Projeto,
}

/// O que pode acontecer na aba Agentes. **Onde:** [`super::msg::Msg::Agentes`].
#[derive(Debug, Clone)]
pub enum AgMsg {
    Escopo(EscopoAg),
    /// `(papel, nome do motor)`.
    Motor(String, String),
    Modelo(String, String),
    Ativo(String, bool),
    Descricao(String, String),
    NovoNome(String),
    AdicionarPapel,
    RemoverPapel(String),
    Subir(usize),
    Descer(usize),
    RemoverDaEscada(usize),
    /// O papel escolhido no seletor «adicionar à escada».
    EscolherParaEscada(String),
    AdicionarNaEscada,
    /// `(tipo de tarefa, papel)`.
    Rotear(String, String),
    Salvar,
    Padrao,
    Descartar,
    VerificarMotores,
    /// O resultado (assíncrono) da checagem de um motor.
    Motores(Motor, Disponibilidade),
}

/// Sugestões de modelo por motor. **Onde:** o seletor de modelo da tabela de papéis.
pub fn sugestoes(m: Motor) -> Vec<String> {
    let lista: &[&str] = match m {
        Motor::Claude => &["opus", "sonnet", "haiku"],
        Motor::Codex => &[
            "gpt-5.6-terra",
            "gpt-5.6-sol",
            "gpt-5.6-luna",
            "gpt-6-sol",
            "gpt-6-astra",
            "gpt-6-luna",
        ],
        Motor::Cursor => &["auto"],
    };
    lista.iter().map(|s| s.to_string()).collect()
}

/// **O quê:** o nome de papel é válido (`[a-z0-9-]{1,32}`)? **Onde:** «adicionar papel».
pub fn nome_valido(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 32
        && n.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// **O quê:** a ordem de exibição dos papéis: `principal`, depois a escada (na ordem dela), depois
/// os alvos do roteamento (visual, imagem, …) e por fim o resto em ordem alfabética; sem repetir
/// e só com papéis que existem. **Onde:** a tabela de papéis da aba.
pub fn ordem_dos_papeis(a: &Agentes) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    let candidatos = std::iter::once(&"principal".to_string())
        .chain(a.escada.iter())
        .cloned()
        .chain(["visual", "imagem"].iter().filter_map(|t| a.roteamento.get(*t).cloned()))
        .chain(a.roteamento.values().cloned())
        .chain(a.papeis.keys().cloned())
        .collect::<Vec<_>>();
    for n in candidatos {
        if a.papeis.contains_key(&n) && !v.contains(&n) {
            v.push(n);
        }
    }
    v
}

/// O estado da aba. `Default` = ainda não carregado.
#[derive(Debug, Clone)]
pub struct EstadoAgentes {
    pub escopo: EscopoAg,
    /// A cópia que a pessoa edita.
    pub editado: Agentes,
    /// A cópia como foi carregada (ou salva por último): a base do «alterado».
    pub original: Agentes,
    pub carregado: bool,
    /// O erro tipado da última tentativa de salvar/validar, inline em vermelho.
    pub erro: String,
    /// A faixa de sucesso/informação.
    pub aviso: Aviso,
    pub novo_nome: String,
    pub para_escada: Option<String>,
    /// O estado de cada motor; `None` = checando.
    pub motores: Vec<(Motor, Option<Disponibilidade>)>,
}

impl Default for EstadoAgentes {
    /// **O quê:** o estado ainda não carregado (editor com o padrão embutido). **Onde:** [`App::novo`].
    fn default() -> Self {
        EstadoAgentes {
            escopo: EscopoAg::Global,
            editado: Agentes::padrao(),
            original: Agentes::padrao(),
            carregado: false,
            erro: String::new(),
            aviso: Aviso::default(),
            novo_nome: String::new(),
            para_escada: None,
            motores: Vec::new(),
        }
    }
}

impl EstadoAgentes {
    /// **O quê:** o estado com `a` carregado (original = editado). **Onde:** a carga do disco.
    pub fn com(escopo: EscopoAg, a: Agentes) -> EstadoAgentes {
        EstadoAgentes {
            escopo,
            original: a.clone(),
            editado: a,
            carregado: true,
            motores: Motor::TODOS.iter().map(|m| (*m, None)).collect(),
            ..EstadoAgentes::default()
        }
    }

    /// **O quê:** há alteração não salva? **Onde:** o indicador e os botões Salvar/Descartar.
    pub fn alterado(&self) -> bool {
        self.editado != self.original
    }

    /// **O quê:** os nomes dos papéis, em ordem. **Onde:** os seletores de escada e roteamento.
    pub fn nomes(&self) -> Vec<String> {
        self.editado.papeis.keys().cloned().collect()
    }

    /// **O quê:** trata a mensagem de EDIÇÃO (tudo que não é disco/processo). **Onde:** o `update`
    /// da aba; `Salvar`, `Escopo` e `VerificarMotores` ficam com o glue ([`super::agentes_tela`]).
    pub fn aplicar(&mut self, m: AgMsg) {
        self.aviso = Aviso::default();
        if !matches!(m, AgMsg::Motores(..) | AgMsg::NovoNome(_)) {
            self.erro.clear();
        }
        match m {
            AgMsg::Motor(p, nome) => self.mudar_motor(&p, &nome),
            AgMsg::Modelo(p, v) => self.com_papel(&p, |x| x.modelo = v),
            AgMsg::Ativo(p, v) => self.com_papel(&p, |x| x.ativo = v),
            AgMsg::Descricao(p, v) => self.com_papel(&p, |x| x.descricao = v),
            AgMsg::NovoNome(n) => self.novo_nome = n,
            AgMsg::AdicionarPapel => self.adicionar_papel(),
            AgMsg::RemoverPapel(p) => self.remover_papel(&p),
            AgMsg::Subir(i) => self.mover(i, true),
            AgMsg::Descer(i) => self.mover(i, false),
            AgMsg::RemoverDaEscada(i) => {
                if i < self.editado.escada.len() {
                    self.editado.escada.remove(i);
                }
            }
            AgMsg::EscolherParaEscada(p) => self.para_escada = Some(p),
            AgMsg::AdicionarNaEscada => {
                if let Some(p) = self.para_escada.take() {
                    self.editado.escada.push(p);
                }
            }
            AgMsg::Rotear(tipo, p) => {
                self.editado.roteamento.insert(tipo, p);
            }
            AgMsg::Padrao => {
                self.editado = Agentes::padrao();
                self.aviso = Aviso::ok("padrão carregado no editor — ainda não foi salvo.");
            }
            AgMsg::Descartar => {
                self.editado = self.original.clone();
                self.aviso = Aviso::ok("alterações descartadas.");
            }
            AgMsg::Motores(m, d) => self.registrar_motor(m, d),
            AgMsg::Escopo(_) | AgMsg::Salvar | AgMsg::VerificarMotores => {}
        }
    }

    /// **O quê:** aplica `f` ao papel `p`, se existir. **Onde:** os campos da tabela.
    fn com_papel(&mut self, p: &str, f: impl FnOnce(&mut Papel)) {
        if let Some(x) = self.editado.papeis.get_mut(p) {
            f(x);
        }
    }

    /// **O quê:** troca o motor; se o modelo atual não é sugestão do novo, usa a 1ª sugestão.
    /// **Onde:** o seletor de motor (um modelo `opus` num motor `cursor` é erro certo).
    fn mudar_motor(&mut self, p: &str, nome: &str) {
        let Some(novo) = Motor::tentar(nome) else {
            self.erro = format!("motor desconhecido: `{nome}`");
            return;
        };
        self.com_papel(p, |x| {
            if x.motor != novo {
                x.motor = novo;
                let s = sugestoes(novo);
                if !s.contains(&x.modelo) {
                    x.modelo = s.into_iter().next().unwrap_or_default();
                }
            }
        });
    }

    /// **O quê:** cria um papel com o nome digitado (validado). **Onde:** «Adicionar papel».
    fn adicionar_papel(&mut self) {
        let n = self.novo_nome.trim().to_string();
        if !nome_valido(&n) {
            self.erro = format!(
                "nome de papel inválido: `{}` (use a-z, 0-9 e `-`, de 1 a 32 caracteres)",
                n.escape_debug()
            );
            return;
        }
        if self.editado.papeis.contains_key(&n) {
            self.erro = format!("o papel `{n}` já existe");
            return;
        }
        let papel = Papel {
            motor: Motor::Claude,
            modelo: "sonnet".into(),
            ativo: true,
            descricao: String::new(),
        };
        self.editado.papeis.insert(n, papel);
        self.novo_nome.clear();
        self.erro.clear();
    }

    /// **O quê:** remove um papel (nunca o `principal`) e limpa as referências a ele: sai da escada
    /// e o roteamento que apontava para ele passa ao `executor` (ou ao `principal`).
    /// **Onde:** o botão ✕ da linha do papel.
    fn remover_papel(&mut self, p: &str) {
        if p == "principal" {
            self.erro = "o papel `principal` é obrigatório e não pode ser removido".into();
            return;
        }
        if self.editado.papeis.remove(p).is_none() {
            return;
        }
        self.editado.escada.retain(|r| r != p);
        let troca =
            if self.editado.papeis.contains_key("executor") { "executor" } else { "principal" };
        for r in self.editado.roteamento.values_mut().filter(|r| r.as_str() == p) {
            *r = troca.to_string();
        }
        self.erro.clear();
    }

    /// **O quê:** sobe (`true`) ou desce o item `i` da escada; nas pontas não faz nada.
    /// **Onde:** os botões ↑/↓.
    fn mover(&mut self, i: usize, subir: bool) {
        let e = &mut self.editado.escada;
        let j = if subir { i.checked_sub(1) } else { i.checked_add(1) };
        if let Some(j) = j.filter(|j| *j < e.len() && i < e.len()) {
            e.swap(i, j);
        }
    }

    /// **O quê:** guarda o resultado da checagem de um motor. **Onde:** `AgMsg::Motores`.
    fn registrar_motor(&mut self, m: Motor, d: Disponibilidade) {
        match self.motores.iter_mut().find(|(x, _)| *x == m) {
            Some(slot) => slot.1 = Some(d),
            None => self.motores.push((m, Some(d))),
        }
    }

    /// **O quê:** valida e grava com `gravar`. Em erro o texto TIPADO vai a `erro` (nada some, nada
    /// panica); em sucesso o «original» passa a ser o gravado. **Onde:** «Salvar».
    pub fn salvar_com(&mut self, gravar: impl FnOnce(&Agentes) -> Result<(), ErroAgentes>) {
        self.aviso = Aviso::default();
        let r = self.editado.validar().and_then(|()| gravar(&self.editado));
        match r {
            Ok(()) => {
                self.original = self.editado.clone();
                self.erro.clear();
                self.aviso = Aviso::ok("salvo.");
            }
            Err(e) => self.erro = e.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn est() -> EstadoAgentes {
        EstadoAgentes::com(EscopoAg::Global, Agentes::padrao())
    }

    /// Subir/descer troca vizinhos e as pontas são no-op.
    #[test]
    fn reordena_a_escada() {
        let mut e = est();
        let antes = e.editado.escada.clone();
        e.aplicar(AgMsg::Subir(1));
        assert_eq!(e.editado.escada[0], antes[1]);
        assert_eq!(e.editado.escada[1], antes[0]);
        e.aplicar(AgMsg::Subir(0));
        e.aplicar(AgMsg::Descer(antes.len() - 1));
        e.aplicar(AgMsg::Descer(99));
        assert_eq!(e.editado.escada[0], antes[1]);
        assert_eq!(e.editado.escada.len(), antes.len());
        e.aplicar(AgMsg::Descer(0));
        assert_eq!(e.editado.escada, antes);
    }

    /// Adicionar/remover da escada e o seletor «adicionar».
    #[test]
    fn edita_a_escada() {
        let mut e = est();
        let n = e.editado.escada.len();
        e.aplicar(AgMsg::RemoverDaEscada(0));
        assert_eq!(e.editado.escada.len(), n - 1);
        e.aplicar(AgMsg::AdicionarNaEscada); // sem escolha: nada
        assert_eq!(e.editado.escada.len(), n - 1);
        e.aplicar(AgMsg::EscolherParaEscada("visual".into()));
        e.aplicar(AgMsg::AdicionarNaEscada);
        assert_eq!(e.editado.escada.last().map(String::as_str), Some("visual"));
        assert!(e.alterado());
    }

    /// Papel novo entra; nome inválido e duplicado viram erro inline e nada entra.
    #[test]
    fn adiciona_papel_validando_o_nome() {
        let mut e = est();
        for ruim in ["", "Maiusc", "com espaço", "a_b", &"x".repeat(33)] {
            e.aplicar(AgMsg::NovoNome(ruim.into()));
            e.aplicar(AgMsg::AdicionarPapel);
            assert!(e.erro.contains("inválido"), "{ruim:?}: {}", e.erro);
            assert!(!e.alterado());
        }
        e.aplicar(AgMsg::NovoNome("executor".into()));
        e.aplicar(AgMsg::AdicionarPapel);
        assert!(e.erro.contains("já existe"));
        e.aplicar(AgMsg::NovoNome("revisor-2".into()));
        e.aplicar(AgMsg::AdicionarPapel);
        assert!(e.erro.is_empty());
        assert!(e.editado.papeis.contains_key("revisor-2"));
        assert!(e.novo_nome.is_empty());
    }

    /// `principal` não sai; outro papel sai e leva a escada/roteamento junto, validando.
    #[test]
    fn remove_papel_mas_nao_o_principal() {
        let mut e = est();
        e.aplicar(AgMsg::RemoverPapel("principal".into()));
        assert!(e.editado.papeis.contains_key("principal"));
        assert!(e.erro.contains("principal"));
        e.aplicar(AgMsg::RemoverPapel("visual".into()));
        assert!(!e.editado.papeis.contains_key("visual"));
        assert_eq!(e.editado.roteamento.get("visual").map(String::as_str), Some("executor"));
        assert!(e.editado.validar().is_ok());
        e.aplicar(AgMsg::RemoverPapel("executor".into()));
        assert!(!e.editado.escada.contains(&"executor".to_string()));
        assert!(e.editado.validar().is_ok());
    }

    /// Salvar com modelo vazio mostra o erro tipado, não chama o gravador e segue alterado.
    #[test]
    fn salvar_invalido_mostra_o_erro() {
        let mut e = est();
        e.aplicar(AgMsg::Modelo("executor".into(), "  ".into()));
        let mut chamou = false;
        e.salvar_com(|_| {
            chamou = true;
            Ok(())
        });
        assert!(!chamou);
        assert!(e.erro.contains("executor") && e.erro.contains("modelo"), "{}", e.erro);
        assert!(e.alterado());
    }

    /// Erro do gravador (disco) também vira texto inline; sucesso zera o «alterado».
    #[test]
    fn salvar_propaga_erro_do_disco_e_sucesso_limpa() {
        let mut e = est();
        e.aplicar(AgMsg::Ativo("trivial".into(), false));
        e.salvar_com(|_| Err(ErroAgentes::Execucao("disco cheio".into())));
        assert!(e.erro.contains("disco cheio") && e.alterado());
        e.salvar_com(|_| Ok(()));
        assert!(e.erro.is_empty() && !e.alterado() && e.aviso.texto == "salvo.");
    }

    /// Padrão só entra no editor (alterado); Descartar volta ao original.
    #[test]
    fn padrao_e_descartar() {
        let mut base = Agentes::padrao();
        if let Some(p) = base.papeis.get_mut("executor") {
            p.modelo = "haiku".into();
        }
        let mut e = EstadoAgentes::com(EscopoAg::Projeto, base.clone());
        assert!(!e.alterado());
        e.aplicar(AgMsg::Padrao);
        assert!(e.alterado() && e.original == base);
        e.aplicar(AgMsg::Descartar);
        assert!(!e.alterado());
    }

    /// Trocar o motor ajusta o modelo para uma sugestão do novo motor.
    #[test]
    fn trocar_motor_ajusta_o_modelo() {
        let mut e = est();
        e.aplicar(AgMsg::Motor("executor".into(), "cursor".into()));
        let p = &e.editado.papeis["executor"];
        assert_eq!((p.motor, p.modelo.as_str()), (Motor::Cursor, "auto"));
        e.aplicar(AgMsg::Motor("executor".into(), "xyz".into()));
        assert!(e.erro.contains("desconhecido"));
    }

    /// `principal` primeiro, escada, roteamento, resto alfabético, sem duplicar.
    #[test]
    fn ordena_os_papeis() {
        let mut a = Agentes::padrao();
        a.escada = vec!["fallback-2".into(), "executor".into(), "fallback-2".into()];
        a.roteamento.insert("imagem".into(), "complementar".into());
        a.roteamento.insert("visual".into(), "visual".into());
        let o = ordem_dos_papeis(&a);
        assert_eq!(&o[..5], ["principal", "fallback-2", "executor", "visual", "complementar"]);
        assert_eq!(o.len(), a.papeis.len());
        assert_eq!(&o[5..], ["fallback-1", "fallback-cursor", "imagem", "trivial"]);
    }

    /// O resultado de um motor é registrado no lugar certo.
    #[test]
    fn registra_status_dos_motores() {
        let mut e = est();
        e.aplicar(AgMsg::Motores(Motor::Codex, Disponibilidade::SemBinario));
        let d = e.motores.iter().find(|(m, _)| *m == Motor::Codex).and_then(|x| x.1.clone());
        assert_eq!(d, Some(Disponibilidade::SemBinario));
    }
}
