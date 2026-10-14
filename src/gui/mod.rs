//! **A janela do `schematize-overdev`** (`schematize-overdev-gui`), como módulo da lib (ADR-0026).
//!
//! **O quê:** mostra o progresso do run do projeto, o checklist, a FILA DE PERGUNTAS (o quiz,
//! que se responde clicando), o histórico de snapshots, a execução, a caixa de entrada e as notas.
//! Troca de projeto pelo botão «Trocar projeto».
//!
//! **Onde:** o ícone do app, e a aba de Overdev do hub, que delega para cá (ADR-0018 E6). O
//! binário `schematize-overdev-gui` é só `gui::run(args, &Plataforma, &dyn Hooks)`; o repo
//! `schematize_windows_rs` chama o mesmo `run` com as costuras do Windows. Atrás da feature `gui`.
//!
//! ## iced, no menor perfil que existe (tiny-skia, sem wgpu)
//!
//! A janela é Elm: [`estado::App`] é o estado, [`msg::Msg`] o evento, [`update`] muda o estado e
//! devolve a tarefa do efeito, [`view`] desenha. Renderer em CPU (`tiny-skia`): ~19 MB de RSS
//! contra ~124 MB do Slint+femtovg, e cerca de metade das crates. Medidas em
//! `.schematize/scratch/ui-spike/`.
//!
//! ## Ela mora no MESMO repo do CLI (ADR-0020), e continua falando por `--json`
//!
//! **Estar no mesmo repo é conveniência de distribuição, não permissão para acoplar.** A janela
//! não usa o domínio do crate a não ser pela `nucleo::procedencia` (de qual commit ela é) e
//! pelo seletor de projeto (`projects`/`config`, que são plataforma): roda o binário e lê o
//! `--json`. Há teste lendo o próprio fonte para cobrar isso.
//!
//! ## O que escreve, e por quê nada disso é a janela escrevendo (D6)
//!
//! Responder uma pergunta é `schematize-overdev reply …`: quem GRAVA e VALIDA é o CLI, que
//! recusa a resposta inválida com o motivo — a janela não consegue gravar uma resposta
//! inválida nem se quiser. O que REGRAVA arquivos (restaurar um snapshot) vai ao TERMINAL, onde
//! o comando aparece antes de rodar. Esta janela nunca grava arquivo de PROJETO; a única escrita
//! é a memória do último projeto escolhido (ver [`projeto`]).

mod agentes_estado;
mod agentes_tela;
mod caixa;
mod carga;
mod checklist;
mod cli;
mod comandos;
mod estado;
mod execucao;
mod externos;
mod historico;
mod home;
mod leituras;
mod msg;
mod notas;
mod perguntas;
mod ponte;
mod projeto;
mod resolver;
mod sino;
mod tarefa;
mod telas;
mod tema;
mod terminal;
mod topo;
mod update;
mod view;
mod widgets;

use crate::hooks::Hooks;
use estado::App;
use iced::{window, Size, Task};
use msg::{Aba, Msg};
use schematize_plataforma::seams::Plataforma;

use std::path::PathBuf;

/// **O quê:** o projeto pedido em `--dir <caminho>`; `None` se não veio.
///
/// **Onde:** [`iniciar`] — é por aqui que o hub diz QUAL projeto a janela abre.
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

/// **O quê:** a aba pedida em `--aba <nome>`: Perguntas (padrão), Checklist, Histórico, Execução,
/// Caixa, Notas ou Agentes.
///
/// **Onde:** [`iniciar`]. Pelo NOME, e nome desconhecido cai nas Perguntas. Função PURA. O hub
/// tem um teste que lê ESTE arquivo e procura `Some("<aba>")` — manter os literais aqui.
fn aba_dos_args(args: &[String]) -> Aba {
    let Some(i) = args.iter().position(|a| a == "--aba") else { return Aba::Perguntas };
    match args.get(i + 1).map(String::as_str) {
        Some("projeto") => Aba::Projeto,
        Some("checklist") => Aba::Checklist,
        Some("historico") | Some("history") => Aba::Historico,
        Some("execucao") => Aba::Execucao,
        Some("caixa") => Aba::Caixa,
        Some("notas") => Aba::Notas,
        Some("agentes") => Aba::Agentes,
        _ => Aba::Perguntas,
    }
}

/// **O quê:** o estado de abertura e a primeira leitura.
///
/// **Onde:** o `boot` do `iced::application`. O projeto é `--dir`, senão o último escolhido no
/// seletor, senão o diretório corrente.
fn iniciar(args: &[String]) -> (App, Task<Msg>) {
    let dados = projeto::dados_dir();
    let cwd = std::env::current_dir().ok();
    let dir = projeto::dir_inicial(dir_dos_args(args), &dados, cwd);
    let versao = format!("schematize-overdev-gui {}", crate::nucleo::procedencia::rotulo_versao());
    let app = App::novo(dir, dados, versao, aba_dos_args(args), true);
    let primeira = Task::batch([
        app.recarregar(carga::Escopo::TUDO),
        tarefa::projetos(app.dir.clone()),
        tarefa::sino(app.dir.clone(), false, false),
        tarefa::espera_sino(),
        tarefa::versao_do_hub(app.dir.clone()),
    ]);
    (app, primeira)
}

/// **O quê:** abre a janela: responde `--version` e sai, garante o market e roda o `iced`.
/// `args` é o argv completo (com o argv0).
///
/// **Onde:** o `main` de `schematize-overdev-gui` (e do `.exe` no repo Windows). `plat` e
/// `hooks` ficam instalados em [`crate::ambiente`] enquanto a janela roda (terminal, abridor e
/// processos das ações passam por eles).
pub fn run(args: Vec<String>, plat: &Plataforma, hooks: &dyn Hooks) -> iced::Result {
    let args: Vec<String> = args.into_iter().skip(1).collect();
    // **RESPONDE `--version` E SAI, antes de qualquer coisa gráfica.**
    //
    // Sem isto a janela IGNORA a flag e ABRE — e quem perguntou fica esperando. O `debugreport`
    // do hub pergunta a versão de cada binário do ecossistema.
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("schematize-overdev-gui {}", crate::nucleo::procedencia::rotulo_versao());
        return Ok(());
    }
    crate::ambiente::com(plat, hooks, || {
        // Parasita do market: antes de abrir a janela (bloqueante só quando falta, uma vez por máquina).
        crate::ambiente::garantir_market();
        iced::application(move || iniciar(&args), App::update, App::view)
            .title(App::titulo)
            .theme(|a: &App| a.tema.clone())
            .window(config_janela())
            .run()
    })
}

/// O quê: monta as `window::Settings` da janela (tamanho, mínimo e, no Linux, o `application_id`).
/// Onde: chamada por `main` ao configurar o `iced::application`.
/// Por que separada: `PlatformSpecific::application_id` SÓ existe com `target_os = "linux"` no iced
/// 0.14 (no macOS/Windows a struct tem outros campos); o literal incondicional quebrava o build
/// release de macOS e Windows. O `cfg` fica num bloco só, sem `..Default::default()` por alvo.
fn config_janela() -> window::Settings {
    #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
    let mut s = window::Settings {
        size: Size::new(980.0, 720.0),
        min_size: Some(Size::new(680.0, 480.0)),
        ..window::Settings::default()
    };
    // O `StartupWMClass` do `.desktop` é o nome deste binário: sem o mesmo id na janela o
    // lançador não a casa com o ícone. Conceito só do Linux (X11/Wayland).
    #[cfg(target_os = "linux")]
    {
        s.platform_specific.application_id = "schematize-overdev-gui".to_string();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::{aba_dos_args, dir_dos_args, Aba};
    use std::path::PathBuf;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_aba_pedida_abre_pelo_nome() {
        assert_eq!(aba_dos_args(&v(&["--aba", "checklist"])), Aba::Checklist);
        assert_eq!(aba_dos_args(&v(&["--aba", "historico"])), Aba::Historico);
        assert_eq!(aba_dos_args(&v(&["--aba", "history"])), Aba::Historico);
        assert_eq!(aba_dos_args(&v(&["--aba", "execucao"])), Aba::Execucao);
        assert_eq!(aba_dos_args(&v(&["--aba", "caixa"])), Aba::Caixa);
        assert_eq!(aba_dos_args(&v(&["--aba", "notas"])), Aba::Notas);
        assert_eq!(aba_dos_args(&v(&["--aba", "agentes"])), Aba::Agentes);
        assert_eq!(aba_dos_args(&v(&["--aba", "perguntas"])), Aba::Perguntas);
        assert_eq!(aba_dos_args(&v(&[])), Aba::Perguntas);
        // Nome inventado não abre aba inventada, e a flag no fim da linha não panica.
        assert_eq!(aba_dos_args(&v(&["--aba", "turbinada"])), Aba::Perguntas);
        assert_eq!(aba_dos_args(&v(&["--aba"])), Aba::Perguntas);
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

    /// **Todos** os fontes de produção da janela. Um arquivo novo que não entre aqui escapa dos
    /// varredores — por isso o teste `a_lista_de_fontes_cobre_a_pasta` confere com o disco.
    fn fontes_da_janela() -> Vec<(&'static str, &'static str)> {
        vec![
            ("mod.rs", include_str!("mod.rs")),
            ("agentes_estado.rs", include_str!("agentes_estado.rs")),
            ("agentes_tela.rs", include_str!("agentes_tela.rs")),
            ("caixa.rs", include_str!("caixa.rs")),
            ("carga.rs", include_str!("carga.rs")),
            ("checklist.rs", include_str!("checklist.rs")),
            ("cli.rs", include_str!("cli.rs")),
            ("comandos.rs", include_str!("comandos.rs")),
            ("estado.rs", include_str!("estado.rs")),
            ("execucao.rs", include_str!("execucao.rs")),
            ("externos.rs", include_str!("externos.rs")),
            ("historico.rs", include_str!("historico.rs")),
            ("home.rs", include_str!("home.rs")),
            ("leituras.rs", include_str!("leituras.rs")),
            ("msg.rs", include_str!("msg.rs")),
            ("notas.rs", include_str!("notas.rs")),
            ("perguntas.rs", include_str!("perguntas.rs")),
            ("ponte.rs", include_str!("ponte.rs")),
            ("projeto.rs", include_str!("projeto.rs")),
            ("resolver.rs", include_str!("resolver.rs")),
            ("sino.rs", include_str!("sino.rs")),
            ("tarefa.rs", include_str!("tarefa.rs")),
            ("telas.rs", include_str!("telas.rs")),
            ("tema.rs", include_str!("tema.rs")),
            ("terminal.rs", include_str!("terminal.rs")),
            ("topo.rs", include_str!("topo.rs")),
            ("update.rs", include_str!("update.rs")),
            ("view.rs", include_str!("view.rs")),
            ("widgets.rs", include_str!("widgets.rs")),
        ]
    }

    /// O código de produção de um fonte: sem os testes e sem os comentários, linha a linha.
    fn producao_de(fonte: &str) -> Vec<String> {
        let prod = fonte.split("#[cfg(test)]").next().expect("há código antes dos testes");
        prod.lines()
            .map(str::trim_start)
            .filter(|l| !l.starts_with("//"))
            .map(String::from)
            .collect()
    }

    /// A lista acima cobre CADA `.rs` da pasta da janela (e nenhum a mais).
    #[test]
    fn a_lista_de_fontes_cobre_a_pasta() {
        let mut no_disco: Vec<String> =
            std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src/gui"))
                .expect("ler src/gui")
                .filter_map(|e| e.ok()?.file_name().into_string().ok())
                .filter(|n| n.ends_with(".rs"))
                .collect();
        no_disco.sort();
        let mut lista: Vec<String> =
            fontes_da_janela().iter().map(|(n, _)| n.to_string()).collect();
        lista.sort();
        assert_eq!(lista, no_disco, "arquivo novo na janela: ponha-o em `fontes_da_janela`");
    }

    /// **Nenhum arquivo da janela passa de 750 linhas** (o teto da casa): a janela é dividida por
    /// tela justamente para isso.
    #[test]
    fn nenhum_arquivo_da_janela_passa_de_750_linhas() {
        for (nome, fonte) in fontes_da_janela() {
            assert!(fonte.lines().count() <= 750, "{nome} tem {} linhas", fonte.lines().count());
        }
    }

    /// **A janela não carrega Slint nem nenhum toolkit além do iced** — e nenhum `.slint` sobrou.
    #[test]
    fn nao_sobrou_slint() {
        for (nome, fonte) in fontes_da_janela() {
            if nome == "mod.rs" {
                // O próprio varredor cita a palavra.
                continue;
            }
            assert!(!producao_de(fonte).iter().any(|l| l.contains("slint::")), "{nome} usa slint");
        }
        let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
        assert!(!ui.exists(), "a pasta `ui/` (.slint) devia ter sido removida");
    }

    /// **A casca não sabe NADA do domínio** (D4 do ADR-0016): a janela só usa
    /// `crate::nucleo::procedencia` (plataforma), o seletor de projeto (`projeto.rs`, que
    /// usa `projects`/`config`, também plataforma) e fala com o BINÁRIO por `--json`.
    #[test]
    fn a_casca_nao_sabe_nada_do_dominio() {
        let mut viu_cli = false;
        for (nome, fonte) in fontes_da_janela() {
            let codigo = producao_de(fonte);
            viu_cli |= codigo.iter().any(|l| l.contains("cli::rodar"));
            for l in &codigo {
                for proibido in [
                    "crate::overdev",
                    "crate::overdevdb",
                    "crate::agents",
                    "crate::settings",
                    "crate::panel",
                    "crate::agentrun",
                ] {
                    assert!(
                        !l.contains(proibido),
                        "{nome} usou `{proibido}`: a janela fala com o binário por `--json`; \
                         depender do crate a faria embutir uma versão — o bug que fez a janela \
                         irmã abrir a antiga"
                    );
                }
            }
        }
        assert!(viu_cli, "o varredor está cego");
        // O mesmo vale para os parsers: nada de ler o disco do run.
        for (nome, fonte) in
            [("telas.rs", include_str!("telas.rs")), ("leituras.rs", include_str!("leituras.rs"))]
        {
            let prod = fonte.split("#[cfg(test)]").next().expect("há código antes dos testes");
            assert!(!prod.contains("std::fs"), "{nome} é puro: texto entra, struct sai");
        }
        // E NENHUMA tela usa o crate do domínio nem sobe processo por conta própria: elas falam por
        // `cli::rodar`/`ponte` (a casca) e a abertura do terminal/editor é da plataforma. Exceções:
        // `projeto.rs` (plataforma: lista e memória do projeto) e `mod.rs`/`cli.rs`/`terminal.rs`.
        for (nome, fonte) in fontes_da_janela() {
            // `agentes_*`: a aba Agentes edita o `agentes.toml` (config, não o domínio do run) pela API
            // `crate::agentes` — ADR-0024; checar motores sobe `codex login status` por ela.
            if matches!(
                nome,
                "mod.rs"
                    | "cli.rs"
                    | "terminal.rs"
                    | "projeto.rs"
                    | "externos.rs"
                    | "agentes_estado.rs"
                    | "agentes_tela.rs"
            ) {
                continue;
            }
            for l in producao_de(fonte) {
                for proibido in ["crate::", "Command::new", "std::process"] {
                    assert!(!l.contains(proibido), "{nome} usou `{proibido}`: {l}");
                }
            }
        }
    }

    /// **`projeto.rs` só toca a PLATAFORMA do crate**: `projects` (a varredura), `config` (pastas
    /// de desenvolvimento e fixados) e `nucleo::{caminhos,util}` (onde o app guarda estado).
    #[test]
    fn o_seletor_so_usa_a_plataforma_do_crate() {
        let permitido =
            ["crate::projects", "crate::config", "crate::nucleo::caminhos", "crate::nucleo::util"];
        let mut usos = 0;
        for l in producao_de(include_str!("projeto.rs")) {
            for (i, _) in l.match_indices("crate::") {
                usos += 1;
                assert!(permitido.iter().any(|p| l[i..].starts_with(p)), "projeto.rs usou: {l}");
            }
        }
        assert!(usos > 0, "o varredor está cego");
    }

    /// **D6: esta janela NUNCA escreve arquivo de PROJETO.** Quem grava é o CLI (`reply`,
    /// `refuse`) ou o terminal (`restore`). O varredor olha o código de TODA a janela.
    ///
    /// A única escrita é a memória do último projeto (`projeto.rs`), pela escrita atômica da
    /// plataforma, na pasta de dados do app — nunca dentro de um projeto.
    #[test]
    fn a_janela_nunca_escreve_no_disco() {
        for (nome, fonte) in fontes_da_janela() {
            for l in producao_de(fonte) {
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
        // A exceção é UMA e é vigiada: só `projeto.rs` chama a escrita atômica, e só para o
        // arquivo do último projeto.
        let mut escritores = Vec::new();
        for (nome, fonte) in fontes_da_janela() {
            if producao_de(fonte).iter().any(|l| l.contains("escreve_atomico")) {
                escritores.push(nome);
            }
        }
        assert_eq!(escritores, ["projeto.rs"], "só o seletor grava, e só a memória do projeto");
        assert!(
            producao_de(include_str!("projeto.rs"))
                .iter()
                .any(|l| l.contains("escreve_atomico(&arquivo_ultimo(dados)")),
            "a escrita do seletor não é mais a do arquivo do último projeto"
        );
        // Self-check: o varredor enxerga o que importa — restaurar passa pelo terminal e a
        // resposta pelo `reply` do CLI.
        let todos: String = fontes_da_janela()
            .iter()
            .flat_map(|(_, f)| f.split("#[cfg(test)]").next())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(todos.contains("\"restore\""), "restore sumiu");
        assert!(todos.contains("\"reply\""), "o reply sumiu");
        // As ações também passam pelo CLI ou pelo terminal — nunca por escrita local.
        for sub in ["\"human\"", "\"answer\"", "\"refuse\"", "\"note\"", "\"split\"", "\"stop\""] {
            assert!(todos.contains(sub), "a ação {sub} sumiu da janela");
        }
        assert!(
            producao_de(include_str!("execucao.rs"))
                .iter()
                .any(|l| l.contains("tarefa::terminal(self.dir.clone(), args, Origem::Executar)")),
            "Executar não abre o terminal"
        );
    }
}
