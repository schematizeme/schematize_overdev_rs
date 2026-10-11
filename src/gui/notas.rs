//! NOTAS e ARQUIVOS do run: abrir PLAN.md/CHECKLIST.md/NOTAS.md no editor e anexar notas.
//!
//! **O quê:** lista os documentos (`arquivos --json`), mostra as notas (`notas --json`), abre um
//! documento no editor do usuário e anexa um ponto por task ou um prompt de correção
//! (`note --kind … -- texto`).
//!
//! **Onde:** a aba Notas. O editor acoplado do hub gravava o PLAN/CHECKLIST de dentro da janela;
//! aqui a janela NÃO escreve arquivo (D6, e há teste que lê o fonte para cobrar): o arquivo
//! abre no editor da pessoa, e a nota entra pelo CLI.

use super::{cli, comandos, leituras, ponte, terminal, ArquivoUI, Estado, MainWindow, Ov};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::rc::Rc;

/// **O quê:** relê os documentos e as notas e escreve a aba.
///
/// **Onde:** a abertura, o «Reler» e depois de anexar uma nota. Cada falha vai para o banner da
/// aba — uma lista de arquivos que não carregou NÃO é «nenhum arquivo».
pub fn atualizar(w: &MainWindow, e: &Estado) {
    let o = w.global::<Ov>();
    match cli::rodar(&e.dir, &["arquivos", "--json"]).and_then(|t| leituras::ler_arquivos(&t)) {
        Ok(a) => o.set_arquivos(ModelRc::new(VecModel::from(
            a.into_iter()
                .map(|x| ArquivoUI {
                    nome: x.nome.into(),
                    caminho: x.caminho.into(),
                    existe: x.existe,
                })
                .collect::<Vec<_>>(),
        ))),
        Err(err) => {
            o.set_arquivos(ModelRc::new(VecModel::from(Vec::<ArquivoUI>::new())));
            o.set_notas_status(err.lines().next().unwrap_or("").to_string().into());
            o.set_notas_erro(true);
        }
    }
    match cli::rodar(&e.dir, &["notas", "--json"]).and_then(|t| leituras::ler_notas(&t)) {
        Ok(t) => o.set_notas_texto(t.trim().to_string().into()),
        Err(err) => {
            o.set_notas_texto(SharedString::new());
            o.set_notas_status(err.lines().next().unwrap_or("").to_string().into());
            o.set_notas_erro(true);
        }
    }
}

/// **O quê:** registra os callbacks da aba. **Onde:** [`super::main`], uma vez.
pub fn ligar(w: &MainWindow, e: &Rc<Estado>) {
    com_ov!(w, e, on_abrir_arquivo, |w, e, caminho| {
        let _ = e;
        let o = w.global::<Ov>();
        match terminal::abrir_arquivo(caminho.as_str()) {
            Ok(m) => {
                o.set_notas_status(m.into());
                o.set_notas_erro(false);
            }
            Err(m) => {
                o.set_notas_status(m.into());
                o.set_notas_erro(true);
            }
        }
    });
    com_ov!(w, e, on_nota_adicionar, |w, e, correcao| {
        let o = w.global::<Ov>();
        let texto = if correcao { o.get_nota_correcao() } else { o.get_nota_task() };
        if texto.trim().is_empty() {
            o.set_notas_status("escreva a nota antes de anexar.".into());
            o.set_notas_erro(true);
        } else {
            let (msg, erro) = ponte::rodar(e, &comandos::nota(correcao, texto.as_str()));
            if !erro {
                // Só limpa o campo se a nota entrou: falhou, o texto digitado fica.
                if correcao {
                    o.set_nota_correcao(SharedString::new());
                } else {
                    o.set_nota_task(SharedString::new());
                }
            }
            o.set_notas_status(msg.into());
            o.set_notas_erro(erro);
            atualizar(&w, e);
        }
    });
}
