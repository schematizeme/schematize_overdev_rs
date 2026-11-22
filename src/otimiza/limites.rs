//! Teto duro por unidade: budget, turnos e tempo de parede (IA-05).
//!
//! **O quê:** limites por classe de unidade, as flags `--max-budget-usd`/`--max-turns` do
//! `claude -p`, e a classificação da causa quando a unidade estoura (`budget`/`turns`/`timeout`).
//! **Onde:** construtor da linha de comando (`comando::aplicar`) e apuração do resultado.
//! A estimativa de custo do Claude Code é client-side e pode ultrapassar o teto: os valores
//! padrão deixam margem e o teto de parede é a segunda barreira.

use serde_json::Value;

/// Classe da unidade (decide o teto).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classe {
    /// Tarefa mecânica (renomear, formatar, mover).
    Mecanica,
    /// Escrever/alterar código.
    Codigo,
    /// Revisar/auditar sem escrever.
    Revisao,
}

/// Teto de uma unidade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limites {
    /// Budget em micro-USD (1 USD = 1_000_000).
    pub max_budget_micro_usd: u64,
    /// Máximo de turnos agênticos.
    pub max_turns: u32,
    /// Tempo de parede em segundos.
    pub parede_seg: u64,
}

/// Causa de a unidade ter parado no teto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Causa {
    /// Budget esgotado.
    Budget,
    /// Turnos esgotados.
    Turns,
    /// Tempo de parede esgotado.
    Timeout,
}

/// Limites padrão da classe. **O quê:** tabela fixa [DESIGN] (sem medição própria).
/// **Onde:** `comando::OpcoesIa` quando a config nomeia a classe.
pub fn limites_da_classe(c: Classe) -> Limites {
    match c {
        Classe::Mecanica => {
            Limites { max_budget_micro_usd: 500_000, max_turns: 15, parede_seg: 600 }
        }
        Classe::Codigo => {
            Limites { max_budget_micro_usd: 2_000_000, max_turns: 40, parede_seg: 1_800 }
        }
        Classe::Revisao => {
            Limites { max_budget_micro_usd: 1_000_000, max_turns: 25, parede_seg: 900 }
        }
    }
}

/// Lê o nome da classe (`mecanica|codigo|revisao`, sem acento, qualquer caixa).
/// **Onde:** parsing da config. Nome desconhecido = `None` (o chamador reporta erro).
pub fn classe_de_nome(n: &str) -> Option<Classe> {
    match n.trim().to_ascii_lowercase().as_str() {
        "mecanica" => Some(Classe::Mecanica),
        "codigo" => Some(Classe::Codigo),
        "revisao" => Some(Classe::Revisao),
        _ => None,
    }
}

/// micro-USD como decimal com 6 casas (`1500000` -> `1.500000`), em aritmética inteira.
/// **Onde:** valor de `--max-budget-usd`.
pub fn formatar_usd(micro: u64) -> String {
    format!("{}.{:06}", micro / 1_000_000, micro % 1_000_000)
}

/// Flags de teto. **O quê:** `--max-budget-usd X --max-turns N`; o teto de parede é do
/// supervisor ([`Limites::parede_seg`]). **Onde:** `comando::aplicar`.
pub fn flags(l: &Limites) -> Vec<String> {
    vec![
        "--max-budget-usd".into(),
        formatar_usd(l.max_budget_micro_usd),
        "--max-turns".into(),
        l.max_turns.to_string(),
    ]
}

/// Causa lida do JSON de resultado do `claude -p` (`subtype`).
///
/// **O quê:** `error_max_budget_usd` -> Budget; `error_max_turns` -> Turns; outro = `None`.
/// **Onde:** [`classificar`].
pub fn causa_do_resultado(r: &Value) -> Option<Causa> {
    match r.get("subtype").and_then(Value::as_str)? {
        "error_max_budget_usd" => Some(Causa::Budget),
        "error_max_turns" => Some(Causa::Turns),
        _ => None,
    }
}

/// Classifica o desfecho: o resultado do `claude` tem precedência; senão o relógio.
///
/// **O quê:** `decorrido_seg >= parede_seg` sem causa no JSON = `Timeout`.
/// **Onde:** fim de tentativa, antes de gravar o ledger e decidir retry.
pub fn classificar(res: Option<&Value>, decorrido_seg: u64, l: &Limites) -> Option<Causa> {
    res.and_then(causa_do_resultado)
        .or_else(|| (l.parede_seg > 0 && decorrido_seg >= l.parede_seg).then_some(Causa::Timeout))
}

/// Estado do ledger para a causa (`failed:budget`, `failed:turns`, `failed:timeout`).
/// **Onde:** `ledger::Registro::estado`.
pub fn estado_ledger(c: Causa) -> &'static str {
    match c {
        Causa::Budget => "failed:budget",
        Causa::Turns => "failed:turns",
        Causa::Timeout => "failed:timeout",
    }
}

/// Relançar igual (mesmo brief, mesmo teto) é inútil: estourou o teto, só muda a decomposição
/// ou o teto. **O quê:** sempre `false`. **Onde:** scheduler (a regra fica explícita e testada).
pub fn retentavel_sem_mudanca(_c: Causa) -> bool {
    false
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;

    #[test]
    fn flags_por_classe_e_formato_inteiro() {
        assert_eq!(formatar_usd(0), "0.000000");
        assert_eq!(formatar_usd(1_500_001), "1.500001");
        assert_eq!(formatar_usd(u64::MAX).len(), 21);
        let f = flags(&limites_da_classe(Classe::Codigo));
        assert_eq!(f, ["--max-budget-usd", "2.000000", "--max-turns", "40"]);
        assert_eq!(classe_de_nome(" Revisao "), Some(Classe::Revisao));
        assert_eq!(classe_de_nome("revisão"), None);
    }

    #[test]
    fn unidade_que_estoura_ganha_causa_e_nao_e_retentada() {
        let l = limites_da_classe(Classe::Mecanica);
        let b = json!({"type":"result","subtype":"error_max_budget_usd"});
        let t = json!({"subtype":"error_max_turns"});
        assert_eq!(classificar(Some(&b), 1, &l), Some(Causa::Budget));
        assert_eq!(classificar(Some(&t), 1, &l), Some(Causa::Turns));
        assert_eq!(classificar(None, l.parede_seg, &l), Some(Causa::Timeout));
        assert_eq!(estado_ledger(Causa::Turns), "failed:turns");
        for c in [Causa::Budget, Causa::Turns, Causa::Timeout] {
            assert!(!retentavel_sem_mudanca(c));
        }
    }

    #[test]
    fn sucesso_e_json_hostil_nao_viram_causa() {
        let l = limites_da_classe(Classe::Revisao);
        assert_eq!(classificar(Some(&json!({"subtype":"success"})), 5, &l), None);
        assert_eq!(classificar(Some(&json!({"subtype":7})), 5, &l), None);
        assert_eq!(classificar(Some(&Value::Null), 5, &l), None);
        assert_eq!(classificar(None, u64::MAX, &Limites { parede_seg: 0, ..l }), None);
    }
}
