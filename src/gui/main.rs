//! **schematize-overdev-gui** — a janela do `schematize-overdev`.
//!
//! **O quê:** mostra o progresso do run do projeto, o checklist, a FILA DE PERGUNTAS (o quiz,
//! que se responde clicando) e o histórico de snapshots.
//!
//! **Onde:** o ícone do app, e a aba de Overdev do hub, que delega para cá (ADR-0018 E6).
//!
//! ## Ela mora no MESMO repo do CLI (ADR-0020), e continua falando por `--json`
//!
//! **Estar no mesmo repo é conveniência de distribuição, não permissão para acoplar.** A janela
//! não usa o crate `overdev_casa` a não ser pela `nucleo::procedencia` (de qual commit ela é):
//! roda o binário e lê o `--json`. Há teste lendo o próprio fonte para cobrar isso.
//!
//! ## O que escreve, e por quê nada disso é a janela escrevendo (D6)
//!
//! Responder uma pergunta é `schematize-overdev reply …`: quem GRAVA e VALIDA é o CLI, que
//! recusa a resposta inválida com o motivo — a janela não consegue gravar uma resposta
//! inválida nem se quiser. O que REGRAVA arquivos (restaurar um snapshot) vai ao TERMINAL, onde
//! o comando aparece antes de rodar. Esta janela nunca chama `fs::write`.

mod cli;
mod telas;
mod terminal;

slint::include_modules!();

use slint::{ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

/// Tudo que a janela guarda entre um clique e outro.
struct Estado {
    /// O projeto: `--dir`, ou o diretório corrente.
    dir: PathBuf,
    /// O checklist COMPLETO; só uma página vai ao Slint.
    itens: RefCell<Vec<telas::Item>>,
    /// As perguntas lidas, para redesenhar as marcas do `multipla` sem reler o CLI.
    cards: RefCell<Vec<telas::Card>>,
    /// O que a pessoa marcou num `multipla`, entre o clique e o Confirmar.
    marcadas: RefCell<HashMap<String, Vec<String>>>,
}

/// **O quê:** um [`telas::Card`] → a struct que o `.slint` consome.
///
/// **Onde:** [`publicar_quiz`]. `marcadas` são os ids já marcados num `multipla`.
fn card_ui(c: &telas::Card, marcadas: &[String]) -> QuizUI {
    QuizUI {
        id: c.id.clone().into(),
        titulo: c.titulo.clone().into(),
        contexto: c.contexto.clone().into(),
        kind: c.kind.clone().into(),
        destrava: c.destrava.clone().into(),
        aberta: c.aberta,
        aguarda_revisao: c.aguarda_revisao,
        resposta_texto: c.resposta_texto.clone().into(),
        respondida_em: c.respondida_em.clone().into(),
        revisao: c.revisao.clone().into(),
        opcoes: ModelRc::new(VecModel::from(
            c.opcoes
                .iter()
                .map(|o| QuizOpcaoUI {
                    id: o.id.clone().into(),
                    rotulo: o.rotulo.clone().into(),
                    detalhe: o.detalhe.clone().into(),
                    sugerida: o.sugerida,
                    marcada: marcadas.contains(&o.id),
                })
                .collect::<Vec<_>>(),
        )),
    }
}

/// **O quê:** escreve a fila (e as contagens) nas propriedades da tela.
///
/// **Onde:** [`recarregar`] e o clique de marcar do `multipla` (que não relê o CLI).
fn publicar_quiz(w: &MainWindow, e: &Estado) {
    let cards = e.cards.borrow();
    let marcadas = e.marcadas.borrow();
    let vazio = Vec::new();
    w.set_quiz_abertas(cards.iter().filter(|c| c.aberta).count() as i32);
    w.set_quiz_revisar(cards.iter().filter(|c| c.aguarda_revisao).count() as i32);
    w.set_quiz(ModelRc::new(VecModel::from(
        cards.iter().map(|c| card_ui(c, marcadas.get(&c.id).unwrap_or(&vazio))).collect::<Vec<_>>(),
    )));
}

/// **O quê:** relê a fila pelo CLI.
///
/// **Onde:** [`recarregar`] e depois de CADA resposta — senão o card responderia e continuaria
/// desenhado como aberto, e a pessoa clicaria de novo.
///
/// **ERRO APARECE, e a lista não é "vazia calada":** "nenhuma pergunta" sobre uma falha de
/// leitura diria a quem tem cinco esperando que não há nada.
fn recarregar_fila(w: &MainWindow, e: &Estado) {
    match cli::rodar(&e.dir, &["fila", "--json"]).and_then(|t| telas::ler(&t)) {
        Ok(c) => {
            *e.cards.borrow_mut() = c;
            publicar_quiz(w, e);
        }
        Err(err) => {
            e.cards.borrow_mut().clear();
            publicar_quiz(w, e);
            w.set_quiz_status(err.into());
            w.set_quiz_erro(true);
        }
    }
}

/// **O quê:** escreve a PÁGINA do checklist sob o filtro atual.
///
/// **Onde:** [`recarregar`] e os chips de filtro/paginação. Só `POR_PAGINA` itens chegam ao
/// Slint, qualquer que seja o tamanho do projeto.
fn publicar_checklist(w: &MainWindow, e: &Estado) {
    let (fatia, total) =
        telas::fatia(&e.itens.borrow(), w.get_filtro(), w.get_pagina().max(0) as usize);
    let paginas = telas::paginas(total);
    w.set_pagina(w.get_pagina().clamp(0, paginas as i32 - 1));
    w.set_paginas(paginas as i32);
    w.set_total_filtrado(total as i32);
    w.set_itens(ModelRc::new(VecModel::from(
        fatia
            .iter()
            .map(|i| ItemUI {
                tipo: i.tipo.clone().into(),
                texto: i.texto.clone().into(),
                maquina: i.maquina,
                sub: i.sub,
            })
            .collect::<Vec<_>>(),
    )));
}

/// **O quê:** relê progresso, checklist, fila e histórico, e escreve a tela.
///
/// **Onde:** a abertura, o «Reler» e depois de cada resposta.
///
/// **São QUATRO documentos, e a falha de um não apaga os outros**; cada erro vai para o lugar
/// dele (o do progresso e do checklist, no topo; o da fila, na aba Perguntas).
fn recarregar(w: &MainWindow, e: &Estado) {
    let mut erros: Vec<String> = Vec::new();
    match cli::rodar(&e.dir, &["progresso", "--json"]).and_then(|t| telas::ler_progresso(&t)) {
        Ok(p) => {
            w.set_resumo(telas::resumo(&p).into());
            w.set_modo(p.modo.clone().unwrap_or_default().into());
            w.set_feitos(p.feitos as i32);
            w.set_abertos(p.abertos as i32);
            w.set_iteracoes(p.iteracoes as i32);
            w.set_teto(p.teto.unwrap_or(0) as i32);
        }
        Err(err) => {
            w.set_resumo(SharedString::new());
            w.set_modo(SharedString::new());
            erros.push(err);
        }
    }
    match cli::rodar(&e.dir, &["checklist", "--json"]).and_then(|t| telas::ler_checklist(&t)) {
        Ok(c) => {
            w.set_objetivo(c.objetivo.unwrap_or_default().into());
            *e.itens.borrow_mut() = c.itens;
        }
        Err(err) => {
            w.set_objetivo(SharedString::new());
            e.itens.borrow_mut().clear();
            erros.push(err);
        }
    }
    publicar_checklist(w, e);
    w.set_erro(erros.join("\n").into());

    w.set_quiz_status(SharedString::new());
    w.set_quiz_erro(false);
    recarregar_fila(w, e);

    match cli::rodar(&e.dir, &["history", "--json", "--limit", "50"])
        .and_then(|t| telas::ler_historico(&t))
    {
        Ok(h) => {
            w.set_snaps_erro(SharedString::new());
            w.set_snaps(ModelRc::new(VecModel::from(
                h.iter()
                    .map(|s| SnapUI {
                        id: s.id as i32,
                        arquivo: s.arquivo.clone().into(),
                        quando: telas::quando(s.em).into(),
                        tamanho: telas::tamanho(s.bytes).into(),
                    })
                    .collect::<Vec<_>>(),
            )));
        }
        Err(err) => {
            w.set_snaps(ModelRc::new(VecModel::from(Vec::<SnapUI>::new())));
            w.set_snaps_erro(err.into());
        }
    }
}

/// **O quê:** roda um subcomando da fila (`reply`/`refuse`), mostra o resultado e RELÊ a fila.
///
/// **Onde:** todos os callbacks de resposta.
///
/// **A mensagem do CLI é o produto:** ela diz POR QUE a resposta foi recusada, e a janela só a
/// repassa (primeira linha).
fn aplicar(w: &MainWindow, e: &Estado, args: &[&str]) {
    match cli::rodar(&e.dir, args) {
        Ok(msg) => {
            w.set_quiz_status(msg.lines().next().unwrap_or("").to_string().into());
            w.set_quiz_erro(false);
        }
        Err(err) => {
            w.set_quiz_status(err.lines().next().unwrap_or("").to_string().into());
            w.set_quiz_erro(true);
        }
    }
    w.set_outro(SharedString::new());
    let status = (w.get_quiz_status(), w.get_quiz_erro());
    recarregar(w, e);
    // O `recarregar` zera o status; a resposta do CLI continua sendo o que a pessoa precisa ler.
    w.set_quiz_status(status.0);
    w.set_quiz_erro(status.1);
}

/// **O quê:** abre um subcomando no TERMINAL, dentro do projeto, e diz na tela o que aconteceu.
///
/// **Onde:** a ação que REGRAVA arquivos — restaurar um snapshot (D6).
///
/// **Sem terminal, a mensagem traz o COMANDO** — quem não tem emulador ainda consegue fazer a
/// coisa colando. Um "não consegui abrir o terminal" seco é um beco sem saída (§37.48).
fn no_terminal(w: &MainWindow, e: &Estado, args: &[&str]) {
    let bin = cli::bin().display().to_string();
    let cmd = cli::comando_para_terminal(&bin, &e.dir, args);
    if terminal::abrir(&cmd) {
        w.set_msg("terminal aberto — clique em «Reler» quando ele terminar.".into());
    } else {
        w.set_msg(format!("não achei um terminal. Rode: {cmd}").into());
    }
}

/// **O quê:** o projeto pedido em `--dir <caminho>`; `None` se não veio.
///
/// **Onde:** [`main`] — é por aqui que o hub diz QUAL projeto a janela abre.
///
/// **Valor que começa com `-` é a flag seguinte**, não o valor: `--dir --aba checklist` não
/// vira um diretório chamado `--aba`. Função PURA.
fn dir_dos_args(args: &[String]) -> Option<PathBuf> {
    let i = args.iter().position(|a| a == "--dir")?;
    match args.get(i + 1) {
        Some(v) if !v.is_empty() && !v.starts_with('-') => Some(PathBuf::from(v)),
        _ => None,
    }
}

/// **O quê:** a aba pedida em `--aba <nome>`: `0` Perguntas, `1` Checklist, `2` Histórico.
///
/// **Onde:** [`main`]. Pelo NOME, e nome desconhecido cai nas Perguntas — o número é detalhe
/// interno do `.slint`. Função PURA.
fn aba_dos_args(args: &[String]) -> i32 {
    let Some(i) = args.iter().position(|a| a == "--aba") else { return 0 };
    match args.get(i + 1).map(String::as_str) {
        Some("checklist") => 1,
        Some("historico") | Some("history") => 2,
        _ => 0,
    }
}

/// **O quê:** registra os callbacks da janela.
///
/// **Onde:** [`main`]. Uma função só porque cada callback é uma linha de despacho — a regra está
/// no CLI, que valida.
fn ligar(w: &MainWindow, e: &Rc<Estado>) {
    macro_rules! com_janela {
        ($registrar:ident, |$w:ident, $e:ident $(, $a:ident)*| $corpo:block) => {{
            let weak = w.as_weak();
            let est = e.clone();
            w.$registrar(move |$($a),*| {
                if let Some($w) = weak.upgrade() {
                    let $e = &est;
                    $corpo
                }
            });
        }};
    }
    com_janela!(on_recarregar, |w, e| { recarregar(&w, e) });
    com_janela!(on_alternar_tema, |w, e| {
        let _ = e;
        w.set_dark(!w.get_dark());
    });
    com_janela!(on_responder, |w, e, qid, op| {
        aplicar(&w, e, &["reply", qid.as_str(), "--escolha", op.as_str()]);
    });
    com_janela!(on_responder_texto, |w, e, qid, texto| {
        if texto.trim().is_empty() {
            // Resposta vazia não é resposta — dizer aqui evita o round-trip.
            w.set_quiz_status("escreva a resposta antes de enviar".into());
            w.set_quiz_erro(true);
        } else {
            aplicar(&w, e, &["reply", qid.as_str(), "--texto", texto.trim()]);
        }
    });
    com_janela!(on_marcar, |w, e, qid, op| {
        {
            let mut m = e.marcadas.borrow_mut();
            let v = m.entry(qid.to_string()).or_default();
            match v.iter().position(|x| x == op.as_str()) {
                Some(i) => {
                    v.remove(i);
                }
                None => v.push(op.to_string()),
            }
        }
        // Redesenha só as marcas, sem reler o CLI: reler perderia as marcas, que o disco não tem.
        publicar_quiz(&w, e);
    });
    com_janela!(on_confirmar_multipla, |w, e, qid| {
        let escolhas = e.marcadas.borrow().get(qid.as_str()).cloned().unwrap_or_default();
        if escolhas.is_empty() {
            w.set_quiz_status("marque ao menos uma opção".into());
            w.set_quiz_erro(true);
        } else {
            let mut args: Vec<&str> = vec!["reply", qid.as_str()];
            for esc in &escolhas {
                args.push("--escolha");
                args.push(esc);
            }
            aplicar(&w, e, &args);
            e.marcadas.borrow_mut().remove(qid.as_str());
        }
    });
    com_janela!(on_marcar_feito, |w, e, qid| {
        aplicar(&w, e, &["reply", qid.as_str(), "--texto", "feito"]);
    });
    com_janela!(on_recusar, |w, e, qid| {
        // `refuse` CANCELA o item de máquina vinculado, em vez de fingir que foi feito.
        aplicar(&w, e, &["refuse", qid.as_str(), "recusado pela janela"]);
    });
    com_janela!(on_set_filtro, |w, e, f| {
        w.set_filtro(f);
        w.set_pagina(0);
        publicar_checklist(&w, e);
    });
    com_janela!(on_set_pagina, |w, e, p| {
        w.set_pagina(p);
        publicar_checklist(&w, e);
    });
    com_janela!(on_restaurar, |w, e, id| {
        // REGRAVA arquivos do projeto: vai ao terminal (D6).
        no_terminal(&w, e, &["restore", &id.to_string()]);
    });
}

fn main() -> Result<(), slint::PlatformError> {
    // **RESPONDE `--version` E SAI, antes de qualquer coisa gráfica.**
    //
    // Sem isto a janela IGNORA a flag e ABRE — e quem perguntou fica esperando. O `debugreport`
    // do hub pergunta a versão de cada binário do ecossistema.
    if std::env::args().skip(1).any(|a| a == "--version" || a == "-V") {
        println!("schematize-overdev-gui {}", overdev_casa::nucleo::procedencia::rotulo_versao());
        return Ok(());
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = dir_dos_args(&args)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let e = Rc::new(Estado {
        dir,
        itens: RefCell::new(Vec::new()),
        cards: RefCell::new(Vec::new()),
        marcadas: RefCell::new(HashMap::new()),
    });

    let w = MainWindow::new()?;
    w.set_versao(
        format!("schematize-overdev-gui {}", overdev_casa::nucleo::procedencia::rotulo_versao())
            .into(),
    );
    w.set_projeto(e.dir.display().to_string().into());
    w.set_aba(aba_dos_args(&args));
    ligar(&w, &e);
    recarregar(&w, &e);
    w.run()
}

#[cfg(test)]
mod tests {
    use super::{aba_dos_args, dir_dos_args};
    use std::path::PathBuf;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_aba_pedida_abre_pelo_nome() {
        assert_eq!(aba_dos_args(&v(&["--aba", "checklist"])), 1);
        assert_eq!(aba_dos_args(&v(&["--aba", "historico"])), 2);
        assert_eq!(aba_dos_args(&v(&["--aba", "history"])), 2);
        assert_eq!(aba_dos_args(&v(&["--aba", "perguntas"])), 0);
        assert_eq!(aba_dos_args(&v(&[])), 0);
        // Nome inventado não abre aba inventada, e a flag no fim da linha não panica.
        assert_eq!(aba_dos_args(&v(&["--aba", "turbinada"])), 0);
        assert_eq!(aba_dos_args(&v(&["--aba"])), 0);
    }

    /// `--dir` diz qual projeto abrir, e a flag seguinte não é tomada por valor.
    #[test]
    fn o_projeto_vem_de_dir() {
        assert_eq!(dir_dos_args(&v(&["--dir", "/p/x"])), Some(PathBuf::from("/p/x")));
        assert_eq!(dir_dos_args(&v(&["--aba", "checklist", "--dir", "/p"])), Some("/p".into()));
        assert_eq!(dir_dos_args(&v(&[])), None);
        assert_eq!(dir_dos_args(&v(&["--dir"])), None);
        assert_eq!(dir_dos_args(&v(&["--dir", "--aba", "checklist"])), None, "flag não é valor");
        assert_eq!(dir_dos_args(&v(&["--dir", ""])), None);
    }

    /// Código de produção desta janela, sem comentários, para os varredores abaixo.
    fn producao() -> Vec<String> {
        let fonte = include_str!("main.rs");
        let prod = fonte.split("#[cfg(test)]").next().expect("há código antes dos testes");
        prod.lines()
            .map(str::trim_start)
            .filter(|l| !l.starts_with("//"))
            .map(String::from)
            .collect()
    }

    /// **A casca não sabe NADA do domínio** (D4 do ADR-0016): a janela só usa
    /// `overdev_casa::nucleo::procedencia` (plataforma) e fala com o BINÁRIO por `--json`.
    #[test]
    fn a_casca_nao_sabe_nada_do_dominio() {
        let codigo = producao();
        for proibido in [
            "overdev_casa::overdev",
            "overdev_casa::overdevdb",
            "overdev_casa::agents",
            "overdev_casa::settings",
            "overdev_casa::projects",
            "overdev_casa::panel",
            "Command::new",
        ] {
            assert!(
                !codigo.iter().any(|l| l.contains(proibido)),
                "a janela usou `{proibido}`: ela fala com o binário por `--json`; depender do \
                 crate a faria embutir uma versão — o bug que fez a janela irmã abrir a antiga"
            );
        }
        assert!(codigo.iter().any(|l| l.contains("cli::rodar")), "o varredor está cego");
        // O mesmo vale para o parser: nada de ler o disco do run.
        let telas = include_str!("telas.rs");
        let prod = telas.split("#[cfg(test)]").next().expect("há código antes dos testes");
        assert!(!prod.contains("std::fs"), "o parser é puro: texto entra, struct sai");
    }

    /// **D6: esta janela NUNCA escreve no disco.** Quem grava é o CLI (`reply`, `refuse`) ou o
    /// terminal (`restore`). O varredor olha o código de TODA a janela, não só do `main.rs`.
    #[test]
    fn a_janela_nunca_escreve_no_disco() {
        let arquivos = [
            ("main.rs", include_str!("main.rs")),
            ("cli.rs", include_str!("cli.rs")),
            ("telas.rs", include_str!("telas.rs")),
            ("terminal.rs", include_str!("terminal.rs")),
        ];
        for (nome, fonte) in arquivos {
            let prod = fonte.split("#[cfg(test)]").next().expect("há código antes dos testes");
            for l in prod.lines().map(str::trim_start).filter(|l| !l.starts_with("//")) {
                for proibido in [
                    "fs::write",
                    "fs::create",
                    "fs::remove",
                    "fs::rename",
                    "OpenOptions",
                    "File::create",
                ] {
                    assert!(
                        !l.contains(proibido),
                        "{nome}: a janela escreveu no disco (`{proibido}`) — D6:\n  {l}"
                    );
                }
            }
        }
        // Self-check: o varredor enxerga o que importa — restaurar passa pelo terminal e a
        // resposta pelo `reply` do CLI.
        let c = producao();
        assert!(c.iter().any(|l| l.contains("no_terminal(&w, e, &[\"restore\"")), "restore sumiu");
        assert!(c.iter().any(|l| l.contains("\"reply\"")), "o reply sumiu");
    }
}
