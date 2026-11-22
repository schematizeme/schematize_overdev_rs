//! Montador de prompt com prefixo estável e hash (IA-03).
//!
//! **O quê:** ordem fixa estático-global, estático-projeto, unidade, tarefa; hash SHA-256 do
//! prefixo (global + projeto); lint que rejeita timestamp, UUID, epoch e caminho temporário nos
//! blocos estáticos (eles invalidariam o cache de prompt entre unidades).
//! **Onde:** gerador de brief do orquestrador; o hash vai ao ledger. O bloco global é o
//! `padroes::prefixo_estavel()` (versionado por `PADROES_VERSAO`): [`montar_com_padroes`].

use super::sha256::sha256_hex;

/// Separador fixo entre blocos (parte do prefixo, byte-estável).
const SEP: &str = "\n\n";

/// Entradas do montador. Só `unidade` e `tarefa` podem variar entre unidades da mesma onda.
#[derive(Debug, Clone, Copy)]
pub struct Brief<'a> {
    /// Bloco estático global (padrões da casa).
    pub global: &'a str,
    /// Bloco estático do projeto.
    pub projeto: &'a str,
    /// Bloco da unidade (contrato, arquivos).
    pub unidade: &'a str,
    /// A tarefa em si.
    pub tarefa: &'a str,
}

/// Prompt montado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Montado {
    /// Texto completo: prefixo + unidade + tarefa.
    pub texto: String,
    /// Tamanho do prefixo em bytes (o texto começa com ele).
    pub prefixo_len: usize,
    /// SHA-256 hex do prefixo (para o ledger).
    pub prefixo_sha256: String,
}

/// Tipo de conteúdo volátil achado num bloco estático.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tipo {
    /// Data e hora (`2026-11-22T10:00` ou com espaço).
    Timestamp,
    /// UUID (8-4-4-4-12 hex).
    Uuid,
    /// Epoch Unix de 10 ou 13 dígitos.
    Epoch,
    /// Caminho temporário (`/tmp/`, `/var/folders/`, ...).
    CaminhoTemporario,
}

/// Uma violação do lint: linha (1-based), tipo e trecho.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violacao {
    /// Linha do bloco.
    pub linha: usize,
    /// O que foi achado.
    pub tipo: Tipo,
    /// Trecho (até 40 caracteres).
    pub trecho: String,
}

const TEMPORARIOS: [&str; 6] =
    ["/tmp/", "/var/tmp/", "/var/folders/", "/private/tmp/", "target/tmp", "\\Temp\\"];

/// `true` se `b[i..]` começa com data-hora `DDDD-DD-DD[T ]DD:DD`.
/// **O quê:** scanner manual (sem regex). **Onde:** [`lint_linha`].
fn timestamp_em(b: &[u8], i: usize) -> bool {
    let molde = b"dddd-dd-ddXdd:dd";
    if b.len() < i + molde.len() {
        return false;
    }
    molde.iter().zip(&b[i..]).all(|(m, c)| match m {
        b'd' => c.is_ascii_digit(),
        b'X' => *c == b'T' || *c == b' ',
        _ => c == m,
    })
}

/// `true` se `b[i..]` começa com UUID. **Onde:** [`lint_linha`].
pub fn uuid_em(b: &[u8], i: usize) -> bool {
    let molde = b"xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx";
    if b.len() < i + molde.len() {
        return false;
    }
    molde.iter().zip(&b[i..]).all(|(m, c)| if *m == b'x' { c.is_ascii_hexdigit() } else { c == m })
}

/// `true` se há run de 10/13 dígitos (começando 15-19) a partir de `i`, delimitado.
/// **O quê:** epoch Unix em s/ms. **Onde:** [`lint_linha`].
fn epoch_em(b: &[u8], i: usize) -> bool {
    if i > 0 && b[i - 1].is_ascii_digit() {
        return false;
    }
    let n = b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
    (n == 10 || n == 13) && b[i] == b'1' && (b'5'..=b'9').contains(&b[i + 1])
}

/// Lint de uma linha. **O quê:** varre posições e coleta tipos. **Onde:** [`lint_estatico`].
fn lint_linha(l: &str, n: usize, out: &mut Vec<Violacao>) {
    let b = l.as_bytes();
    let trecho = |i: usize| l.get(i..).unwrap_or("").chars().take(40).collect::<String>();
    let mut push = |tipo, i: usize| out.push(Violacao { linha: n, tipo, trecho: trecho(i) });
    for i in 0..b.len() {
        if !b[i].is_ascii_digit() && !b[i].is_ascii_hexdigit() {
            continue;
        }
        if timestamp_em(b, i) {
            push(Tipo::Timestamp, i);
        } else if uuid_em(b, i) {
            push(Tipo::Uuid, i);
        } else if b[i].is_ascii_digit() && epoch_em(b, i) {
            push(Tipo::Epoch, i);
        }
    }
    for t in TEMPORARIOS {
        if let Some(i) = l.find(t) {
            push(Tipo::CaminhoTemporario, i);
        }
    }
}

/// Procura conteúdo volátil em um bloco estático.
///
/// **O quê:** lista de violações (vazia = estável). **Onde:** [`montar`] e testes do prefixo.
pub fn lint_estatico(bloco: &str) -> Vec<Violacao> {
    let mut out = Vec::new();
    for (i, l) in bloco.lines().enumerate() {
        lint_linha(l, i + 1, &mut out);
    }
    out
}

/// Monta o prompt na ordem fixa e calcula o hash do prefixo.
///
/// **O quê:** rejeita (`Err`) se `global` ou `projeto` tiverem conteúdo volátil; `unidade` e
/// `tarefa` NÃO passam pelo lint (variam por definição). **Onde:** gerador de brief.
pub fn montar(b: &Brief) -> Result<Montado, Vec<Violacao>> {
    let mut v = lint_estatico(b.global);
    v.extend(lint_estatico(b.projeto));
    if !v.is_empty() {
        return Err(v);
    }
    let prefixo = format!("{}{SEP}{}{SEP}", b.global, b.projeto);
    let texto = format!("{prefixo}{}{SEP}{}", b.unidade, b.tarefa);
    Ok(Montado {
        prefixo_len: prefixo.len(),
        prefixo_sha256: sha256_hex(prefixo.as_bytes()),
        texto,
    })
}

/// Como [`montar`], com o bloco global = `padroes::prefixo_estavel()`.
///
/// **O quê:** o global não passa pelo lint (é versionado e coberto por `padroes_prefixo`);
/// só o bloco do projeto é conferido. **Onde:** briefs de overdev/agentes.
pub fn montar_com_padroes(
    projeto: &str,
    unidade: &str,
    tarefa: &str,
) -> Result<Montado, Vec<Violacao>> {
    let v = lint_estatico(projeto);
    if !v.is_empty() {
        return Err(v);
    }
    let g = crate::padroes::prefixo_estavel();
    let prefixo = format!("{g}{SEP}{projeto}{SEP}");
    Ok(Montado {
        texto: format!("{prefixo}{unidade}{SEP}{tarefa}"),
        prefixo_len: prefixo.len(),
        prefixo_sha256: sha256_hex(prefixo.as_bytes()),
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    fn br<'a>(u: &'a str, t: &'a str) -> Brief<'a> {
        Brief { global: "G regras", projeto: "P stack rust", unidade: u, tarefa: t }
    }

    #[test]
    fn unidades_diferentes_mesmo_prefixo_byte_identico() {
        let a = montar(&br("unidade A", "faz x")).unwrap();
        let b = montar(&br("unidade B com texto maior", "faz y")).unwrap();
        assert_eq!(a.prefixo_sha256, b.prefixo_sha256);
        assert_eq!(a.texto.as_bytes()[..a.prefixo_len], b.texto.as_bytes()[..b.prefixo_len]);
        assert_eq!(a.prefixo_len, b.prefixo_len);
        assert_ne!(a.texto, b.texto);
        assert!(a.texto.ends_with("faz x"));
    }

    #[test]
    fn lint_falha_com_timestamp_uuid_epoch_e_tmp() {
        for (txt, tipo) in [
            ("gerado em 2026-11-22T10:05:00Z", Tipo::Timestamp),
            ("agora 2026-11-22 10:05", Tipo::Timestamp),
            ("id 123e4567-e89b-12d3-a456-426614174000", Tipo::Uuid),
            ("ts 1763800000", Tipo::Epoch),
            ("ts 1763800000123", Tipo::Epoch),
            ("dir /tmp/abc", Tipo::CaminhoTemporario),
        ] {
            let g = format!("ok\n{txt}");
            let e = montar(&Brief { global: &g, ..br("u", "t") }).unwrap_err();
            assert_eq!(e[0].tipo, tipo, "{txt}");
            assert_eq!(e[0].linha, 2);
        }
        let e = montar(&Brief { projeto: "x /var/folders/zz", ..br("u", "t") }).unwrap_err();
        assert_eq!(e.len(), 1);
    }

    #[test]
    fn nao_acusa_falso_positivo_e_aguenta_unicode() {
        assert!(lint_estatico("versão 2026.11.20-1, 12345678901234 e 99999 ação ç").is_empty());
        assert!(lint_estatico("data 2026-11-22 sem hora").is_empty());
        assert!(lint_estatico("𝔘𝔫𝔦 \u{0}").is_empty());
        assert!(lint_estatico("").is_empty());
        // unidade e tarefa podem ter volátil: não passam pelo lint.
        assert!(montar(&br("em 2026-11-22T10:00", "/tmp/x")).is_ok());
    }

    #[test]
    fn prefixo_dos_padroes_nao_muda_com_a_unidade() {
        let a = montar_com_padroes("proj", "u1", "t1").unwrap();
        let b = montar_com_padroes("proj", "u2", "t2").unwrap();
        assert_eq!(a.prefixo_sha256, b.prefixo_sha256);
        assert!(a.texto.starts_with(&crate::padroes::prefixo_estavel()));
        assert!(montar_com_padroes("em 2026-11-22T10:00", "u", "t").is_err());
    }
}
