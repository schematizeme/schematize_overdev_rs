//! Parkeamento no `CHECKLIST.md`: item aberto que esgotou a escada vira `- [~]` — nunca pergunta.
//!
//! **O quê:** troca `- [ ]` por `- [~]` na linha que contém a marca do item (função PURA +
//! gravação atômica). **Onde:** executor, ao parkear um item; retomada, ao fechar o que sobrou.

use std::path::Path;

/// **O quê:** devolve o texto com a PRIMEIRA linha `- [ ]` que contém `marca` trocada por
/// `- [~]`; `None` se nenhuma linha aberta casa (ou marca vazia). **Onde:** [`parkear_no_arquivo`].
pub fn parkear_linha(md: &str, marca: &str) -> Option<String> {
    if marca.trim().is_empty() {
        return None;
    }
    let mut feito = false;
    let linhas: Vec<String> = md
        .lines()
        .map(|l| {
            let t = l.trim_start();
            if !feito && t.starts_with("- [ ]") && l.contains(marca) {
                feito = true;
                l.replacen("- [ ]", "- [~]", 1)
            } else {
                l.to_string()
            }
        })
        .collect();
    let mut out = linhas.join("\n");
    if md.ends_with('\n') {
        out.push('\n');
    }
    feito.then_some(out)
}

/// **O quê:** aplica [`parkear_linha`] no arquivo (escrita atômica); `Ok(true)` se mudou algo.
/// **Onde:** executor, depois de registrar `Parkeada` no journal. Arquivo ausente = `Ok(false)`.
pub fn parkear_no_arquivo(caminho: &Path, marca: &str) -> Result<bool, String> {
    let md = match std::fs::read_to_string(caminho) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(format!("ler {}: {e}", caminho.display())),
    };
    match parkear_linha(&md, marca) {
        Some(novo) => {
            schematize_plataforma::arquivo::escreve_atomico(caminho, &novo)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Só a primeira linha ABERTA que casa muda; `[x]`, `[H ]` e marca vazia ficam intactos.
    #[test]
    fn parkeia_so_a_linha_aberta_certa() {
        let md = "- [x] fazer A\n- [ ] fazer A\n- [ ] fazer A de novo\n- [H ] fazer A\n";
        let novo = parkear_linha(md, "fazer A").unwrap();
        assert_eq!(novo, "- [x] fazer A\n- [~] fazer A\n- [ ] fazer A de novo\n- [H ] fazer A\n");
        assert_eq!(parkear_linha(md, ""), None);
        assert_eq!(parkear_linha(md, "inexistente"), None);
        assert_eq!(parkear_linha("- [x] ok", "ok"), None);
    }

    /// Arquivo ausente não é erro; arquivo real é reescrito.
    #[test]
    fn arquivo_ausente_e_real() {
        let d = std::env::temp_dir().join(format!("od-chk-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(parkear_no_arquivo(&d.join("nada.md"), "x"), Ok(false));
        let f = d.join("CHECKLIST.md");
        std::fs::write(&f, "- [ ] item ç\n").unwrap();
        assert_eq!(parkear_no_arquivo(&f, "item ç"), Ok(true));
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "- [~] item ç\n");
        std::fs::remove_dir_all(&d).unwrap();
    }
}
