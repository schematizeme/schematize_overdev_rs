//! Os ARGUMENTOS que a janela manda ao binário — montados por funções PURAS.
//!
//! **O quê:** cada ação da janela (fechar/responder/recusar item humano, anexar nota, capturar
//! demanda, dividir, executar) vira um `Vec<String>` de argumentos do `schematize-overdev`.
//!
//! **Onde:** [`super::resolver`], [`super::execucao`], [`super::caixa`] e [`super::notas`].
//!
//! ## Por que montar aqui, e não na hora do clique
//!
//! O que a janela faz É a linha de comando que ela monta. Separada do callback, a linha é
//! afirmável sem display e sem binário: o teste diz EXATAMENTE o que o clique roda — inclusive
//! para as ações que abrem o terminal (Executar, Dividir + lançar), que nenhum teste pode clicar
//! sem spawnar um `claude`.
//!
//! ## O `--` antes do texto livre
//!
//! Texto digitado pela pessoa pode começar com `-` ("-corrija o botão"). Sem o `--` o `clap` o
//! tomaria por flag e a ação falharia com "unexpected argument" sobre algo que a pessoa escreveu
//! de boa-fé (§37.48: edge case de leigo é bug do software).

/// **O quê:** `human --done N` — fecha o N-ésimo item humano ABERTO. **Onde:** «Feito» na linha.
pub fn feito(n: i64) -> Vec<String> {
    vec!["human".into(), "--done".into(), n.to_string()]
}

/// **O quê:** `answer N -- texto` ou `refuse N -- texto`. **Onde:** o modal de resolver.
///
/// `responder` LIBERA o item de máquina que o humano travava; recusar o CANCELA.
pub fn resolver(n: i64, responder: bool, texto: &str) -> Vec<String> {
    vec![
        if responder { "answer" } else { "refuse" }.into(),
        n.to_string(),
        "--".into(),
        texto.trim().to_string(),
    ]
}

/// **O quê:** `note --kind task|correcao -- texto`. **Onde:** a aba Notas.
pub fn nota(correcao: bool, texto: &str) -> Vec<String> {
    vec![
        "note".into(),
        "--kind".into(),
        if correcao { "correcao" } else { "task" }.into(),
        "--".into(),
        texto.trim().to_string(),
    ]
}

/// **O quê:** `add -- texto` — captura uma demanda na caixa. **Onde:** a aba Caixa.
pub fn capturar(texto: &str) -> Vec<String> {
    vec!["add".into(), "--".into(), texto.trim().to_string()]
}

/// **O quê:** `caixa agente` — o comando do app que abre um agente num TERMINAL para organizar as
/// demandas. **Onde:** «Organizar com agente». O app abre o terminal; a janela não spawna o `claude`.
pub fn caixa_agente() -> Vec<String> {
    vec!["caixa".into(), "agente".into()]
}

/// **O quê:** `caixa merge` — funde as demandas organizadas no checklist (atômico, sob trava).
/// **Onde:** «Fundir no checklist».
pub fn caixa_mesclar() -> Vec<String> {
    vec!["caixa".into(), "merge".into()]
}

/// **O quê:** `split K [--dispatch]` — o que o TERMINAL roda para dividir o checklist.
///
/// **Onde:** «Dividir» e «Dividir + lançar». Vai ao terminal porque REORGANIZA o checklist em
/// arquivos e, com `--dispatch`, lança K `claude`.
pub fn dividir(k: i64, dispatch: bool) -> Vec<String> {
    let mut a = vec!["split".to_string(), k.to_string()];
    if dispatch {
        a.push("--dispatch".into());
    }
    a
}

/// **O quê:** `run` — o que o TERMINAL roda ao «Executar». **Onde:** o guardrail de Executar.
///
/// **SEM `--yes` de propósito:** o `run` pergunta `[s/N]` no terminal antes de disparar o agente
/// (que MEXE no projeto). A confirmação da janela mostra o comando; a do terminal é a segunda
/// chance, com o `Ctrl-C` funcionando.
pub fn executar() -> Vec<&'static str> {
    vec!["run"]
}

/// **O quê:** `stop` — encerra o run. **Onde:** «Parar». Comando rápido, sem terminal.
pub fn parar() -> Vec<&'static str> {
    vec!["stop"]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feito_e_human_done_com_o_numero() {
        assert_eq!(feito(3), ["human", "--done", "3"]);
    }

    /// Responder e recusar têm VERBOS diferentes: trocar um pelo outro libera quando era para
    /// cancelar (ou o contrário).
    #[test]
    fn responder_libera_e_recusar_cancela() {
        assert_eq!(resolver(2, true, "  vale a opção A "), ["answer", "2", "--", "vale a opção A"]);
        assert_eq!(resolver(2, false, "não se aplica"), ["refuse", "2", "--", "não se aplica"]);
    }

    /// **Texto que começa com `-` não vira flag**: o `--` vem antes dele, sempre.
    #[test]
    fn texto_livre_vem_depois_do_separador() {
        for a in [
            resolver(1, true, "-x"),
            resolver(1, false, "--force"),
            nota(false, "-corrija"),
            nota(true, "--kind"),
            capturar("-também precisa"),
        ] {
            let i = a.iter().position(|x| x == "--").expect("tem separador");
            assert_eq!(i, a.len() - 2, "o texto é o ÚLTIMO argumento, logo após `--`: {a:?}");
        }
    }

    #[test]
    fn nota_distingue_task_de_correcao() {
        assert_eq!(nota(false, "t"), ["note", "--kind", "task", "--", "t"]);
        assert_eq!(nota(true, "c"), ["note", "--kind", "correcao", "--", "c"]);
    }

    /// «Organizar com agente» roda o comando do app que abre o terminal; «Fundir» não passa por
    /// terminal nenhum (é a fusão atômica do CLI). Trocar um pelo outro lançaria um `claude`
    /// quando só se queria fundir.
    #[test]
    fn caixa_agente_abre_terminal_e_mesclar_nao() {
        assert_eq!(caixa_agente(), ["caixa", "agente"]);
        assert_eq!(caixa_mesclar(), ["caixa", "merge"]);
    }

    #[test]
    fn captura_nao_toca_o_checklist() {
        assert_eq!(capturar(" demanda "), ["add", "--", "demanda"]);
    }

    /// Dividir NÃO lança, a menos que peça; lançar é a flag `--dispatch`.
    #[test]
    fn dividir_so_lanca_com_dispatch() {
        assert_eq!(dividir(4, false), ["split", "4"]);
        assert_eq!(dividir(4, true), ["split", "4", "--dispatch"]);
    }

    /// **Executar não passa `--yes`:** o terminal pergunta antes de o agente tocar o projeto.
    #[test]
    fn executar_nao_pula_a_confirmacao() {
        assert_eq!(executar(), ["run"]);
        assert!(!executar().contains(&"--yes"));
        assert_eq!(parar(), ["stop"]);
    }

    /// **A linha de comando EXATA que o clique abre no terminal** — o teste que substitui clicar
    /// em «Executar» e «Dividir + lançar», que spawnariam um `claude` de verdade.
    #[test]
    fn o_terminal_de_executar_e_de_dividir_roda_exatamente_isto() {
        use crate::cli::comando_para_terminal;
        use std::path::Path;
        let bin = "/home/u/.cargo/bin/schematize-overdev";
        let dir = Path::new("/p/meu projeto");

        let c = comando_para_terminal(bin, dir, &executar());
        assert!(c.starts_with("cd '/p/meu projeto' || exit 1;"), "{c}");
        assert_eq!(
            c.matches(&format!("'{bin}' run;")).count(),
            1,
            "executa UMA vez, sem --yes: {c}"
        );
        assert!(!c.contains("--yes"), "{c}");
        assert!(c.contains("read -n1"), "o terminal não fecha na cara: {c}");

        let a = dividir(4, true);
        let refs: Vec<&str> = a.iter().map(String::as_str).collect();
        let c = comando_para_terminal(bin, dir, &refs);
        assert_eq!(c.matches(&format!("'{bin}' split 4 --dispatch;")).count(), 1, "{c}");
        let a = dividir(3, false);
        let refs: Vec<&str> = a.iter().map(String::as_str).collect();
        let c = comando_para_terminal(bin, dir, &refs);
        assert!(c.contains(&format!("'{bin}' split 3;")) && !c.contains("--dispatch"), "{c}");
    }
}
