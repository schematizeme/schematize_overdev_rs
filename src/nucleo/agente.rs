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

use schematize_plataforma::seams::terminal;

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
    // O terminal roda `bash run.sh` NON-LOGIN: ele não lê o `~/.bashrc`. O corpo do seam
    // (`terminal::corpo_agente`) reforça o PATH e chama o claude pelo caminho ABSOLUTO — sem isso
    // o comando morre com "not found" num app que está instalado, e o erro chega como uma linha
    // crua de bash.
    let st = terminal::script_arquivo(
        script,
        terminal::corpo_agente(projeto, &claude, &pf),
        terminal::Emuladores::Agente,
    );
    super::terminal::abrir_nomeado(&st)
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

    /// A lista de terminais do agente cobre os dois separadores (a lista mora no seam
    /// `Terminal`): errar o separador não dá erro — o terminal abre VAZIO.
    #[test]
    fn a_lista_de_terminais_cobre_os_dois_separadores() {
        let nomes = terminal::nomes(terminal::Emuladores::Agente);
        assert!(nomes.contains(&"gnome-terminal"), "o do GNOME usa `--`");
        assert!(nomes.contains(&"konsole"), "o do KDE usa `-e`");
        assert!(nomes.contains(&"xterm"), "o último recurso tem de estar na lista");
    }
}
