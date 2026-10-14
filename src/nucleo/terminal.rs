//! Abrir um script num terminal pelo seam `Terminal` — PLATAFORMA, e nada de domínio.
//!
//! **O quê:** [`abrir_nomeado`] entrega um `ScriptTerminal` à plataforma e diz em QUAL terminal
//! abriu ("abri no konsole" é acionável; "abri um terminal" manda procurar entre as janelas).
//!
//! **Onde:** o lançador do overdev (`agentrun::lancador`) e o `rerun` (`nucleo::agente`).

use schematize_plataforma::seams::abridor::no_path;
use schematize_plataforma::seams::terminal::{escolher, ScriptTerminal};

/// **O quê:** abre `st` pela `Terminal` da plataforma instalada e devolve o nome do terminal.
///
/// **Onde:** os dois lançadores acima. O trait só devolve `()`, então o nome vem da MESMA escolha
/// (`escolher` sobre o `PATH`, escolhida antes de abrir) e vira `"terminal"` onde nenhum emulador
/// da lista existe (outra plataforma). O desacoplamento do app (`process_group(0)`) é do seam —
/// sem ele, fechar a janela que abriu o terminal mataria o agente no meio do trabalho.
///
/// **Efeitos:** grava o script (se houver arquivo) e cria processo.
pub fn abrir_nomeado(st: &ScriptTerminal) -> Result<String, String> {
    let nome = escolher(st.emuladores, &no_path).unwrap_or("terminal").to_string();
    crate::ambiente::plat().terminal.abrir_script(st)?;
    Ok(nome)
}
