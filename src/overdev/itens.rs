//! Os ITENS do checklist, um a um — o que a janela desenha na lista.
//!
//! **O quê:** lê o `CHECKLIST.md` (2 níveis: máquina e humano) e devolve cada item com o
//! `tipo` já classificado, para o `checklist --json` entregar à janela sem ela parsear
//! Markdown.
//!
//! **Onde:** `cli::saidajson::checklist` (o comando `checklist --json`).
//!
//! ## Por que a classificação mora AQUI e não na janela
//!
//! A janela fala com o binário por `--json` (ADR-0020). Se ela lesse o `.md`, teria uma segunda
//! cópia da lei dos marcadores — e a que divergisse contaria diferente do gate. Aqui os
//! marcadores são os mesmos de [`super::progresso`]: `- [H ]` é casado ANTES de `- [ ]`, ou o
//! item humano cairia no ramo de máquina.

use std::path::Path;

/// Um item do checklist, já classificado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// `open` | `done` | `hold` | `cancel` | `hopen` | `hdone` | `hresp` | `hrec` | `nota`.
    pub tipo: &'static str,
    /// O texto do item, sem o marcador nem o comentário de vínculo (`<!-- … -->`).
    pub texto: String,
    /// `true` para item de máquina (o agente fecha); `false` para humano e notas.
    pub maquina: bool,
    /// Posição 1-based entre os humanos ABERTOS (é o argumento de `human --done`); 0 nos demais.
    pub humano_n: usize,
    /// `true` para linha indentada (subtarefa/vínculo/nota de resposta).
    pub sub: bool,
}

/// **O quê:** tira o comentário de vínculo (`<!-- ovf:q:… -->`) do texto que vai para a tela.
///
/// **Onde:** [`parse`]. O marcador é a cola entre a pergunta e o item que ela trava — mora no
/// arquivo, mas mostrá-lo vazaria detalhe de implementação na cara de quem lê.
fn limpa(s: &str) -> String {
    s.split("<!--").next().unwrap_or(s).trim().to_string()
}

/// **O quê:** o texto do checklist → seus itens, na ordem do arquivo. Função PURA.
///
/// **Onde:** [`itens_at`] e os testes.
///
/// **Linha que não é item é ignorada**, exceto as filhas `- resposta:`/`- recusado:`: elas são
/// o CONTEÚDO da decisão, e esconder deixaria o "respondido" sem dizer o que foi respondido.
pub fn parse(texto: &str) -> Vec<Item> {
    let mut out = Vec::new();
    let mut hopen = 0usize;
    for line in texto.lines() {
        let t = line.trim_start();
        let sub = line.len() > t.len();
        if sub && (t.starts_with("- resposta:") || t.starts_with("- recusado:")) {
            out.push(Item {
                tipo: "nota",
                texto: limpa(t.trim_start_matches("- ")),
                maquina: false,
                humano_n: 0,
                sub: true,
            });
            continue;
        }
        let (tipo, maquina, n, resto): (&'static str, bool, usize, &str) = if let Some(r) =
            t.strip_prefix("- [H ]")
        {
            hopen += 1;
            ("hopen", false, hopen, r)
        } else if let Some(r) = t.strip_prefix("- [H x]").or_else(|| t.strip_prefix("- [H X]")) {
            ("hdone", false, 0, r)
        } else if let Some(r) = t.strip_prefix("- [H r]").or_else(|| t.strip_prefix("- [H R]")) {
            ("hresp", false, 0, r)
        } else if let Some(r) = t.strip_prefix("- [H -]") {
            ("hrec", false, 0, r)
        } else if let Some(r) = t.strip_prefix("- [-]") {
            ("cancel", true, 0, r)
        } else if let Some(r) = t.strip_prefix("- [ ]") {
            ("open", true, 0, r)
        } else if let Some(r) = t.strip_prefix("- [x]").or_else(|| t.strip_prefix("- [X]")) {
            ("done", true, 0, r)
        } else if let Some(r) = t.strip_prefix("- [~]") {
            ("hold", true, 0, r)
        } else {
            continue;
        };
        out.push(Item { tipo, texto: limpa(resto), maquina, humano_n: n, sub });
    }
    out
}

/// **O quê:** os itens do checklist do run em `raiz` (CHECKLIST.md e/ou `checklist/*.md`).
///
/// **Onde:** `checklist --json`. Usa o MESMO leitor multi-arquivo do `progresso`, então o que a
/// lista mostra e o que a contagem soma saem do mesmo texto.
pub fn itens_at(raiz: &Path) -> Vec<Item> {
    let d = super::dir_at(raiz);
    parse(&crate::paths::read_multidoc(&d, "CHECKLIST.md", "checklist"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CL: &str = "# t\n- [x] feito\n- [ ] aberto <!-- ovf:q:1 -->\n- [~] preso\n\
        - [H ] decida\n- [H x] humano feito\n- [H r] respondi\n  - resposta: sim\n\
        - [H -] nao\n- [-] cancelado\n  - [ ] sub\nlixo\n- [H ] outro\n";

    /// Cada marcador vira o tipo certo, na ordem do arquivo.
    #[test]
    fn classifica_os_marcadores() {
        let tipos: Vec<&str> = parse(CL).iter().map(|i| i.tipo).collect();
        assert_eq!(
            tipos,
            [
                "done", "open", "hold", "hopen", "hdone", "hresp", "nota", "hrec", "cancel",
                "open", "hopen"
            ]
        );
    }

    /// `- [H ]` não cai no ramo de máquina, e só os humanos ABERTOS são numerados.
    #[test]
    fn humano_aberto_e_numerado_e_nao_e_maquina() {
        let it = parse(CL);
        let h: Vec<(usize, bool)> =
            it.iter().filter(|i| i.tipo == "hopen").map(|i| (i.humano_n, i.maquina)).collect();
        assert_eq!(h, [(1, false), (2, false)]);
        assert!(it.iter().filter(|i| i.tipo != "hopen").all(|i| i.humano_n == 0));
    }

    /// O comentário de vínculo não vaza; a subtarefa e a nota são marcadas `sub`.
    #[test]
    fn limpa_vinculo_e_marca_sub() {
        let it = parse(CL);
        assert_eq!(it[1].texto, "aberto");
        assert!(it.iter().find(|i| i.tipo == "nota").is_some_and(|n| n.sub));
        assert!(it[9].sub && it[9].tipo == "open");
    }

    /// Entrada hostil não panica: vazio, binário, marcador sem texto.
    #[test]
    fn entrada_hostil_nao_panica() {
        assert!(parse("").is_empty());
        let _ = parse("\u{0}\n- [\n- [H\n- [ ]\n- [x]\u{0}");
        assert_eq!(parse("- [ ]")[0].texto, "");
    }
}
