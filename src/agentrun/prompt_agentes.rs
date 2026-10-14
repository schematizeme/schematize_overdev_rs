//! O trecho do prompt do overdev que descreve a ESTRUTURA DE AGENTES (ADR-0024).
//!
//! **O quê:** tabela de papéis efetiva (papel | motor | modelo | quando), a escada e as regras
//! de delegação.
//! **Onde:** [`super::overdev_prompt_com`], que o injeta no prompt inicial do `claude`.

use crate::agentes::{Agentes, Motor};

/// **O quê:** a tabela markdown dos papéis ATIVOS. **Onde:** [`trecho_agentes`].
fn tabela(ag: &Agentes) -> String {
    let mut t = String::from("| papel | motor | modelo | quando |\n|---|---|---|---|\n");
    for (nome, p) in ag.papeis.iter().filter(|(_, p)| p.ativo) {
        let quando = p.descricao.replace('|', "/").replace('\n', " ");
        t.push_str(&format!("| {nome} | {} | {} | {quando} |\n", p.motor.nome(), p.modelo));
    }
    t
}

/// **O quê:** o bloco de texto com papéis, escada, roteamento e as regras de delegação.
/// PURA. **Onde:** [`super::overdev_prompt_com`].
pub fn trecho_agentes(ag: &Agentes) -> String {
    let escada = ag.escada().join(" -> ");
    let rotas: Vec<String> =
        ag.roteamento.iter().map(|(tipo, papel)| format!("{tipo} -> {papel}")).collect();
    let rotas = if rotas.is_empty() { "(nenhuma)".to_string() } else { rotas.join(", ") };
    let ex_codex = ag
        .papeis
        .iter()
        .find(|(_, p)| p.ativo && p.motor != Motor::Claude)
        .map_or("fallback-1", |(n, _)| n.as_str());
    format!(
        "ESTRUTURA DE AGENTES (config efetiva — você é o papel `principal`, modelo {}):\n{}\n\
         ESCADA para tarefa de código: {escada}. Falhou -> o MESMO agente corrige (até 2 rodadas) \
         -> sobe um degrau. Esgotou a escada -> `schematize-overdev park` + item `- [~]` (não trave).\n\
         ROTEAMENTO por tipo de tarefa: {rotas}. Tarefa visual/imagem -> use o papel do \
         roteamento e PULE a escada.\n\
         REGRAS: (1) o principal NÃO gera código de entrega — despache, supervisione e revise; \
         tarefa trivial vai ao papel `trivial` (haiku), nunca inline. (2) Subagents Claude: Agent \
         tool com `model=<modelo do papel>`. (3) Codex/Cursor: `schematize-overdev delegar \
         --papel <p> --brief-stdin --json` (ex.: `--papel {ex_codex}`); rc 3 = indisponível, siga \
         o `proximo` do JSON. (4) Imagem: peça ao Codex Terra via `delegar --papel imagem`; \
         PROIBIDO endpoint/API de imagem pago.\n\n",
        ag.modelo_principal(),
        tabela(ag),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O trecho carrega a tabela efetiva, a escada e as regras obrigatórias do ADR-0024.
    #[test]
    fn trecho_tem_tabela_escada_e_regras() {
        let t = trecho_agentes(&Agentes::padrao());
        assert!(t.contains("| papel | motor | modelo | quando |"));
        assert!(t.contains("| executor | claude | sonnet |"));
        assert!(t.contains("trivial -> executor -> fallback-1 -> fallback-cursor -> fallback-2"));
        assert!(t.contains("PULE a escada"));
        assert!(t.contains("PROIBIDO endpoint/API de imagem pago"));
        assert!(t.contains("model=<modelo do papel>"));
        assert!(t.contains("delegar --papel <p> --brief-stdin --json"));
        assert!(t.contains("NÃO gera código"));
        assert!(t.contains("park"));
        assert!(t.contains("`- [~]`"));
    }

    /// Papel inativo não aparece na tabela; override de modelo aparece.
    #[test]
    fn tabela_reflete_a_config_efetiva() {
        let mut a = Agentes::padrao();
        a.papeis.get_mut("trivial").expect("trivial").ativo = false;
        a.papeis.get_mut("executor").expect("executor").modelo = "sonnet-9".into();
        let t = trecho_agentes(&a);
        assert!(!t.contains("| trivial |"));
        assert!(t.contains("| executor | claude | sonnet-9 |"));
    }
}
