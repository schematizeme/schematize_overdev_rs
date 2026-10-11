//! A CAIXA DE ENTRADA: capturar uma demanda, organizar com um agente e fundir no checklist.
//!
//! **O quê:** `add` (captura), `caixa agente` (abre um agente num terminal para organizar) e
//! `caixa merge` (funde as organizadas no checklist), mais a leitura por `caixa list --json`.
//!
//! **Onde:** a aba Caixa. Existe para jogar «isso também precisa» no projeto ENQUANTO um agente
//! trabalha, sem interromper ninguém e sem tocar no checklist.
//!
//! **Quem grava é o CLI** — a captura é atômica e a fusão roda sob trava; a janela só dispara.
//! `caixa agente` é o PRÓPRIO comando do app que abre o terminal (o agente mexe no projeto, e
//! mexer sem a pessoa ver acontecer é o que a casa não faz).

use super::{cli, comandos, leituras, ponte, CaixaUI, Estado, MainWindow, Ov};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::rc::Rc;

/// **O quê:** uma demanda da caixa → a struct do Slint. **Onde:** [`atualizar`].
///
/// Os itens organizados viram `• item` por linha; a demanda crua não tem itens.
fn demanda_ui(d: &leituras::Demanda) -> CaixaUI {
    CaixaUI {
        id: d.id.clone().into(),
        texto: d.texto.clone().into(),
        itens: d.itens.iter().map(|i| format!("• {i}")).collect::<Vec<_>>().join("\n").into(),
    }
}

/// **O quê:** relê a caixa por `caixa list --json` e escreve as duas listas.
///
/// **Onde:** a abertura, o «Reler» e depois de cada ação da aba.
///
/// **ERRO APARECE:** uma caixa que não leu NÃO é uma caixa vazia — "nenhuma demanda esperando"
/// sobre uma falha de leitura mandaria a pessoa reescrever o que já capturou.
pub fn atualizar(w: &MainWindow, e: &Estado) {
    let o = w.global::<Ov>();
    match cli::rodar(&e.dir, &["caixa", "list", "--json"]).and_then(|t| leituras::ler_caixa(&t)) {
        Ok(c) => {
            let uis = |v: &[leituras::Demanda]| {
                ModelRc::new(VecModel::from(v.iter().map(demanda_ui).collect::<Vec<_>>()))
            };
            o.set_caixa_pendentes(uis(&c.pendentes));
            o.set_caixa_prontas(uis(&c.processadas));
        }
        Err(err) => {
            o.set_caixa_pendentes(ModelRc::new(VecModel::from(Vec::<CaixaUI>::new())));
            o.set_caixa_prontas(ModelRc::new(VecModel::from(Vec::<CaixaUI>::new())));
            o.set_caixa_status(err.lines().next().unwrap_or("").to_string().into());
            o.set_caixa_erro(true);
        }
    }
}

/// **O quê:** escreve o banner da aba. **Onde:** as ações abaixo.
fn banner(w: &MainWindow, msg: String, erro: bool) {
    w.global::<Ov>().set_caixa_status(msg.into());
    w.global::<Ov>().set_caixa_erro(erro);
}

/// **O quê:** registra os callbacks da caixa. **Onde:** [`super::main`], uma vez.
pub fn ligar(w: &MainWindow, e: &Rc<Estado>) {
    com_ov!(w, e, on_caixa_adicionar, |w, e| {
        let texto = w.global::<Ov>().get_caixa_texto().to_string();
        if texto.trim().is_empty() {
            banner(&w, "escreva a demanda antes de capturar.".into(), true);
        } else {
            let (msg, erro) = ponte::rodar(e, &comandos::capturar(&texto));
            if !erro {
                // Só limpa o campo se capturou: se falhou, o texto digitado NÃO se perde.
                w.global::<Ov>().set_caixa_texto(SharedString::new());
            }
            banner(&w, msg, erro);
            atualizar(&w, e);
        }
    });
    com_ov!(w, e, on_caixa_agente, |w, e| {
        let (msg, erro) = ponte::rodar(e, &comandos::caixa_agente());
        // Sem terminal o CLI sai 0 e imprime o prompt: é uma falha para a pessoa, não um sucesso.
        let falhou = erro || msg.starts_with("não consegui");
        banner(&w, msg, falhou);
    });
    com_ov!(w, e, on_caixa_mesclar, |w, e| {
        let (msg, erro) = ponte::rodar(e, &comandos::caixa_mesclar());
        banner(&w, msg, erro);
        atualizar(&w, e);
        // Os itens novos têm de aparecer na hora, senão parece que a fusão não fez nada.
        crate::recarregar(&w, e);
    });
}
