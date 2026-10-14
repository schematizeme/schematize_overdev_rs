//! Montagem do `PATH` do filho (o `claude` no PTY) — a regra pura, com o separador como dado.
//!
//! **O quê:** [`montar_path`] junta o `PATH` herdado com diretórios extras sem duplicar;
//! [`extras_unix`] lista os extras de um HOME Unix.
//!
//! **Onde:** `ClaudeRunner::spawn` (o filho herda um env mínimo quando o app abre pelo lançador
//! do desktop). Exposta de propósito (ADR-0026): o repo `schematize_windows_rs` chama
//! [`montar_path`] com `;` e os diretórios dele, sem reescrever a regra.

/// **O quê:** `base` (um `PATH`, separado por `sep`) seguido de cada `extras` que ainda não
/// estiver nele, na ordem dada; entradas vazias de `base` são descartadas.
///
/// **Onde:** `ClaudeRunner::spawn` e o Windows. PURA: sem tocar o ambiente.
pub fn montar_path(sep: char, base: &str, extras: &[String]) -> String {
    let mut partes: Vec<String> =
        base.split(sep).filter(|s| !s.is_empty()).map(String::from).collect();
    for e in extras {
        if !partes.contains(e) {
            partes.push(e.clone());
        }
    }
    partes.join(&sep.to_string())
}

/// **O quê:** os diretórios que um HOME Unix acrescenta ao `PATH` do filho: `~/.local/bin`,
/// `~/.cargo/bin`, `/usr/local/bin`, `/usr/bin`, `/bin`.
///
/// **Onde:** `ClaudeRunner::spawn` em Unix.
pub fn extras_unix(home: &str) -> Vec<String> {
    vec![
        format!("{home}/.local/bin"),
        format!("{home}/.cargo/bin"),
        "/usr/local/bin".to_string(),
        "/usr/bin".to_string(),
        "/bin".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O resultado Unix é o de sempre: base primeiro, extras na ordem, sem duplicar, sem vazios.
    #[test]
    fn unix_mantem_a_ordem_e_nao_duplica() {
        let p = montar_path(':', "/usr/bin::/x/bin", &extras_unix("/h"));
        assert_eq!(p, "/usr/bin:/x/bin:/h/.local/bin:/h/.cargo/bin:/usr/local/bin:/bin");
    }

    /// Base vazia vira só os extras; separador é parâmetro (o Windows usa `;`).
    #[test]
    fn separador_e_base_vazia() {
        assert_eq!(montar_path(';', "", &["C:\\a".into(), "C:\\b".into()]), "C:\\a;C:\\b");
        assert_eq!(montar_path(':', "a", &["a".into()]), "a");
    }
}
