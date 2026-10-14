//! Integração com o ambiente gráfico: ícone e entrada no menu de aplicativos.
//!
//! **O quê:** descreve o atalho do app (`EntradaAtalho`) e o entrega ao seam `Atalhos` da
//! plataforma, que grava o `.desktop` e os ícones (o app só DESENHA o ícone), para o app
//! **aparecer na lista de programas** e abrir sozinho — sem passar pelo hub.
//!
//! **Onde:** `schematize-overdev desktop --install` / `--remover`, e o `schematize-market`
//! depois de instalar o app.
//!
//! ## Por que o app instala a PRÓPRIA integração
//!
//! Se quem gravasse o `.desktop` deste app fosse o instalador do hub, ele deixaria de ser
//! instalável sozinho — e essa autonomia é a razão do ADR-0012 F4. Quem baixa o binário do
//! release, ou compila do fonte, tem de conseguir o ícone também.
//!
//! ## O ícone tem DUAS formas, e escolhe a melhor disponível
//!
//! **Com a janela instalada** (`schematize-overdev-gui`, que sai do mesmo repo pelo
//! ADR-0020): `Exec=` aponta para ela e `Terminal=false`.
//!
//! **Sem a janela:** cai em `status` com `Terminal=true`. Um `.desktop` com
//! `Terminal=false` apontando para uma CLI abriria um processo sem janela que termina no mesmo
//! instante — o clique não faria **nada**, que é pior que não ter ícone. §37.48: o software se
//! adapta ao clique que a pessoa deu.
//!
//! **Por que a escolha é feita ao GRAVAR, e não ao clicar.** Um `Exec=` que decidisse na hora
//! precisaria de `Terminal=` fixo, e nenhum dos dois valores serve aos dois caminhos: com
//! `false` o fallback não teria terminal; com `true` a janela abriria com um terminal preto
//! pendurado atrás.

use std::path::{Path, PathBuf};

use schematize_plataforma::seams::atalhos_render::resolver_gui_irma;
use schematize_plataforma::seams::EntradaAtalho;

/// Nome do arquivo `.desktop`. Um lugar só: se divergir do ícone, o menu mostra um quadrado
/// cinza e ninguém liga a causa ao nome.
pub const ID: &str = "schematize-overdev";

/// Nome do executável da janela — o segundo `[[bin]]` deste repo (ADR-0020).
pub const GUI_BIN: &str = "schematize-overdev-gui";

/// **O quê:** o caminho ABSOLUTO da janela, ou `None` se ela não estiver instalada.
///
/// **Onde:** [`instalar`], para decidir entre as duas formas do `.desktop`.
///
/// **Ao lado do próprio binário PRIMEIRO:** é onde o instalador põe o par, e é o par que se
/// atualiza junto (regra do seam, `resolver_gui_irma`). Só depois `~/.cargo/bin` e o `$PATH`,
/// que podem ter cópia velha.
pub fn resolver_gui(bin_proprio: &Path) -> Option<PathBuf> {
    resolver_gui_irma(bin_proprio, GUI_BIN).or_else(|| super::bin::resolve_bin(GUI_BIN))
}

/// **O quê:** a entrada de atalho do Overdev: textos, categorias, `bin`/`gui` e os `icones`
/// (PNG por tamanho) dados.
///
/// **Onde:** [`instalar_com_gui`], [`remover`] e os testes de render. Os valores são os que o
/// `.desktop` do app sempre teve (travados por `tests/snapshot_f4b.rs`).
///
/// **`Exec` ABSOLUTO nas duas formas** (a janela, ou a CLI com `status` em terminal), pela razão
/// do módulo `bin`; **o ícone é obrigatório** (falha ao gravá-lo é erro, não aviso).
pub fn entrada(
    home: &Path,
    bin: &Path,
    gui: Option<&Path>,
    icones: Vec<(u32, Vec<u8>)>,
) -> EntradaAtalho {
    let lista = |t: &[&str]| t.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    EntradaAtalho {
        home: home.to_path_buf(),
        id: ID.into(),
        nome: "schematize Overdev".into(),
        generico: Some("Run, checklist, perguntas".into()),
        comentario: Some("O progresso do run, o checklist e as perguntas que esperam você".into()),
        bin: Some(bin.to_path_buf()),
        // A CLI recebe `status`, que IMPRIME a resposta que esta tela existe para dar — em que pé
        // está o run. O binário puro imprimiria o help do clap e sairia.
        args_sem_gui: "status".into(),
        gui: gui.map(Path::to_path_buf),
        categorias: lista(&["Development", "Utility"]),
        palavras_chave: lista(&[
            "skill", "skills", "catalogo", "catalog", "claude", "agente", "agent", "fork",
        ]),
        startup_notify: Some(false),
        startup_wm_class: Some(GUI_BIN.into()),
        icone_nome: super::icone::NOME.into(),
        icones,
        icone_extras: true,
        icone_obrigatorio: true,
        cache_icones: false,
        legados: vec![],
    }
}

/// **O quê:** onde os `.desktop` do usuário moram (layout freedesktop, para exibir ao usuário).
pub fn dir_apps(home: &Path) -> PathBuf {
    home.join(".local/share/applications")
}

/// **O quê:** o caminho do `.desktop` deste app (para exibir e checar se existe).
pub fn arquivo_desktop(home: &Path) -> PathBuf {
    dir_apps(home).join(format!("{ID}.desktop"))
}

/// **O quê:** instala ícone + `.desktop`, e devolve o caminho do `.desktop`.
///
/// **Onde:** `schematize-overdev desktop --install`.
///
/// **`bin` é o caminho do próprio executável**, resolvido pelo chamador — gravar um caminho
/// adivinhado faria o ícone abrir outra coisa (ou nada) na máquina de quem instalou fora do
/// lugar padrão.
pub fn instalar(home: &Path, bin: &Path) -> Result<PathBuf, String> {
    instalar_com_gui(home, bin, resolver_gui(bin).as_deref())
}

/// **O quê:** o mesmo que [`instalar`], com a janela DADA em vez de procurada.
///
/// **Onde:** [`instalar`] em produção, e os testes — que precisam das duas formas sem depender
/// do que está instalado na máquina de quem roda a suíte. Teste que muda de resultado com o
/// ambiente não prova nem uma coisa nem outra.
pub fn instalar_com_gui(home: &Path, bin: &Path, gui: Option<&Path>) -> Result<PathBuf, String> {
    // O ícone primeiro (o seam grava os PNGs ANTES do `.desktop`): um `.desktop` apontando para
    // ícone inexistente vira quadrado cinza, e a pessoa conclui que o app não instalou direito.
    let icones =
        super::icone::icones_hicolor().map_err(|e| format!("não consegui gerar o ícone: {e}"))?;
    crate::ambiente::plat().atalhos.instalar(&entrada(home, bin, gui, icones))?;
    Ok(arquivo_desktop(home))
}

/// **O quê:** remove o atalho deste app. `true` se havia um.
///
/// **Onde:** `schematize-overdev desktop --remover`. Os ícones ficam: são inertes, e apagá-los
/// mexeria numa árvore (`hicolor`) compartilhada com outros apps.
pub fn remover(home: &Path) -> Result<bool, String> {
    let havia = arquivo_desktop(home).exists();
    if !havia {
        return Ok(false);
    }
    // O `bin` só valida a entrada; nada é executado na remoção.
    crate::ambiente::plat().atalhos.remover(&entrada(home, Path::new(ID), None, Vec::new()))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use schematize_plataforma::seams::atalhos_render::render as render_seam;

    /// O texto do `.desktop` que o seam geraria para estes caminhos (sem tocar o disco).
    fn render(bin: &Path, icone: &Path, gui: Option<&Path>) -> String {
        let e = entrada(Path::new("/h"), bin, gui, Vec::new());
        render_seam(&e, &icone.display().to_string()).expect("render")
    }

    /// A linha `Exec=` de um render, sem o prefixo.
    fn exec_de(t: &str) -> String {
        t.lines().find(|l| l.starts_with("Exec=")).expect("sem Exec= a entrada é ignorada")[5..]
            .to_string()
    }

    /// **O `Exec` tem de ser ABSOLUTO — nas DUAS formas.** O lançador do desktop dá PATH
    /// mínimo, então `Exec=schematize-overdev` não acharia o binário em `~/.cargo/bin`.
    #[test]
    fn o_exec_e_absoluto_com_e_sem_janela() {
        let bin = Path::new("/home/u/.cargo/bin/schematize-overdev");
        let gui = Path::new("/home/u/.cargo/bin/schematize-overdev-gui");

        let sem = exec_de(&render(bin, Path::new("/i/x.png"), None));
        assert!(sem.starts_with("/home/u/.cargo/bin/schematize-overdev "), "{sem}");

        let com = exec_de(&render(bin, Path::new("/i/x.png"), Some(gui)));
        assert_eq!(com, "/home/u/.cargo/bin/schematize-overdev-gui");
        assert!(!com.starts_with("schematize-overdev-gui"), "relativo depende do PATH do DE");
    }

    /// **A JANELA é o caminho normal**, e ela abre SEM terminal atrás.
    #[test]
    fn com_a_janela_instalada_o_icone_abre_a_janela() {
        let t = render(
            Path::new("/b/schematize-overdev"),
            Path::new("/i/x.png"),
            Some(Path::new("/b/schematize-overdev-gui")),
        );
        assert!(t.contains("Terminal=false"), "janela com Terminal=true abre terminal atrás");
        let exec = exec_de(&t);
        assert!(exec.ends_with("schematize-overdev-gui"), "{exec}");
        assert!(!exec.contains(" status"), "a janela não recebe subcomando: {exec}");
    }

    /// **Sem a janela o ícone NÃO fica quebrado**: cai em terminal com algo que IMPRIME.
    ///
    /// `Terminal=false` numa CLI abriria um processo sem janela que morre no mesmo instante —
    /// o clique não faria nada, e nada explicaria por quê (§37.48).
    #[test]
    fn sem_a_janela_o_icone_ainda_faz_alguma_coisa() {
        let t = render(Path::new("/b/schematize-overdev"), Path::new("/i/x.png"), None);
        assert!(t.contains("Terminal=true"), "CLI com Terminal=false é um clique que não faz nada");
        assert!(exec_de(&t).contains("status"), "o clique tem de IMPRIMIR algo");
    }

    /// **O `Icon=`, o `ID` e o nome do PNG são o mesmo nome.** Se divergirem, o menu mostra um
    /// quadrado cinza — e ninguém liga a causa ao nome, porque nada dá erro.
    #[test]
    fn o_icone_do_desktop_e_o_png_que_o_app_grava() {
        assert_eq!(ID, super::super::icone::NOME);
        let t = render(Path::new("/b/x"), Path::new("/i/schematize-overdev.png"), None);
        let icon = t.lines().find(|l| l.starts_with("Icon=")).expect("sem Icon=");
        assert!(icon.ends_with(&format!("{}.png", super::super::icone::NOME)), "{icon}");
    }

    /// **`StartupWMClass` é o nome da JANELA, e não o do CLI.** No Wayland o compositor casa a
    /// janela ao `.desktop` por essa classe para achar o ícone; errada, o dock mostra um "W".
    #[test]
    fn a_classe_de_janela_e_a_da_janela() {
        let t = render(Path::new("/b/x"), Path::new("/i/y.png"), None);
        assert!(t.contains(&format!("StartupWMClass={GUI_BIN}")), "{t}");
        assert!(!t.contains("StartupWMClass=schematize-overdev\n"), "é a classe do CLI: {t}");
    }

    /// Gravar e remover, de verdade, num HOME temporário.
    #[test]
    fn instala_e_remove_no_disco() {
        let home = std::env::temp_dir().join(format!("db-desk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let bin = home.join(".cargo/bin/schematize-overdev");
        let p = instalar_com_gui(&home, &bin, None).expect("instala");
        assert!(p.is_file(), "{}", p.display());
        assert!(
            home.join(".local/share/icons/hicolor/256x256/apps/schematize-overdev.png").is_file(),
            "o ícone tem de existir ANTES do .desktop apontar para ele"
        );
        assert!(remover(&home).expect("remove"), "havia um .desktop");
        assert!(!remover(&home).expect("remove de novo"), "o segundo remove não acha nada");
        let _ = std::fs::remove_dir_all(&home);
    }
}
