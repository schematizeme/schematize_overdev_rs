//! A CASCA — falar com o binário `schematize-overdev`, e nada de domínio.
//!
//! **O quê:** resolve onde o binário headless está e roda um subcomando NO PROJETO pedido,
//! lendo o `--json`.
//!
//! **Onde:** as telas, que só desenham o que sai daqui.
//!
//! ## Isto é PLATAFORMA, e é o que torna a cópia legítima (D4)
//!
//! Irmã do `cli.rs` da janela do skills. Nada aqui sabe o que é run, fila ou checklist: o que
//! a casca sabe do app entra por `--json`, e sai como texto. **Se um dia algo de domínio migrar
//! para cá, o corte foi feito errado.**
//!
//! ## Por que a janela NÃO depende do crate `overdev_casa` (a não ser pela procedência)
//!
//! Depender dele a faria embutir a versão com que foi compilada. Falando só com o binário, ela
//! sempre conversa com o que está instalado.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// **O quê:** o nome do binário headless nesta plataforma.
///
/// **Onde:** [`bin`]. Constante e não literal espalhado: nome que não bate com o dono mata o
/// update em silêncio — o comando simplesmente não é encontrado.
fn nome() -> &'static str {
    if cfg!(windows) {
        "schematize-overdev.exe"
    } else {
        "schematize-overdev"
    }
}

/// **O quê:** o caminho do binário headless: ao lado deste executável → `~/.cargo/bin` → PATH.
///
/// **Onde:** [`rodar`] e [`comando_para_terminal`].
///
/// **O "ao lado" vem primeiro** porque é onde o instalador põe o par, e é o par que se atualiza
/// junto. O lançador do desktop dá PATH mínimo, sem `~/.cargo/bin`: sem esta cadeia, a janela
/// aberta pelo ícone não acharia o binário instalado.
pub fn bin() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let c = dir.join(nome());
            if c.is_file() {
                return c;
            }
        }
    }
    if let Some(home) = home() {
        let c = home.join(".cargo").join("bin").join(nome());
        if c.is_file() {
            return c;
        }
    }
    PathBuf::from(nome())
}

/// **O quê:** o diretório home, na variável que cada plataforma usa.
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

/// **O quê:** roda `<bin> <args…>` COM `dir` como diretório corrente — EXATAMENTE os args
/// recebidos — e devolve o stdout, ou POR QUE não deu.
///
/// **Onde:** toda leitura e toda resposta da janela.
///
/// **O `dir` é o projeto, e é quem o CLI usa** (ele lê `.schematize/overdev` do cwd). A casca
/// NÃO acrescenta flag nenhuma: casca que adivinha o comando decide domínio.
///
/// **Devolve `Result`, nunca string vazia:** "não consegui executar" e "executei e falhou"
/// precisam chegar à tela como MOTIVO. Vazio viraria tela em branco — a janela afirmando que não
/// há nada quando o que houve foi não ter conseguido perguntar.
pub fn rodar<S: AsRef<str>>(dir: &Path, args: &[S]) -> Result<String, String> {
    let b = bin();
    let out = Command::new(&b)
        .args(args.iter().map(AsRef::as_ref))
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("não consegui executar {}: {e}", b.display()))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = err.trim();
        return Err(if err.is_empty() {
            let a: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
            format!("{} {} falhou ({})", b.display(), a.join(" "), out.status)
        } else {
            err.to_string()
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// **O quê:** aspa um argumento para o shell (aspas simples, com `'` escapado).
///
/// **Onde:** [`comando_para_terminal`]. O diretório do projeto pode ter espaço e `'`; sem aspas
/// o comando que a pessoa vê (e roda) seria outro.
fn aspa(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// **O quê:** a linha `'<bin>' arg arg…` — o comando que roda, sem o `cd` nem o eco.
///
/// **Onde:** [`comando_para_terminal`] e o guardrail de «Executar», que MOSTRA esta linha antes
/// de abrir o terminal. Uma função só garante que o mostrado é o executado.
pub fn linha_de_comando(bin: &str, args: &[&str]) -> String {
    format!("{} {}", aspa(bin), args.join(" "))
}

/// **O quê:** monta o comando de shell que roda um subcomando NO TERMINAL, dentro do projeto,
/// com o eco no topo.
///
/// **Onde:** a ação que muda estado (D6).
///
/// **O `bin` tem de ser ABSOLUTO** (PATH mínimo do lançador, herdado pelo terminal). **O eco
/// mostra o MESMO comando que roda**, e o `read` no fim segura o terminal depois que ele
/// termina — sem ele o erro some junto com a janela.
pub fn comando_para_terminal(bin: &str, dir: &Path, args: &[&str]) -> String {
    let linha = linha_de_comando(bin, args);
    let d = aspa(&dir.display().to_string());
    format!(
        "cd {d} || exit 1; echo {}; echo; {linha}; echo; read -n1 -s -r -p '…'",
        aspa(&format!("── {linha} ({}) ──", dir.display()))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O nome do binário conhece a plataforma, e é o do app CERTO.
    #[test]
    fn o_nome_do_binario_conhece_a_plataforma_e_o_dono() {
        let n = nome();
        assert!(n.starts_with("schematize-overdev"), "{n}");
        assert!(!n.contains("-gui"), "o par headless, não a janela: {n}");
        assert_eq!(n.ends_with(".exe"), cfg!(windows), "{n}");
    }

    /// `bin()` nunca devolve vazio.
    #[test]
    fn o_caminho_nunca_e_vazio() {
        assert!(bin().to_string_lossy().contains("schematize-overdev"));
    }

    /// **O comando entra no PROJETO, usa o caminho absoluto e o eco bate com o que roda.**
    #[test]
    fn o_comando_usa_caminho_absoluto_entra_no_projeto_e_o_eco_bate() {
        let g = "/home/u/.cargo/bin/schematize-overdev";
        let c = comando_para_terminal(g, Path::new("/p/meu projeto"), &["restore", "7"]);
        assert!(c.starts_with("cd '/p/meu projeto' || exit 1;"), "{c}");
        // Execução com o binário aspado; o eco mostra o mesmo caminho e os mesmos argumentos.
        assert_eq!(c.matches(&format!("'{g}' restore 7;")).count(), 1, "execução: {c}");
        assert!(c.contains("echo '── ") && c.contains("restore 7 ("), "eco: {c}");
        assert!(c.contains("read -n1"), "o terminal não fecha na cara");
    }

    /// Aspas simples no caminho não quebram o comando.
    #[test]
    fn aspa_no_caminho_nao_escapa_do_comando() {
        assert_eq!(aspa("a'b"), "'a'\\''b'");
        let c = comando_para_terminal("/b/x", Path::new("/p/o'brien; rm -rf ~"), &["restore", "1"]);
        assert!(c.starts_with("cd '/p/o'\\''brien; rm -rf ~' || exit 1;"), "{c}");
    }

    /// **A casca não sabe NADA do domínio (D4)**: lê o próprio fonte e procura as palavras do
    /// negócio (fora de comentário e de teste). Self-check: o varredor sabe ACHAR.
    #[test]
    fn a_casca_nao_sabe_nada_do_dominio() {
        let fonte = include_str!("cli.rs");
        let producao = fonte.split("#[cfg(test)]").next().expect("há código antes dos testes");
        let codigo: String = producao
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
            .to_lowercase();
        for palavra in ["checklist", "pergunta", "fila", "quiz", "progresso", "snapshot"] {
            assert!(!codigo.contains(palavra), "`{palavra}` é domínio e não pode estar na casca");
        }
        assert!(codigo.contains("comando_para_terminal"), "o filtro comeu o código de produção");
    }
}
