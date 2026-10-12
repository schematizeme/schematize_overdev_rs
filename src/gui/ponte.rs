//! A PONTE entre um clique e o binário: roda o comando e devolve a mensagem para a tela.
//!
//! **O quê:** [`rodar`] (comando rápido, resultado na hora) e [`no_terminal`] (o que spawna o
//! `claude` ou reorganiza arquivos abre o TERMINAL — D6).
//!
//! **Onde:** todas as telas que agem ([`super::resolver`], [`super::execucao`],
//! [`super::caixa`], [`super::notas`]).
//!
//! **A mensagem do CLI é o produto:** ela diz POR QUE a ação foi recusada, e a janela só a
//! repassa (primeira linha). Devolve `(mensagem, erro)` e deixa cada tela escolher ONDE mostrar.

use super::{cli, terminal};
use std::path::Path;

/// **O quê:** a primeira linha não vazia de `t`. **Onde:** [`rodar`] — a mensagem inteira de um
/// erro do clap tem dicas e `Usage:` que não cabem num banner.
pub fn primeira_linha(t: &str) -> String {
    t.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string()
}

/// **O quê:** roda `<bin> <args>` no projeto e devolve `(mensagem, deu_erro)`.
///
/// **Onde:** as ações RÁPIDAS (fechar item, responder, recusar, capturar, fundir, anotar, parar).
/// Quem GRAVA e VALIDA é o CLI; a janela nunca escreve no disco.
pub fn rodar(dir: &Path, args: &[String]) -> (String, bool) {
    match cli::rodar(dir, args) {
        Ok(msg) => (primeira_linha(&msg), false),
        Err(err) => (primeira_linha(&err), true),
    }
}

/// **O quê:** abre `<bin> <args>` no TERMINAL, dentro do projeto, e devolve `(mensagem, erro)`.
///
/// **Onde:** o que spawna o `claude` (Executar, Dividir + lançar) ou reorganiza arquivos
/// (Dividir). **Sem terminal, a mensagem traz o COMANDO** — quem não tem emulador ainda consegue
/// fazer a coisa colando (§37.48).
pub fn no_terminal(dir: &Path, args: &[&str]) -> (String, bool) {
    let bin = cli::bin().display().to_string();
    let cmd = cli::comando_para_terminal(&bin, dir, args);
    if terminal::abrir(&cmd) {
        ("terminal aberto — o comando aparece lá antes de rodar.".to_string(), false)
    } else {
        (format!("não achei um terminal. Rode: {cmd}"), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mensagem_e_a_primeira_linha_com_texto() {
        assert_eq!(primeira_linha("\n\n  liberado: item\nsegunda\n"), "liberado: item");
        assert_eq!(primeira_linha(""), "");
        assert_eq!(primeira_linha("   \n  "), "");
    }
}
