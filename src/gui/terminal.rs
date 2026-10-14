//! Abrir um TERMINAL do sistema — a parte de plataforma, e nada de domínio.
//!
//! **O quê:** roda um comando de shell num emulador de terminal gráfico, desacoplado desta
//! janela.
//!
//! **Onde:** a única ação desta janela que muda estado — restaurar um snapshot do histórico.
//!
//! ## Por que terminal, e não um botão que age dentro da janela (D6)
//!
//! Restaurar REGRAVA arquivos do projeto sobre o que estiver lá. No terminal o comando aparece
//! ANTES de rodar, o resultado é visível e o `Ctrl-C` funciona — numa janela Slint, nada disso.
//! A mesma decisão que o hub tomou, e que as janelas irmãs mantêm.
//!
//! ## Por que esta cópia existe, e o que a torna legítima
//!
//! É PLATAFORMA (D4): a cópia não sabe o que é overdev, run ou checklist. O que ela sabe do
//! domínio entra por parâmetro, como string já montada pelo chamador.

use std::path::Path;

use schematize_plataforma::seams::terminal;

/// **O quê:** um binário existe no `$PATH`? **Onde:** [`abrir_vscode`] e [`abrir_arquivo`].
///
/// Pelo seam `Shell` da plataforma (que sonda o `PATH` do processo): a janela é `std`-only e
/// reimplementar isso seria mais código para acertar menos.
fn existe(cmd: &str) -> bool {
    crate::ambiente::plat().shell.existe(cmd)
}

/// **O quê:** abre `comando` no primeiro terminal disponível (seam `Terminal`, perfil `Gui`:
/// `x-terminal-emulator` primeiro — em Debian/Ubuntu é o alternativo escolhido pela pessoa —,
/// flag por emulador e `bash -c`). `false` se não houver nenhum.
///
/// **Onde:** as ações que mudam estado.
///
/// **`false` NÃO é o fim do caminho**, e é por isso que ele é `bool` e não `panic`: quando não
/// há terminal, a janela entrega o comando para a pessoa rodar onde quiser. Sumir com o
/// problema é o que o §37.48 chama de bug do software — a janela se adapta ao que a máquina
/// tem, em vez de exigir que a pessoa saiba por que nada aconteceu.
pub fn abrir(comando: &str) -> bool {
    crate::ambiente::plat().terminal.abrir_script(&terminal::script_gui(comando)).is_ok()
}

/// **O quê:** solta `prog args…` desacoplado desta janela (stdio nulo) e diz o que fez. **Onde:**
/// [`abrir_vscode`]. `Err` carrega o motivo e o que fazer à mão.
fn soltar(prog: &str, args: &[String], feito: String, a_mao: &str) -> Result<String, String> {
    if !existe(prog) {
        return Err(format!("não achei `{prog}` para abrir. {a_mao}"));
    }
    let mut cmd = std::process::Command::new(prog);
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    cmd.spawn().map(|_| feito).map_err(|e| format!("não consegui rodar `{prog}` ({e}). {a_mao}"))
}

/// **O quê:** abre um link no navegador (seam `Abridor`). **Onde:** a ação de notícia do sino. O
/// chamador JÁ validou que é `https` ([`super::sino::url_segura`]).
pub fn abrir_url(url: &str) -> Result<String, String> {
    crate::ambiente::plat().abridor.abrir(url).map(|()| format!("aberto no navegador: {url}"))
}

/// **O quê:** abre a pasta do projeto no gerenciador de arquivos (seam `Abridor`). **Onde:** o
/// ícone de pasta do topo.
pub fn abrir_pasta(dir: &Path) -> Result<String, String> {
    let d = dir.display().to_string();
    crate::ambiente::plat().abridor.abrir(&d).map(|()| format!("pasta aberta: {d}"))
}

/// **O quê:** abre o projeto no VS Code (`code <pasta>`). **Onde:** o ícone `</>` do topo. Sem o
/// `code` o erro diz isso — não cai em outro editor por conta própria.
pub fn abrir_vscode(dir: &Path) -> Result<String, String> {
    let d = dir.display().to_string();
    soltar(
        "code",
        std::slice::from_ref(&d),
        format!("aberto no VS Code: {d}"),
        &format!("Abra à mão: {d}"),
    )
}

/// **O quê:** abre ESTA janela noutro processo, no mesmo projeto (`--dir`). **Onde:** o ícone de
/// nova janela: overdev de projetos em paralelo, sem estado compartilhado.
pub fn nova_janela(dir: &Path) -> Result<String, String> {
    let exe =
        std::env::current_exe().map_err(|e| format!("não sei qual é o meu executável: {e}"))?;
    let d = dir.display().to_string();
    let mut cmd = std::process::Command::new(&exe);
    cmd.arg("--dir")
        .arg(&d)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    cmd.spawn()
        .map(|_| format!("nova janela aberta em {d}"))
        .map_err(|e| format!("não consegui abrir a nova janela ({}): {e}", exe.display()))
}

/// **O quê:** quem abre um arquivo no editor do usuário: o VS Code ou o aplicativo padrão.
///
/// **Onde:** [`plano_do_editor`] e [`abrir_arquivo`].
#[derive(Debug, PartialEq, Eq)]
pub enum Editor {
    /// `code <arquivo>`.
    Code(String),
    /// O aplicativo padrão do desktop para o tipo (seam `Abridor`).
    Padrao(String),
}

/// **O quê:** qual editor abre `caminho`, decidido de forma PURA.
///
/// **Onde:** [`abrir_arquivo`]. O VS Code (`code`) primeiro, porque é o que a aba do hub usava e
/// o que a pessoa que edita PLAN.md costuma ter; sem ele, o `Abridor` entrega ao editor padrão
/// do desktop para `.md`. Separada do spawn para o teste dizer o que o clique roda.
pub fn plano_do_editor(caminho: &str, tem_code: bool) -> Editor {
    if tem_code {
        Editor::Code(caminho.to_string())
    } else {
        Editor::Padrao(caminho.to_string())
    }
}

/// **O quê:** abre um ARQUIVO no editor do usuário, desacoplado desta janela.
///
/// **Onde:** «Abrir no editor» da aba Notas. **A janela não edita arquivo (D6):** quem edita é o
/// editor da pessoa, e quem grava é ele. Erro devolve o CAMINHO — sem editor ainda dá para abrir
/// à mão (§37.48), e um "não consegui" seco seria um beco sem saída.
pub fn abrir_arquivo(caminho: &str) -> Result<String, String> {
    match plano_do_editor(caminho, existe("code")) {
        Editor::Code(c) => soltar(
            "code",
            std::slice::from_ref(&c),
            format!("aberto no editor (code): {c}"),
            &format!("Abra à mão: {c}"),
        ),
        Editor::Padrao(c) => crate::ambiente::plat()
            .abridor
            .abrir(&c)
            .map(|()| format!("aberto no editor (xdg-open): {c}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lista de terminais da GUI (no seam) está sã: nenhum nome vazio, nenhum duplicado, a
    /// escolha da pessoa (`x-terminal-emulator`) vem primeiro.
    #[test]
    fn a_lista_de_terminais_esta_sa() {
        let nomes = terminal::nomes(terminal::Emuladores::Gui);
        assert!(nomes.iter().all(|n| !n.is_empty()));
        let mut ordenados = nomes.clone();
        ordenados.sort_unstable();
        ordenados.dedup();
        assert_eq!(ordenados.len(), nomes.len(), "terminal duplicado nunca é tentado duas vezes");
        assert_eq!(nomes[0], "x-terminal-emulator", "a escolha da pessoa vem primeiro");
    }

    /// O `kitty` — o único sem flag — recebe o comando direto: o argv do perfil `Gui` é
    /// `bash -c <cmd>` sem `-e`; os outros levam a flag deles.
    #[test]
    fn o_kitty_recebe_o_comando_sem_flag() {
        let e = terminal::Entrega::Inline("ls".into());
        let kitty = terminal::argv(terminal::Emuladores::Gui, "kitty", &e);
        assert_eq!(kitty, ["bash", "-c", "ls"]);
        let konsole = terminal::argv(terminal::Emuladores::Gui, "konsole", &e);
        assert_eq!(konsole, ["-e", "bash", "-c", "ls"]);
    }

    /// **O clique em «Abrir no editor» roda isto:** `code <arquivo>` se houver, senão o
    /// aplicativo padrão. O caminho vai como UM argumento (espaço no nome não o parte) e nunca
    /// por shell.
    #[test]
    fn o_editor_recebe_o_caminho_como_um_argumento() {
        let c = "/p/meu projeto/.schematize/overdev/PLAN.md";
        assert_eq!(plano_do_editor(c, true), Editor::Code(c.to_string()));
        assert_eq!(plano_do_editor(c, false), Editor::Padrao(c.to_string()));
    }

    /// `existe` responde `false` para o que não existe, sem entrar em pânico e sem imprimir
    /// nada na saída desta janela.
    #[test]
    fn binario_inexistente_e_false_e_nao_panico() {
        assert!(!existe("terminal-que-nao-existe-em-lugar-nenhum-42"));
        assert!(existe("sh"), "`sh` existe em qualquer unix");
    }
}
