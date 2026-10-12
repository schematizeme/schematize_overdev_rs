//! O SELETOR DE PROJETO: trocar o projeto da janela, e lembrar qual foi o último.
//!
//! **O quê:** a lista de projetos (fixados primeiro), a validação do caminho digitado, a troca do
//! `dir` do [`App`] (que reinicia tudo e relê) e a memória do último projeto escolhido.
//!
//! **Onde:** o botão «Trocar projeto» do cabeçalho; e [`super::iniciar`], que decide o projeto de
//! abertura: `--dir` > último escolhido > diretório corrente.
//!
//! ## Esta é a ÚNICA parte da janela que escreve no disco, e escreve uma linha
//!
//! A janela nunca grava arquivo do PROJETO (D6): quem grava é o CLI ou o terminal. Lembrar o
//! último projeto é estado da PRÓPRIA janela, na pasta de dados do app (`~/.claude/schematize/`),
//! e não toca em projeto nenhum. A escrita é atômica (`escreve_atomico`) e o teste que vigia D6
//! lista este módulo como a exceção, e confere que é a única.
//!
//! ## `--dir` vence a memória
//!
//! O hub abre a janela com `--dir <projeto>` quando a pessoa clicou num projeto LÁ. Esse pedido
//! explícito não pode ser desfeito por uma memória antiga — e também NÃO a sobrescreve: a memória
//! é só do que se escolheu AQUI.

use super::carga::Escopo;
use super::estado::App;
use super::msg::{Msg, Opcao};
use super::tema;
use super::widgets::{btn, modal, negrito, status, txt};
use iced::widget::{button, column, row, scrollable, text_input, Space};
use iced::{Element, Fill, Length};
pub use overdev_casa::projects::Project;
use std::path::{Path, PathBuf};

/// O nome do arquivo (na pasta de dados do app) que guarda o último projeto escolhido.
pub const ARQUIVO_ULTIMO: &str = "overdev-gui-ultimo-projeto";

/// O estado do modal «Trocar projeto».
#[derive(Debug, Default)]
pub struct Seletor {
    pub aberto: bool,
    /// A lista chegou? `false` enquanto a varredura roda.
    pub carregando: bool,
    pub lista: Vec<Project>,
    /// As entradas do seletor flutuante do topo (fixados primeiro, e «Outro caminho…» no fim).
    pub opcoes: Vec<Opcao>,
    /// O estado do `combo_box` do topo (o texto digitado e as opções filtradas).
    pub combo: iced::widget::combo_box::State<Opcao>,
    /// O que a pessoa digitou/colou no campo de caminho.
    pub caminho: String,
    /// Por que o caminho foi recusado. Vazio = sem erro.
    pub erro: String,
}

impl Seletor {
    /// **O quê:** o seletor fechado, MANTENDO a lista de projetos já lida. **Onde:** a troca de
    /// projeto: o seletor flutuante do topo precisa da lista sempre à mão, não só com o modal aberto.
    pub fn fechado(&self) -> Seletor {
        Seletor {
            lista: self.lista.clone(),
            opcoes: self.opcoes.clone(),
            combo: self.combo.clone(),
            ..Seletor::default()
        }
    }

    /// **O quê:** esvazia o filtro do `combo_box` (todas as opções aparecem). **Onde:** o seletor
    /// do topo ao abrir: com o nome do projeto atual no campo só ELE passaria no filtro.
    pub fn mostrar_todos(&mut self) {
        self.combo = iced::widget::combo_box::State::new(self.opcoes.clone());
    }

    /// **O quê:** refaz as opções e o estado do `combo_box` para o projeto `atual`. **Onde:** a
    /// chegada da lista e a troca de projeto — o texto do campo é SEMPRE o projeto atual.
    pub fn refazer(&mut self, atual: &Path) {
        self.opcoes = opcoes_de(&self.lista, atual);
        let sel = self.opcoes.iter().find(|o| Path::new(&o.caminho) == atual).cloned();
        self.combo =
            iced::widget::combo_box::State::with_selection(self.opcoes.clone(), sel.as_ref());
    }
}

/// **O quê:** as entradas do seletor flutuante: o projeto ATUAL (se a varredura não o achou, entra
/// no topo), os da lista na ordem em que vieram (fixados primeiro) e «Outro caminho…». **Onde:**
/// `Msg::Projetos` e [`Seletor::fechado`]. PURA.
pub fn opcoes_de(lista: &[Project], atual: &Path) -> Vec<Opcao> {
    let nome =
        |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut v: Vec<Opcao> = Vec::new();
    if !lista.iter().any(|p| Path::new(&p.path) == atual) {
        v.push(Opcao { rotulo: nome(atual), caminho: atual.display().to_string() });
    }
    v.extend(lista.iter().map(|p| Opcao { rotulo: p.name.clone(), caminho: p.path.clone() }));
    v.push(Opcao { rotulo: "Outro caminho…".to_string(), caminho: String::new() });
    v
}

/// **O quê:** o arquivo da memória do último projeto, dentro de `dados`. **Onde:**
/// [`gravar_ultimo`] e [`ler_ultimo`].
fn arquivo_ultimo(dados: &Path) -> PathBuf {
    dados.join(ARQUIVO_ULTIMO)
}

/// **O quê:** a pasta de dados do app (`~/.claude/schematize/`, pela regra "ler ambos").
///
/// **Onde:** [`super::iniciar`]. É a única função daqui que lê o ambiente (o HOME); o resto
/// recebe a pasta por parâmetro, e por isso os testes usam uma pasta própria.
pub fn dados_dir() -> PathBuf {
    overdev_casa::nucleo::caminhos::dados_dir()
}

/// **O quê:** grava `dir` como o último projeto escolhido, em `dados`.
///
/// **Onde:** [`App::trocar_para`], depois de uma troca VÁLIDA. Escrita atômica (nunca deixa o
/// arquivo pela metade) e o erro volta ao chamador, que o mostra: lembrar é conveniência, mas
/// falhar calado deixaria a pessoa reabrir o projeto errado sem saber por quê.
pub fn gravar_ultimo(dados: &Path, dir: &Path) -> Result<(), String> {
    let texto = dir.to_str().ok_or("o caminho do projeto não é texto UTF-8 válido")?;
    overdev_casa::nucleo::caminhos::escreve_atomico(&arquivo_ultimo(dados), texto)
}

/// **O quê:** o último projeto gravado em `dados`, SE a pasta ainda existir.
///
/// **Onde:** [`dir_inicial`]. Memória ausente, vazia ou apontando para uma pasta que sumiu vale
/// como "sem memória": abrir um caminho morto seria pior que abrir o diretório corrente. Um erro
/// de leitura que não seja "não existe" (permissão, p. ex.) vai ao `stderr` — a abertura segue,
/// mas o motivo não fica escondido.
pub fn ler_ultimo(dados: &Path) -> Option<PathBuf> {
    let arquivo = arquivo_ultimo(dados);
    let texto = match std::fs::read_to_string(&arquivo) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            eprintln!("schematize-overdev-gui: não consegui ler {}: {e}", arquivo.display());
            return None;
        }
    };
    let dir = PathBuf::from(texto.trim());
    (!texto.trim().is_empty() && dir.is_dir()).then_some(dir)
}

/// **O quê:** o projeto de abertura: `--dir` > último escolhido > diretório corrente > `.`.
///
/// **Onde:** [`super::iniciar`]. PURA no que decide (a leitura da memória é a de [`ler_ultimo`]).
/// O `--dir` vence mesmo com memória válida e NÃO a sobrescreve: é um pedido de quem abriu a
/// janela (o hub), não uma escolha feita aqui.
pub fn dir_inicial(arg: Option<PathBuf>, dados: &Path, cwd: Option<PathBuf>) -> PathBuf {
    arg.or_else(|| ler_ultimo(dados)).or(cwd).unwrap_or_else(|| PathBuf::from("."))
}

/// **O quê:** valida o caminho que a pessoa digitou/escolheu e o devolve CANÔNICO (absoluto, sem
/// `..` nem symlink). `home` resolve o `~`.
///
/// **Onde:** [`validar`] e os testes. Só vale PASTA existente: um projeto sem run é legítimo
/// ("sem run neste diretório"), então não se exige marcador. O erro cita o que foi digitado.
pub fn validar_com_home(entrada: &str, home: Option<&Path>) -> Result<PathBuf, String> {
    let t = entrada.trim();
    if t.is_empty() {
        return Err("digite o caminho do projeto.".to_string());
    }
    let bruto = match (t, home) {
        ("~", Some(h)) => h.to_path_buf(),
        (_, Some(h)) if t.starts_with("~/") => h.join(&t[2..]),
        (_, None) if t == "~" || t.starts_with("~/") => {
            return Err(
                "não sei onde fica a sua pasta pessoal (~): digite o caminho completo.".into()
            )
        }
        _ => PathBuf::from(t),
    };
    let canonico = std::fs::canonicalize(&bruto).map_err(|e| format!("não achei «{t}»: {e}"))?;
    if canonico.is_dir() {
        Ok(canonico)
    } else {
        Err(format!("«{t}» não é uma pasta."))
    }
}

/// **O quê:** [`validar_com_home`] com a home do ambiente. **Onde:** [`App::trocar_para`].
pub fn validar(entrada: &str) -> Result<PathBuf, String> {
    let home = overdev_casa::nucleo::util::home();
    validar_com_home(entrada, Some(&home))
}

/// **O quê:** a lista de projetos para o seletor: os FIXADOS primeiro, depois os descobertos.
///
/// **Onde:** [`carregar_lista`]. Quem manda é o `scan_with_pins` (fixados entram mesmo sem
/// marcador e antes dos outros). **Sem pasta de desenvolvimento cadastrada** varre o PAI do
/// projeto atual — a convenção é projetos lado a lado, e uma lista vazia num primeiro uso seria
/// um beco sem saída (§37.48).
pub fn listar(dev_dirs: &[String], fixados: &[String], atual: &Path) -> Vec<Project> {
    if dev_dirs.is_empty() {
        let pai: Vec<String> =
            atual.parent().map(|p| p.display().to_string()).into_iter().collect();
        return overdev_casa::projects::scan_with_pins(&pai, fixados);
    }
    overdev_casa::projects::scan_with_pins(dev_dirs, fixados)
}

/// **O quê:** lê a config do app (pastas de desenvolvimento e fixados) e monta a lista.
///
/// **Onde:** a `Task` de `Msg::AbrirSeletor`. BLOQUEIA na varredura do disco: nunca no `update`.
pub fn carregar_lista(atual: &Path) -> Vec<Project> {
    let c = overdev_casa::config::load();
    listar(&c.dev_dirs, &c.projects, atual)
}

impl App {
    /// **O quê:** trata as mensagens do seletor. **Onde:** [`App::update`].
    pub fn atualizar_seletor(&mut self, m: Msg) -> iced::Task<Msg> {
        match m {
            Msg::AbrirSeletor => {
                self.seletor = Seletor { aberto: true, carregando: true, ..Seletor::default() };
                let atual = self.dir.clone();
                iced::Task::perform(async move { carregar_lista(&atual) }, Msg::Projetos)
            }
            Msg::FecharSeletor => {
                self.seletor.aberto = false;
                iced::Task::none()
            }
            Msg::Projetos(lista) => {
                self.seletor.lista = lista;
                let atual = self.dir.clone();
                self.seletor.refazer(&atual);
                self.seletor.carregando = false;
                iced::Task::none()
            }
            Msg::SeletorCaminho(t) => {
                self.seletor.caminho = t;
                self.seletor.erro.clear();
                iced::Task::none()
            }
            Msg::SeletorAbrirCaminho => {
                let t = self.seletor.caminho.clone();
                self.trocar_para(&t)
            }
            Msg::SeletorEscolher(p) => self.trocar_para(&p),
            _ => iced::Task::none(),
        }
    }

    /// **O quê:** valida `entrada` e, se for uma pasta, TROCA o projeto: reinicia o estado, grava
    /// a memória e pede a releitura.
    ///
    /// **Onde:** [`App::atualizar_seletor`]. **Caminho inválido NÃO troca:** o erro vai para o
    /// modal (que continua aberto para a pessoa corrigir) e nada é relido nem gravado. Escolher o
    /// projeto em que já está só fecha o modal e relê. Falhar ao LEMBRAR não impede a troca, mas
    /// a falha aparece na mensagem do topo.
    pub(super) fn trocar_para(&mut self, entrada: &str) -> iced::Task<Msg> {
        let dir = match validar(entrada) {
            Ok(d) => d,
            Err(e) => {
                self.seletor.erro = e;
                return iced::Task::none();
            }
        };
        if dir == self.dir {
            self.seletor = self.seletor.fechado();
            return self.recarregar(Escopo::TUDO);
        }
        self.trocar_projeto(dir.clone());
        if let Err(e) = gravar_ultimo(&self.dados, &dir) {
            self.msg = format!("não consegui lembrar o projeto escolhido: {e}");
        }
        self.recarregar(Escopo::TUDO)
    }

    /// **O quê:** o modal «Trocar projeto». **Onde:** [`super::view`], num `stack`; `None` = fechado.
    pub fn ver_seletor(&self) -> Option<Element<'_, Msg>> {
        if !self.seletor.aberto {
            return None;
        }
        let c = self.cores;
        let s = &self.seletor;
        let mut lista = column![].spacing(6).width(Fill);
        if s.carregando {
            lista = lista.push(txt("procurando projetos…", 12.0, c.dim));
        } else if s.lista.is_empty() {
            lista = lista.push(txt(
                "Nenhum projeto encontrado — digite um caminho abaixo.",
                12.0,
                c.dim,
            ));
        }
        for p in &s.lista {
            lista = lista.push(self.linha_de_projeto(p));
        }
        let campo = text_input("ou digite / cole o caminho de um projeto", &s.caminho)
            .on_input(Msg::SeletorCaminho)
            .on_submit(Msg::SeletorAbrirCaminho)
            .size(13)
            .padding(8)
            .style(tema::campo(c));
        let abrir = btn(
            c,
            "Abrir",
            (!s.caminho.trim().is_empty()).then_some(Msg::SeletorAbrirCaminho),
            true,
            false,
        );
        let corpo = column![
            negrito("Trocar projeto", 16.0, c.texto),
            txt(
                "Os fixados vêm primeiro. A janela abre neste projeto na próxima vez.",
                12.0,
                c.dim
            )
            .width(Fill),
            scrollable(lista.padding([0, 8])).height(Length::Fixed(280.0)),
            row![campo, abrir].spacing(8),
            status(c, &s.erro, true, 12.0),
            row![
                Space::new().width(Fill),
                btn(c, "Cancelar", Some(Msg::FecharSeletor), false, false)
            ],
        ]
        .spacing(12);
        Some(modal(c, corpo.into(), Msg::FecharSeletor, false))
    }

    /// **O quê:** a linha clicável de um projeto: nome, selo (fixado/atual) e caminho.
    /// **Onde:** [`App::ver_seletor`].
    fn linha_de_projeto<'a>(&'a self, p: &'a Project) -> Element<'a, Msg> {
        let c = self.cores;
        let mut titulo = row![negrito(&p.name, 13.0, c.texto)].spacing(8);
        match p.marker.as_str() {
            "pinned" => titulo = titulo.push(negrito("fixado", 10.5, c.accent)),
            "umbrella" => titulo = titulo.push(negrito("aplicação", 10.5, c.dim)),
            _ => {}
        }
        if Path::new(&p.path) == self.dir {
            titulo = titulo.push(negrito("atual", 10.5, c.bom));
        }
        let corpo = column![titulo, txt(&p.path, 11.0, c.dim).width(Fill)].spacing(2);
        button(corpo)
            .width(Fill)
            .padding(8)
            .on_press(Msg::SeletorEscolher(p.path.clone()))
            .style(tema::opcao(c, false))
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::super::msg::Aba;
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Uma pasta única DENTRO do workspace (`target/test-tmp`): o piso 12 veta `/tmp`.
    fn tmp(nome: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp").join(format!(
            "{nome}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).expect("criar tmp");
        p
    }

    fn app(dir: &Path, dados: &Path) -> App {
        App::novo(dir.to_path_buf(), dados.to_path_buf(), "v".to_string(), Aba::Perguntas, true)
    }

    // ---------- validar ----------

    #[test]
    fn validar_aceita_pasta_existente_e_devolve_o_caminho_canonico() {
        let d = tmp("val");
        let sub = d.join("a b");
        std::fs::create_dir_all(&sub).unwrap();
        let ok = validar_com_home(&format!("  {}  ", sub.display()), None).expect("é uma pasta");
        assert_eq!(ok, std::fs::canonicalize(&sub).unwrap(), "espaço em volta é ignorado");
    }

    #[test]
    fn validar_recusa_vazio_inexistente_arquivo_e_byte_nulo() {
        let d = tmp("inv");
        let arq = d.join("arquivo.txt");
        std::fs::write(&arq, "x").unwrap();
        for ruim in ["", "   ", "/nao/existe/mesmo/42", &arq.display().to_string(), "a\0b"] {
            let e = validar_com_home(ruim, None).expect_err(ruim);
            assert!(!e.is_empty(), "o erro precisa dizer o motivo: {ruim:?}");
        }
        assert!(validar_com_home("", None).unwrap_err().contains("digite"));
        assert!(validar_com_home(&arq.display().to_string(), None).unwrap_err().contains("pasta"));
    }

    #[test]
    fn validar_expande_til_para_a_home() {
        let home = tmp("home");
        std::fs::create_dir_all(home.join("proj")).unwrap();
        let h = Some(home.as_path());
        assert_eq!(
            validar_com_home("~/proj", h).unwrap(),
            std::fs::canonicalize(home.join("proj")).unwrap()
        );
        assert_eq!(validar_com_home("~", h).unwrap(), std::fs::canonicalize(&home).unwrap());
        assert!(validar_com_home("~/nao-existe", h).is_err());
    }

    // ---------- persistência ----------

    #[test]
    fn o_ultimo_projeto_grava_e_le_de_volta() {
        let dados = tmp("dados");
        let proj = tmp("proj");
        gravar_ultimo(&dados, &proj).expect("gravar");
        assert_eq!(ler_ultimo(&dados), Some(proj));
    }

    #[test]
    fn ultimo_projeto_ausente_ou_apagado_ou_arquivo_vazio_e_none() {
        let dados = tmp("dados");
        assert_eq!(ler_ultimo(&dados), None, "nunca gravado");
        let proj = tmp("some");
        gravar_ultimo(&dados, &proj).expect("gravar");
        std::fs::remove_dir_all(&proj).unwrap();
        assert_eq!(ler_ultimo(&dados), None, "a pasta lembrada sumiu: não abre um caminho morto");
        std::fs::write(dados.join(ARQUIVO_ULTIMO), "  \n").unwrap();
        assert_eq!(ler_ultimo(&dados), None, "arquivo vazio");
    }

    #[test]
    fn dir_inicial_o_arg_vence_o_persistido_que_vence_o_cwd() {
        let dados = tmp("dados");
        let (persistido, do_arg, cwd) = (tmp("persistido"), tmp("arg"), tmp("cwd"));
        gravar_ultimo(&dados, &persistido).unwrap();
        // --dir vence, mesmo com memória válida.
        assert_eq!(dir_inicial(Some(do_arg.clone()), &dados, Some(cwd.clone())), do_arg);
        // Sem --dir, a memória vence o cwd.
        assert_eq!(dir_inicial(None, &dados, Some(cwd.clone())), persistido);
        // Sem memória, o cwd; sem cwd, ".".
        let vazio = tmp("dados-vazio");
        assert_eq!(dir_inicial(None, &vazio, Some(cwd.clone())), cwd);
        assert_eq!(dir_inicial(None, &vazio, None), PathBuf::from("."));
    }

    #[test]
    fn dir_inicial_com_o_arg_nao_sobrescreve_a_memoria() {
        let dados = tmp("dados");
        let persistido = tmp("persistido");
        gravar_ultimo(&dados, &persistido).unwrap();
        let _ = dir_inicial(Some(tmp("arg")), &dados, None);
        assert_eq!(ler_ultimo(&dados), Some(persistido), "--dir não é uma escolha da janela");
    }

    // ---------- lista ----------

    #[test]
    fn a_lista_poe_os_fixados_primeiro_mesmo_sem_marcador() {
        let raiz = tmp("lista");
        for n in ["aaa", "mmm", "zzz"] {
            std::fs::create_dir_all(raiz.join(n).join(".git")).unwrap();
        }
        let fixado = tmp("fixado-sem-git");
        let l = listar(&[raiz.display().to_string()], &[fixado.display().to_string()], &raiz);
        assert_eq!(l[0].marker, "pinned", "{l:?}");
        assert_eq!(l[0].path, std::fs::canonicalize(&fixado).unwrap().to_string_lossy());
        // A pasta-guarda-chuva com 2+ projetos dentro aparece como "umbrella" (a aplicação
        // multi-repo, que o app seleciona para o grafo global); os repos seguem listados.
        assert!(l.iter().any(|p| p.marker == "umbrella"), "{l:?}");
        let nomes: Vec<&str> =
            l[1..].iter().filter(|p| p.marker != "umbrella").map(|p| p.name.as_str()).collect();
        assert_eq!(nomes, ["aaa", "mmm", "zzz"]);
    }

    #[test]
    fn sem_pastas_de_desenvolvimento_varre_o_pai_do_projeto_atual() {
        let raiz = tmp("guarda-chuva");
        let (a, b) = (raiz.join("um"), raiz.join("dois"));
        for d in [&a, &b] {
            std::fs::create_dir_all(d.join(".git")).unwrap();
        }
        let l = listar(&[], &[], &a);
        let nomes: Vec<&str> =
            l.iter().filter(|p| p.marker != "umbrella").map(|p| p.name.as_str()).collect();
        assert_eq!(nomes, ["dois", "um"], "os irmãos do projeto atual aparecem");
    }

    // ---------- o update ----------

    /// Escolher um projeto TROCA o `dir`, reinicia o estado do anterior, PEDE a releitura e
    /// grava a memória.
    #[test]
    fn escolher_um_projeto_troca_o_dir_e_recarrega() {
        let (dados, antigo, novo) = (tmp("dados"), tmp("antigo"), tmp("novo"));
        let mut a = app(&antigo, &dados);
        a.msg = "sobra do projeto antigo".into();
        a.itens.push(super::super::telas::Item {
            tipo: "open".into(),
            texto: "x".into(),
            maquina: true,
            sub: false,
            humano_n: 0,
        });
        a.seletor.aberto = true;
        let t = a.update(Msg::SeletorEscolher(novo.display().to_string()));
        assert_eq!(a.dir, std::fs::canonicalize(&novo).unwrap());
        assert!(a.itens.is_empty() && a.msg.is_empty(), "o estado do projeto antigo não vaza");
        assert!(!a.seletor.aberto && a.seletor.erro.is_empty());
        assert_eq!(t.units(), 1, "uma releitura completa foi pedida");
        assert_eq!(ler_ultimo(&dados), Some(std::fs::canonicalize(&novo).unwrap()));
    }

    /// Caminho inválido: erro na tela, o `dir` NÃO muda, nada é relido nem gravado.
    #[test]
    fn caminho_invalido_mostra_erro_e_nao_troca() {
        let (dados, atual) = (tmp("dados"), tmp("atual"));
        let mut a = app(&atual, &dados);
        a.seletor.aberto = true;
        let _ = a.update(Msg::SeletorCaminho("/nao/existe/mesmo/42".into()));
        let t = a.update(Msg::SeletorAbrirCaminho);
        assert_eq!(a.dir, atual, "o projeto não mudou");
        assert!(a.seletor.aberto, "o modal continua aberto para a pessoa corrigir");
        assert!(a.seletor.erro.contains("42"), "o erro cita o caminho: {}", a.seletor.erro);
        assert_eq!(t.units(), 0, "nenhuma releitura");
        assert_eq!(ler_ultimo(&dados), None, "nada foi lembrado");
        // Digitar de novo limpa o erro antigo.
        let _ = a.update(Msg::SeletorCaminho("/outro".into()));
        assert!(a.seletor.erro.is_empty());
    }

    /// O último projeto é lembrado e a PRÓXIMA abertura (sem `--dir`) o restaura.
    #[test]
    fn o_ultimo_projeto_e_lembrado_e_restaurado_na_proxima_abertura() {
        let (dados, atual, novo) = (tmp("dados"), tmp("atual"), tmp("novo"));
        let mut a = app(&atual, &dados);
        let _ = a.update(Msg::SeletorCaminho(novo.display().to_string()));
        let _ = a.update(Msg::SeletorAbrirCaminho);
        let proxima = dir_inicial(None, &dados, Some(atual.clone()));
        assert_eq!(proxima, std::fs::canonicalize(&novo).unwrap());
        // E com --dir, a memória fica de lado.
        assert_eq!(dir_inicial(Some(atual.clone()), &dados, None), atual);
    }

    /// Falhar ao LEMBRAR não impede de trocar — mas a falha aparece (não é engolida).
    #[test]
    fn falha_ao_gravar_a_memoria_aparece_mas_a_troca_acontece() {
        let (atual, novo) = (tmp("atual"), tmp("novo"));
        let dados_que_e_arquivo = tmp("x").join("arquivo");
        std::fs::write(&dados_que_e_arquivo, "não sou uma pasta").unwrap();
        let mut a = app(&atual, &dados_que_e_arquivo);
        let _ = a.update(Msg::SeletorEscolher(novo.display().to_string()));
        assert_eq!(a.dir, std::fs::canonicalize(&novo).unwrap());
        assert!(a.msg.contains("não consegui lembrar"), "{}", a.msg);
    }

    /// Abrir o seletor pede a lista (uma tarefa) e a lista que chega é guardada.
    #[test]
    fn abrir_o_seletor_pede_a_lista_e_guarda_o_resultado() {
        let (dados, atual) = (tmp("dados"), tmp("atual"));
        let mut a = app(&atual, &dados);
        let t = a.update(Msg::AbrirSeletor);
        assert!(a.seletor.aberto && a.seletor.carregando);
        assert_eq!(t.units(), 1);
        let p = Project { name: "p".into(), path: "/p".into(), marker: "pinned".into() };
        let _ = a.update(Msg::Projetos(vec![p.clone()]));
        assert!(!a.seletor.carregando);
        assert_eq!(a.seletor.lista, vec![p]);
        let _ = a.update(Msg::FecharSeletor);
        assert!(!a.seletor.aberto);
    }

    /// Escolher o projeto em que JÁ está não reinicia nada, só fecha e relê.
    #[test]
    fn escolher_o_mesmo_projeto_nao_reinicia_o_estado() {
        let (dados, atual) = (tmp("dados"), tmp("atual"));
        let mut a = app(&std::fs::canonicalize(&atual).unwrap(), &dados);
        a.objetivo = "meu objetivo".into();
        a.seletor.aberto = true;
        let t = a.update(Msg::SeletorEscolher(atual.display().to_string()));
        assert_eq!(a.objetivo, "meu objetivo");
        assert!(!a.seletor.aberto);
        assert_eq!(t.units(), 1);
    }
}
