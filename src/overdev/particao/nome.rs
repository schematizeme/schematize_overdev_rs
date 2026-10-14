//! Nome de uma partição de checklist (ADR-0027): parse e format, contrato com o parser.
//!
//! Regex do ADR: `^p([0-3])-([0-9]{3})-([a-z0-9]+(-[a-z0-9]+)*)(--completed)?\.md$`.
//! Sem a crate `regex` de propósito (dependência nova exige verificação, e a gramática é
//! pequena): o parser é manual e testado com entrada hostil.

use crate::paths::SUFIXO_CONCLUIDA;

/// Teto de caracteres do slug.
pub const SLUG_MAX: usize = 40;

/// Nome de partição já decomposto. `Ord` por `(prio, id)` = ordem de trabalho do ADR.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Nome {
    /// Prioridade 0..=3 (`p0` bloqueante … `p3` baixa).
    pub prio: u8,
    /// Id estável `NNN` (0..=999), nunca reusado.
    pub id: u16,
    /// kebab-case ASCII, até [`SLUG_MAX`], sem `--`.
    pub slug: String,
    /// Sufixo `--completed` presente.
    pub concluida: bool,
}

impl Nome {
    /// **O quê:** o nome do arquivo (`p0-001-slug[--completed].md`). **Onde:** `sync`,
    /// `particionar` e os testes (`parse(format(x)) == x`).
    pub fn formatar(&self) -> String {
        let suf = if self.concluida { SUFIXO_CONCLUIDA } else { "" };
        format!("p{}-{:03}-{}{suf}.md", self.prio, self.id, self.slug)
    }

    /// **O quê:** o mesmo nome com `--completed` ligado/desligado. **Onde:** `sync`.
    pub fn com_conclusao(&self, concluida: bool) -> Nome {
        Nome { concluida, ..self.clone() }
    }

    /// **O quê:** o id formatado com 3 dígitos. **Onde:** cabeçalho `depende:` e tabela do índice.
    pub fn id3(&self) -> String {
        format!("{:03}", self.id)
    }
}

/// **O quê:** valida um slug (kebab ASCII minúsculo, sem `--`, sem hífen nas pontas, ≤40).
/// **Onde:** [`parse`] e o teste de [`slugar`].
fn slug_valido(s: &str) -> bool {
    if s.is_empty() || s.len() > SLUG_MAX || s.starts_with('-') || s.ends_with('-') {
        return false;
    }
    if s.contains("--") {
        return false;
    }
    s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// **O quê:** decompõe `nome` segundo a regex do ADR; `None` se não casar.
/// **Onde:** `estado::listar`, `check` (nome inválido = erro) e `sync`.
///
/// Hostil de propósito: unicode, `p4`, NNN com 2/4 dígitos, sem `.md`, `--` no slug e o sufixo
/// no lugar errado devolvem `None`, nunca panic. `--completed` só vale NO FIM, antes de `.md`.
pub fn parse(nome: &str) -> Option<Nome> {
    let base = nome.strip_suffix(".md")?;
    let (base, concluida) = match base.strip_suffix(SUFIXO_CONCLUIDA) {
        Some(b) => (b, true),
        None => (base, false),
    };
    let b = base.as_bytes();
    // `pN-NNN-` = 7 bytes; tudo ASCII, então o fatiamento por byte não corta caractere.
    if !base.is_ascii() || b.len() < 8 || b[0] != b'p' || b[2] != b'-' || b[6] != b'-' {
        return None;
    }
    let prio = match b[1] {
        d @ b'0'..=b'3' => d - b'0',
        _ => return None,
    };
    if !b[3..6].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let id: u16 = base[3..6].parse().ok()?;
    let slug = &base[7..];
    if !slug_valido(slug) {
        return None;
    }
    Some(Nome { prio, id, slug: slug.to_string(), concluida })
}

/// **O quê:** letra sem acento (pt-BR/latinas comuns) ou a própria. **Onde:** [`slugar`].
fn sem_acento(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
        'ç' | 'Ç' => 'c',
        'ñ' | 'Ñ' => 'n',
        _ => c,
    }
}

/// **O quê:** transforma um título livre num slug válido (≤`max` chars; `geral` se vazio).
/// **Onde:** `migrar` ao nomear a partição a partir do título `##`.
pub fn slugar(titulo: &str, max: usize) -> String {
    let mut s = String::new();
    let mut ultimo_hifen = true;
    for c in titulo.chars().map(sem_acento) {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
            ultimo_hifen = false;
        } else if !ultimo_hifen {
            s.push('-');
            ultimo_hifen = true;
        }
    }
    let teto = max.min(SLUG_MAX);
    if s.len() > teto {
        // Corta em fronteira de palavra: se o char seguinte ao corte não é `-`, recua até o
        // último `-` dentro do teto (só cai no corte seco se a 1ª palavra já passa do teto).
        let corte =
            if s.as_bytes()[teto] == b'-' { teto } else { s[..teto].rfind('-').unwrap_or(teto) };
        s.truncate(corte);
    }
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "geral".to_string()
    } else {
        s
    }
}
