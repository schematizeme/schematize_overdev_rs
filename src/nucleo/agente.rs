//! Abrir um AGENTE num terminal — PLATAFORMA, e nada de domínio.
//!
//! **O quê:** grava o prompt num arquivo, escreve um wrapper de shell e abre um terminal que
//! roda o `claude` na raiz do projeto.
//!
//! **Onde:** `schematize-overdev rerun`, que reaplica uma skill sobre um projeto.
//!
//! ## Por que TERMINAL, e não um processo capturado
//!
//! O agente é interativo e longo: ele pergunta, mostra progresso, e a pessoa pode interromper.
//! Capturar a saída daria uma tela que congela por minutos e um `Ctrl-C` que não chega a
//! ninguém. No terminal, tudo isso já funciona — e o que roda está à vista antes de rodar.
//!
//! ## O que NÃO veio junto do hub, e é o ponto do corte
//!
//! O lançador do hub também purga lançamentos velhos, pré-confia a pasta e grava em
//! `.schematize/overdev/`. Isso é maquinaria de OVERDEV; trazê-la faria este app depender do
//! domínio de onde ele acabou de sair. Aqui o prompt mora em `.schematize/skills/`, e o resto
//! fica com quem é dono.

use std::path::{Path, PathBuf};

/// Os terminais que a casa procura, na ordem em que os ambientes de desktop os trazem.
///
/// **A ordem é a da probabilidade**, e não a do gosto: achar um terminal instalado e não o
/// preferido é melhor que não achar nenhum.
const TERMINAIS: [&str; 7] =
    ["konsole", "gnome-terminal", "xfce4-terminal", "alacritty", "kitty", "wezterm", "xterm"];

/// **O quê:** abre o `claude` num terminal, na raiz de `projeto`, com `prompt`.
///
/// **Onde:** o `rerun`.
///
/// **Devolve o NOME do terminal aberto** para a mensagem dizer onde olhar: "abri no konsole" é
/// acionável; "abri um terminal" manda a pessoa procurar entre as janelas.
pub fn abrir(projeto: &Path, prompt: &str) -> Result<String, String> {
    let claude = super::bin::resolve_bin("claude").ok_or_else(|| {
        "o CLI `claude` não está no PATH — instale o Claude Code (claude.ai/code).".to_string()
    })?;
    let dir = super::caminhos::schematize_dir_at(projeto).join("skills");
    std::fs::create_dir_all(&dir).map_err(|e| format!("criar {}: {e}", dir.display()))?;
    let (pf, script) = arquivos(&dir);
    std::fs::write(&pf, prompt).map_err(|e| format!("gravar prompt: {e}"))?;
    // O terminal roda `bash run.sh` NON-LOGIN: ele não lê o `~/.bashrc`. Sem reforçar o PATH e
    // sem chamar o claude pelo caminho ABSOLUTO, o comando morre com "not found" num app que
    // está instalado — e o erro chega como uma linha crua de bash.
    let sh = format!(
        "#!/usr/bin/env bash\n\
         export PATH=\"$HOME/.local/bin:$HOME/.cargo/bin:$HOME/.claude/local:$PATH\"\n\
         cd {proj:?} || exit 1\n\
         {claude:?} --dangerously-skip-permissions \"$(cat {pf:?})\"\n\
         echo\n\
         read -rp '[agente encerrado — Enter para fechar] '\n",
        proj = projeto,
        claude = claude,
        pf = pf,
    );
    std::fs::write(&script, &sh).map_err(|e| format!("gravar wrapper: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755));
    }
    spawn(&script)
}

/// **O quê:** os dois arquivos de um lançamento, com nome ÚNICO.
///
/// **Onde:** [`abrir`].
///
/// **Único por lançamento** porque dois `rerun` seguidos em projetos diferentes usariam o
/// mesmo nome: o segundo sobrescreveria o prompt do primeiro, e o terminal que já estava
/// aberto leria o prompt errado no `cat`.
fn arquivos(dir: &Path) -> (PathBuf, PathBuf) {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let id = format!("{}-{n}", std::process::id());
    (dir.join(format!("prompt-{id}.txt")), dir.join(format!("run-{id}.sh")))
}

/// **O quê:** abre o terminal rodando `script`. Devolve o nome do terminal.
///
/// **`process_group(0)` desacopla do app:** sem isso, fechar a janela que abriu o terminal
/// mataria o agente no meio do trabalho.
fn spawn(script: &Path) -> Result<String, String> {
    let term = TERMINAIS
        .iter()
        .find(|t| super::bin::resolve_bin(t).is_some())
        .ok_or_else(|| format!("nenhum terminal encontrado ({}).", TERMINAIS.join("/")))?;
    let mut cmd = std::process::Command::new(term);
    match *term {
        // Estes usam `--` para separar o comando a executar; os outros usam `-e`. Errar o
        // separador não dá erro: o terminal abre VAZIO, e a pessoa conclui que o app travou.
        "gnome-terminal" | "xfce4-terminal" => {
            cmd.arg("--").arg("bash").arg(script);
        }
        _ => {
            cmd.arg("-e").arg("bash").arg(script);
        }
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn().map_err(|e| format!("abrir terminal `{term}`: {e}"))?;
    Ok((*term).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Cada lançamento tem arquivos PRÓPRIOS.** Dois `rerun` seguidos com o mesmo nome fariam
    /// o segundo sobrescrever o prompt do primeiro — e o terminal já aberto leria o errado.
    #[test]
    fn dois_lancamentos_nao_disputam_o_mesmo_arquivo() {
        let d = std::path::Path::new("/tmp");
        let (p1, s1) = arquivos(d);
        let (p2, s2) = arquivos(d);
        assert_ne!(p1, p2);
        assert_ne!(s1, s2);
        assert!(p1.to_string_lossy().contains("prompt-"));
        assert!(s1.to_string_lossy().ends_with(".sh"));
    }

    /// O separador certo por terminal — errar não dá erro: o terminal abre VAZIO, e a pessoa
    /// conclui que o app travou.
    #[test]
    fn a_lista_de_terminais_cobre_os_dois_separadores() {
        assert!(TERMINAIS.contains(&"gnome-terminal"), "o do GNOME usa `--`");
        assert!(TERMINAIS.contains(&"konsole"), "o do KDE usa `-e`");
        assert!(TERMINAIS.contains(&"xterm"), "o último recurso tem de estar na lista");
    }
}
