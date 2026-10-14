//! Montagem do `PATH` do filho (o `claude` no PTY) — a regra pura, com o separador como dado.
//!
//! **O quê:** [`montar_path`] junta o `PATH` herdado com diretórios extras sem duplicar;
//! [`path_do_filho`] o monta com as listas da `Plataforma` (seam `caminhos`).
//!
//! **Onde:** `ClaudeRunner::spawn` (o filho herda um env mínimo quando o app abre pelo lançador
//! do desktop). As listas e o separador vêm do seam `Caminhos` da plataforma (o Windows injeta
//! os dele); a regra de junção é uma só.

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

/// **O quê:** o `PATH` do filho para `home`, com separador e diretórios extras da `Plataforma`
/// instalada (`plat.caminhos`: `:` e `~/.local/bin`… no Unix; `;` e os do Windows lá).
///
/// **Onde:** `ClaudeRunner::spawn`. Efeitos: lê a plataforma; não toca o ambiente.
pub fn path_do_filho(base: &str, home: &std::path::Path) -> String {
    let c = &crate::ambiente::plat().caminhos;
    let extras: Vec<String> =
        c.extras_path_agente(home).iter().map(|p| p.to_string_lossy().into_owned()).collect();
    montar_path(c.separador_path(), base, &extras)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O resultado Unix é o de sempre: base primeiro, extras na ordem, sem duplicar, sem vazios.
    #[test]
    fn unix_mantem_a_ordem_e_nao_duplica() {
        let p = path_do_filho("/usr/bin::/x/bin", std::path::Path::new("/h"));
        assert_eq!(p, "/usr/bin:/x/bin:/h/.local/bin:/h/.cargo/bin:/usr/local/bin:/bin");
    }

    /// Base vazia vira só os extras; separador é parâmetro (o Windows usa `;`).
    #[test]
    fn separador_e_base_vazia() {
        assert_eq!(montar_path(';', "", &["C:\\a".into(), "C:\\b".into()]), "C:\\a;C:\\b");
        assert_eq!(montar_path(':', "a", &["a".into()]), "a");
    }
}
