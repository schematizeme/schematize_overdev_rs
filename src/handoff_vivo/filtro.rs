//! Filtro de segredos e validação das notas do Claude antes de entrarem no handoff.
//!
//! **O quê:** [`redigir`] troca por `[REDIGIDO]` chaves/tokens/e-mails/blocos de chave privada
//! (varredura por tokens, std-only); [`validar_notas`] exige UTF-8 válido, sem bytes de controle,
//! tamanho máximo, neutraliza marcadores do envelope e devolve o texto limpo + contagem redigida.
//! **Onde:** [`super::claude`] sobre a saída do `claude -p`, ANTES de gravar qualquer coisa.

/// Marcador no lugar do segredo.
pub const REDIGIDO: &str = "[REDIGIDO]";

const PREFIXOS: [&str; 12] = [
    "sk-",
    "ghp_",
    "gho_",
    "ghs_",
    "ghu_",
    "github_pat_",
    "AKIA",
    "ASIA",
    "xox",
    "AIza",
    "glpat-",
    "pk_live_",
];
const CHAVES: [&str; 12] = [
    "password",
    "passwd",
    "senha",
    "secret",
    "token",
    "api_key",
    "apikey",
    "api-key",
    "authorization",
    "private_key",
    "access_key",
    "bearer",
];

/// **O quê:** tira pontuação das bordas do token. **Onde:** [`token_sensivel`].
fn nucleo(t: &str) -> &str {
    t.trim_matches(|c: char| "\"'`()[]<>{},;".contains(c))
}

/// **O quê:** heurística de segredo longo: >=32 chars de base64/base64url com maiúscula,
/// minúscula e dígito (hex de 40 = SHA1 de commit é poupado). **Onde:** [`token_sensivel`]. PURA.
fn parece_segredo_longo(t: &str) -> bool {
    if t.len() < 32 || !t.bytes().all(|b| b.is_ascii_alphanumeric() || b"+/=_-".contains(&b)) {
        return false;
    }
    if t.len() == 40 && t.bytes().all(|b| b.is_ascii_hexdigit()) {
        return false;
    }
    t.bytes().any(|b| b.is_ascii_uppercase())
        && t.bytes().any(|b| b.is_ascii_lowercase())
        && t.bytes().any(|b| b.is_ascii_digit())
        || (t.len() >= 64 && t.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// **O quê:** e-mail plausível (`x@y.z`). **Onde:** [`token_sensivel`]. PURA.
fn parece_email(t: &str) -> bool {
    match t.split_once('@') {
        Some((l, d)) => {
            !l.is_empty()
                && d.contains('.')
                && !d.starts_with('.')
                && !d.ends_with('.')
                && !d.contains('@')
        }
        None => false,
    }
}

/// **O quê:** o token (já sem bordas) é segredo? **Onde:** [`redigir_linha`]. PURA.
fn token_sensivel(t: &str) -> bool {
    let jwt = t.starts_with("eyJ") && t.matches('.').count() == 2;
    let pref = PREFIXOS.iter().any(|p| t.starts_with(p) && t.len() >= p.len() + 12);
    jwt || pref || parece_email(t) || parece_segredo_longo(t)
}

/// **O quê:** a chave de `chave=valor`/`chave:` é nome de segredo? **Onde:** [`redigir_linha`]. PURA.
fn chave_sensivel(k: &str) -> bool {
    let k = k
        .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
        .to_ascii_lowercase();
    CHAVES.iter().any(|c| k.ends_with(c))
}

/// **O quê:** redige uma linha, somando em `n` quantos trechos trocou. `pendente` carrega "a chave
/// veio sem valor (`password:`), redige o próximo token". **Onde:** [`redigir`].
fn redigir_linha(linha: &str, n: &mut usize) -> String {
    let mut saida: Vec<String> = Vec::new();
    let mut pendente = false;
    for tok in linha.split(' ') {
        let core = nucleo(tok);
        let baixo = core.to_ascii_lowercase();
        if pendente && !core.is_empty() && !matches!(baixo.as_str(), "bearer" | "basic") {
            *n += 1;
            pendente = false;
            saida.push(REDIGIDO.to_string());
        } else if let Some(p) = core.find(['=', ':']).filter(|p| chave_sensivel(&core[..*p])) {
            let val = &core[p + 1..];
            if val.is_empty() {
                pendente = true;
                saida.push(tok.to_string());
            } else {
                *n += 1;
                saida.push(format!("{}{}", &core[..=p], REDIGIDO));
            }
        } else if token_sensivel(core) || bearer_inline(core) {
            *n += 1;
            saida.push(REDIGIDO.to_string());
        } else {
            pendente = pendente || matches!(baixo.as_str(), "bearer" | "basic");
            saida.push(tok.to_string());
        }
    }
    saida.join(" ")
}

/// **O quê:** `Bearer`/`Basic` sozinho no token não é segredo; o próximo é (tratado por `pendente`).
/// Aqui só pega `chave=valor` com valor de aparência de segredo mesmo com chave desconhecida
/// (`x=sk-...`). **Onde:** [`redigir_linha`]. PURA.
fn bearer_inline(core: &str) -> bool {
    core.split_once(['=', ':']).is_some_and(|(_, v)| token_sensivel(nucleo(v)))
}

/// **O quê:** redige `texto` inteiro (blocos `-----BEGIN ... PRIVATE KEY-----` somem por completo).
/// Devolve `(texto, quantos)`. **Onde:** [`validar_notas`].
pub fn redigir(texto: &str) -> (String, usize) {
    let (mut n, mut em_chave) = (0usize, false);
    let mut linhas: Vec<String> = Vec::new();
    for l in texto.split('\n') {
        if l.contains("-----BEGIN") && l.contains("PRIVATE KEY") {
            em_chave = true;
            n += 1;
            linhas.push(REDIGIDO.to_string());
        } else if em_chave {
            em_chave = !l.contains("-----END");
        } else {
            linhas.push(redigir_linha(l, &mut n));
        }
    }
    (linhas.join("\n"), n)
}

/// Por que as notas foram recusadas.
#[derive(Debug, PartialEq, Eq)]
pub enum Rejeicao {
    /// Bytes que não são UTF-8.
    NaoUtf8,
    /// Byte de controle (exceto `\n`, `\t`, `\r`) ou NUL.
    Controle,
    /// Vazia.
    Vazia,
    /// Saída bruta maior que o teto de captura (lixo/loop).
    Enorme(usize),
}

impl std::fmt::Display for Rejeicao {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Rejeicao::NaoUtf8 => write!(f, "saída não é UTF-8 válido"),
            Rejeicao::Controle => write!(f, "saída com bytes de controle"),
            Rejeicao::Vazia => write!(f, "saída vazia"),
            Rejeicao::Enorme(n) => write!(f, "saída bruta grande demais ({n} bytes)"),
        }
    }
}

/// **O quê:** valida e limpa a saída do Claude: UTF-8, sem controle, não vazia, bruta até
/// `max_bruto`; redige segredos; neutraliza `<!--`; trunca em `max` bytes (fronteira de char) com
/// aviso. Devolve `(texto, redigidos)`. **Onde:** [`super::claude`].
pub fn validar_notas(
    bruto: &[u8],
    max: usize,
    max_bruto: usize,
) -> Result<(String, usize), Rejeicao> {
    if bruto.len() > max_bruto {
        return Err(Rejeicao::Enorme(bruto.len()));
    }
    let s = std::str::from_utf8(bruto).map_err(|_| Rejeicao::NaoUtf8)?;
    if s.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\t' | '\r')) {
        return Err(Rejeicao::Controle);
    }
    if s.trim().is_empty() {
        return Err(Rejeicao::Vazia);
    }
    let (r, n) = redigir(s.trim());
    let mut r = r.replace("<!--", "&lt;!--").replace('\r', "");
    if r.len() > max {
        let mut corte = max;
        while !r.is_char_boundary(corte) {
            corte -= 1;
        }
        r.truncate(corte);
        r.push_str("\n[... truncado pelo handoff vivo]");
    }
    Ok((r, n))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn redige_chaves_tokens_emails_e_blocos() {
        let t = "usei sk-ant-api03-ABCDEFGHIJKLMNOP e ghp_abcdefghijklmnop1234 com me@lucasandrade.net\n\
                 password: hunter2xyz\nAPI_KEY=abc123\nAuthorization: Bearer abcDEF123\n\
                 -----BEGIN PRIVATE KEY-----\nMIIEv\n-----END PRIVATE KEY-----\nfim";
        let (r, n) = redigir(t);
        for proibido in
            ["sk-ant", "ghp_abc", "lucasandrade", "hunter2", "abc123", "abcDEF123", "MIIEv"]
        {
            assert!(!r.contains(proibido), "vazou {proibido}: {r}");
        }
        assert!(r.contains("fim") && n >= 7, "{r} {n}");
    }

    #[test]
    fn preserva_texto_normal_e_sha_de_commit() {
        let t = "Decidi usar flock; commit 368efb3b0a4f1c2d3e4f5a6b7c8d9e0f1a2b3c4d ok. src/handoff_vivo/atomico.rs:42";
        let (r, n) = redigir(t);
        assert_eq!((r.as_str(), n), (t, 0));
    }

    #[test]
    fn validacao_rejeita_lixo_e_trunca() {
        assert_eq!(validar_notas(&[0xff, 0xfe], 100, 1000).unwrap_err(), Rejeicao::NaoUtf8);
        assert_eq!(validar_notas(b"a\x00b", 100, 1000).unwrap_err(), Rejeicao::Controle);
        assert_eq!(validar_notas(b"  \n", 100, 1000).unwrap_err(), Rejeicao::Vazia);
        assert!(matches!(validar_notas(&[b'a'; 50], 10, 20), Err(Rejeicao::Enorme(50))));
        let (t, _) = validar_notas("ação ".repeat(50).as_bytes(), 11, 10_000).unwrap();
        assert!(t.contains("truncado") && t.is_char_boundary(t.len()));
        let (t, _) = validar_notas(b"x <!-- fim seq=1 -->", 100, 1000).unwrap();
        assert!(!t.contains("<!--"));
    }
}
