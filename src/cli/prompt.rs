//! Helpers de terminal usados por vários comandos do hub.
//!
//! **O quê:** confirmação interativa e canonicalização de caminho.
//!
//! **Onde:** `overdev`, `diversos` e o que mais precisar perguntar antes de agir.
//!
//! ## Por que estão aqui, e não onde estavam
//!
//! Moravam em `cli/ssh.rs` — por acidente de história, porque foi lá que nasceram. Quando o
//! `ssh` foi delegado ao `schematize-deployer` (ADR-0010, enfim cumprido), o arquivo inteiro
//! virou código morto, e estes dois helpers, que nunca tiveram nada de SSH, teriam ido junto.
//!
//! Um helper genérico escondido dentro de um módulo de domínio é uma armadilha para o dia em
//! que aquele domínio sair: ou ele é apagado com o resto, ou obriga a manter o módulo vivo só
//! por causa dele.

use std::io::{self, BufRead, Write};

/// **O quê:** confirmação interativa (y/N). **Onde:** antes de qualquer passo destrutivo.
///
/// **Falha fechada:** erro, EOF ou qualquer coisa diferente de "sim" devolve `false`. Num pipe
/// sem entrada, o silêncio vira NÃO — o contrário faria um comando destrutivo rodar sozinho.
pub(crate) fn confirm(prompt: &str) -> bool {
    print!("{prompt} ");
    let _ = io::stdout().flush();
    let mut line = String::new();
    if io::stdin().lock().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_lowercase().as_str(), "y" | "yes" | "s" | "sim")
}

// O `canon_or` veio junto na cópia e NÃO é usado aqui: ele canonizava caminho para as
// mensagens de confirmação de comandos que ficaram no hub. Código que ninguém chama é dívida,
// e o `-D warnings` o pega.
