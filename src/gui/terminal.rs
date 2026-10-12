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

/// **O quê:** um binário existe no `$PATH`? **Onde:** [`abrir`].
///
/// Via `command -v` num shell, e não varrendo o `$PATH` à mão: o shell já sabe de alias, de
/// função e do `PATH` efetivo, e esta janela é `std`-only — reimplementar isso seria mais
/// código para acertar menos.
fn existe(cmd: &str) -> bool {
    std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {cmd}"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Emuladores tentados, em ordem, com a flag que cada um usa para receber o comando.
///
/// **`x-terminal-emulator` primeiro** porque em Debian/Ubuntu ele é o alternativo escolhido
/// pela pessoa — respeitar essa escolha vale mais que adivinhar. O `kitty` não tem flag: ele
/// recebe o comando direto.
const TERMINAIS: &[(&str, &[&str])] = &[
    ("x-terminal-emulator", &["-e"]),
    ("konsole", &["-e"]),
    ("gnome-terminal", &["--"]),
    ("xfce4-terminal", &["-x"]),
    ("tilix", &["-e"]),
    ("kitty", &[]),
    ("alacritty", &["-e"]),
    ("xterm", &["-e"]),
];

/// **O quê:** abre `comando` no primeiro terminal disponível. `false` se não houver nenhum.
///
/// **Onde:** as ações que mudam estado.
///
/// **`false` NÃO é o fim do caminho**, e é por isso que ele é `bool` e não `panic`: quando não
/// há terminal, a janela entrega o comando para a pessoa rodar onde quiser. Sumir com o
/// problema é o que o §37.48 chama de bug do software — a janela se adapta ao que a máquina
/// tem, em vez de exigir que a pessoa saiba por que nada aconteceu.
pub fn abrir(comando: &str) -> bool {
    for (term, pre) in TERMINAIS {
        if existe(term)
            && std::process::Command::new(term)
                .args(*pre)
                .arg("bash")
                .arg("-c")
                .arg(comando)
                .spawn()
                .is_ok()
        {
            return true;
        }
    }
    false
}

/// **O quê:** solta `prog args…` desacoplado desta janela (stdio nulo) e diz o que fez. **Onde:**
/// [`abrir_url`], [`abrir_pasta`], [`abrir_vscode`], [`nova_janela`]. `Err` carrega o motivo e o
/// que fazer à mão.
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

/// **O quê:** abre um link no navegador (`xdg-open`). **Onde:** a ação de notícia do sino. O
/// chamador JÁ validou que é `https` ([`super::sino::url_segura`]).
pub fn abrir_url(url: &str) -> Result<String, String> {
    soltar(
        "xdg-open",
        &[url.to_string()],
        format!("aberto no navegador: {url}"),
        &format!("Abra à mão: {url}"),
    )
}

/// **O quê:** abre a pasta do projeto no gerenciador de arquivos. **Onde:** o ícone de pasta do topo.
pub fn abrir_pasta(dir: &Path) -> Result<String, String> {
    let d = dir.display().to_string();
    soltar(
        "xdg-open",
        std::slice::from_ref(&d),
        format!("pasta aberta: {d}"),
        &format!("Abra à mão: {d}"),
    )
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

/// **O quê:** qual programa abre `caminho` no editor do usuário, e com que argumentos. PURA.
///
/// **Onde:** [`abrir_arquivo`]. O VS Code (`code`) primeiro, porque é o que a aba do hub usava e
/// o que a pessoa que edita PLAN.md costuma ter; sem ele, o `xdg-open` entrega ao editor padrão
/// do desktop para `.md`. Separada do spawn para o teste dizer o que o clique roda.
pub fn plano_do_editor(caminho: &str, tem_code: bool) -> (&'static str, Vec<String>) {
    if tem_code {
        ("code", vec![caminho.to_string()])
    } else {
        ("xdg-open", vec![caminho.to_string()])
    }
}

/// **O quê:** abre um ARQUIVO no editor do usuário, desacoplado desta janela.
///
/// **Onde:** «Abrir no editor» da aba Notas. **A janela não edita arquivo (D6):** quem edita é o
/// editor da pessoa, e quem grava é ele. Erro devolve o CAMINHO — sem editor ainda dá para abrir
/// à mão (§37.48), e um "não consegui" seco seria um beco sem saída.
pub fn abrir_arquivo(caminho: &str) -> Result<String, String> {
    let (prog, args) = plano_do_editor(caminho, existe("code"));
    if !existe(prog) {
        return Err(format!("não achei `{prog}` para abrir. Abra à mão: {caminho}"));
    }
    std::process::Command::new(prog)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| format!("aberto no editor ({prog}): {caminho}"))
        .map_err(|e| format!("não consegui rodar `{prog}` ({e}). Abra à mão: {caminho}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lista de terminais está sã: nenhum nome vazio, nenhum duplicado, e o `kitty` — o
    /// único sem flag — continua sem flag. Um nome duplicado faria o segundo nunca ser
    /// tentado, e um vazio tentaria executar o programa "".
    #[test]
    fn a_lista_de_terminais_esta_sa() {
        let nomes: Vec<&str> = TERMINAIS.iter().map(|(n, _)| *n).collect();
        assert!(nomes.iter().all(|n| !n.is_empty()));
        let mut ordenados = nomes.clone();
        ordenados.sort_unstable();
        ordenados.dedup();
        assert_eq!(ordenados.len(), nomes.len(), "terminal duplicado nunca é tentado duas vezes");
        assert_eq!(nomes[0], "x-terminal-emulator", "a escolha da pessoa vem primeiro");
        let kitty = TERMINAIS.iter().find(|(n, _)| *n == "kitty").unwrap();
        assert!(kitty.1.is_empty(), "o kitty recebe o comando direto, sem flag");
    }

    /// **O clique em «Abrir no editor» roda isto:** `code <arquivo>` se houver, senão `xdg-open`.
    /// O caminho vai como UM argumento (espaço no nome não o parte) e nunca por shell.
    #[test]
    fn o_editor_recebe_o_caminho_como_um_argumento() {
        let c = "/p/meu projeto/.schematize/overdev/PLAN.md";
        assert_eq!(plano_do_editor(c, true), ("code", vec![c.to_string()]));
        assert_eq!(plano_do_editor(c, false), ("xdg-open", vec![c.to_string()]));
    }

    /// `existe` responde `false` para o que não existe, sem entrar em pânico e sem imprimir
    /// nada na saída desta janela.
    #[test]
    fn binario_inexistente_e_false_e_nao_panico() {
        assert!(!existe("terminal-que-nao-existe-em-lugar-nenhum-42"));
        assert!(existe("sh"), "`sh` existe em qualquer unix");
    }
}
