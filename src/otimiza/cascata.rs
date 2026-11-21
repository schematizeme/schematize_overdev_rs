//! Cascata de modelos haiku -> sonnet -> opus (IA-07), como função PURA.
//!
//! **O quê:** dado o histórico de tentativas de uma unidade, decide o próximo passo.
//! **Onde:** scheduler, antes de relançar uma unidade que falhou.
//! Política: falha de infraestrutura (OOM/timeout/budget) relança no MESMO modelo (máx. 3 seguidas);
//! falha de capacidade (gate/loop/schema) tem 2 rodadas no mesmo modelo, depois re-decompõe UMA vez,
//! depois escala; Opus só com motivo registrado. Sucesso nunca escala.

/// Degrau da escada de modelos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Modelo {
    /// Mecânico.
    Haiku,
    /// Código (padrão).
    Sonnet,
    /// Só após falha com motivo.
    Opus,
}

/// Motivo de uma falha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motivo {
    /// Gate vermelho (capacidade).
    GateVermelho,
    /// Loop detectado (capacidade).
    Loop,
    /// Saída fora do schema (capacidade).
    SchemaInvalido,
    /// Morto por memória (infra).
    Oom,
    /// Estourou tempo de parede (infra).
    Timeout,
    /// Estourou budget/turnos (infra).
    Orcamento,
}

impl Motivo {
    /// É falha de capacidade do modelo (e não de infraestrutura)?
    /// **Onde:** separa o que conta para escalar do que só relança.
    pub fn e_capacidade(self) -> bool {
        matches!(self, Motivo::GateVermelho | Motivo::Loop | Motivo::SchemaInvalido)
    }
}

/// Uma tentativa já feita. `motivo == None` com `ok == false` = falha sem motivo registrado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tentativa {
    /// Modelo usado.
    pub modelo: Modelo,
    /// Terminou entregue?
    pub ok: bool,
    /// Motivo da falha.
    pub motivo: Option<Motivo>,
}

/// Decisão da cascata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decisao {
    /// Sem histórico: começar no modelo inicial.
    Iniciar(Modelo),
    /// Última tentativa entregue: nada a fazer.
    Concluida,
    /// Relançar no mesmo modelo.
    Reexecutar(Modelo),
    /// Re-decompor a unidade em partes menores (uma vez).
    Redecompor,
    /// Subir um degrau, com o motivo a registrar no ledger.
    Escalar(Modelo, Motivo),
    /// Opus exigido mas a falha não tem motivo registrado: registrar antes.
    ExigeMotivo,
    /// Esgotou a escada: parkear (`- [~]`).
    Parkear,
}

/// Máx. de rodadas de capacidade no mesmo modelo antes de re-decompor.
pub const MAX_RODADAS_MESMO_MODELO: usize = 2;
/// Máx. de falhas de infra seguidas antes de parkear.
pub const MAX_INFRA_SEGUIDAS: usize = 3;

fn proximo(m: Modelo) -> Option<Modelo> {
    match m {
        Modelo::Haiku => Some(Modelo::Sonnet),
        Modelo::Sonnet => Some(Modelo::Opus),
        Modelo::Opus => None,
    }
}

/// Decide o próximo passo da unidade.
///
/// **O quê:** função pura sobre `historico`; `redecompostas` = vezes que a unidade já foi
/// re-decomposta; `inicial` = modelo da classe da unidade.
/// **Onde:** scheduler (IA-07), antes do relançamento.
pub fn proximo_modelo(historico: &[Tentativa], redecompostas: u32, inicial: Modelo) -> Decisao {
    let Some(ult) = historico.last() else {
        return Decisao::Iniciar(inicial);
    };
    if ult.ok {
        return Decisao::Concluida;
    }
    let seguidas_infra = historico
        .iter()
        .rev()
        .take_while(|t| !t.ok && t.motivo.is_some_and(|m| !m.e_capacidade()))
        .count();
    if ult.motivo.is_some_and(|m| !m.e_capacidade()) {
        return if seguidas_infra >= MAX_INFRA_SEGUIDAS {
            Decisao::Parkear
        } else {
            Decisao::Reexecutar(ult.modelo)
        };
    }
    // Falhas de capacidade (ou sem motivo) no modelo atual, ignorando infra intercalada.
    let rodadas = historico
        .iter()
        .rev()
        .filter(|t| t.modelo == ult.modelo)
        .take_while(|t| !t.ok)
        .filter(|t| t.motivo.is_none_or(Motivo::e_capacidade))
        .count();
    if rodadas < MAX_RODADAS_MESMO_MODELO {
        return Decisao::Reexecutar(ult.modelo);
    }
    if redecompostas == 0 {
        return Decisao::Redecompor;
    }
    match (proximo(ult.modelo), ult.motivo) {
        (None, _) => Decisao::Parkear,
        (Some(Modelo::Opus), None) => Decisao::ExigeMotivo,
        (Some(m), Some(mot)) => Decisao::Escalar(m, mot),
        (Some(m), None) => Decisao::Escalar(m, Motivo::GateVermelho),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn f(m: Modelo, mot: Option<Motivo>) -> Tentativa {
        Tentativa { modelo: m, ok: false, motivo: mot }
    }

    #[test]
    fn vazio_inicia_e_sucesso_nao_escala() {
        assert_eq!(proximo_modelo(&[], 0, Modelo::Haiku), Decisao::Iniciar(Modelo::Haiku));
        let h = [
            f(Modelo::Sonnet, Some(Motivo::Loop)),
            Tentativa { modelo: Modelo::Sonnet, ok: true, motivo: None },
        ];
        assert_eq!(proximo_modelo(&h, 0, Modelo::Sonnet), Decisao::Concluida);
    }

    #[test]
    fn sonnet_sonnet_redecompoe_depois_opus_com_motivo() {
        let g = Some(Motivo::GateVermelho);
        let mut h = vec![f(Modelo::Sonnet, g)];
        assert_eq!(proximo_modelo(&h, 0, Modelo::Sonnet), Decisao::Reexecutar(Modelo::Sonnet));
        h.push(f(Modelo::Sonnet, g));
        assert_eq!(proximo_modelo(&h, 0, Modelo::Sonnet), Decisao::Redecompor);
        assert_eq!(
            proximo_modelo(&h, 1, Modelo::Sonnet),
            Decisao::Escalar(Modelo::Opus, Motivo::GateVermelho)
        );
    }

    #[test]
    fn opus_sem_motivo_exige_motivo_e_esgotado_parkeia() {
        let h = [f(Modelo::Sonnet, None), f(Modelo::Sonnet, None)];
        assert_eq!(proximo_modelo(&h, 1, Modelo::Sonnet), Decisao::ExigeMotivo);
        let h = [f(Modelo::Opus, Some(Motivo::Loop)), f(Modelo::Opus, Some(Motivo::Loop))];
        assert_eq!(proximo_modelo(&h, 1, Modelo::Opus), Decisao::Parkear);
    }

    #[test]
    fn oom_nao_escala_e_tem_teto() {
        let o = Some(Motivo::Oom);
        let h = [f(Modelo::Haiku, o), f(Modelo::Haiku, o)];
        assert_eq!(proximo_modelo(&h, 0, Modelo::Haiku), Decisao::Reexecutar(Modelo::Haiku));
        let h = [f(Modelo::Haiku, o), f(Modelo::Haiku, o), f(Modelo::Haiku, Some(Motivo::Timeout))];
        assert_eq!(proximo_modelo(&h, 0, Modelo::Haiku), Decisao::Parkear);
    }

    #[test]
    fn infra_intercalada_nao_conta_como_rodada() {
        let g = Some(Motivo::SchemaInvalido);
        let h = [f(Modelo::Haiku, g), f(Modelo::Haiku, Some(Motivo::Oom)), f(Modelo::Haiku, g)];
        assert_eq!(
            proximo_modelo(&h, 0, Modelo::Haiku),
            Decisao::Redecompor,
            "2 rodadas de capacidade"
        );
        let h = [f(Modelo::Haiku, Some(Motivo::Oom)), f(Modelo::Haiku, g)];
        assert_eq!(proximo_modelo(&h, 0, Modelo::Haiku), Decisao::Reexecutar(Modelo::Haiku));
    }

    #[test]
    fn haiku_escala_para_sonnet_sem_exigir_motivo() {
        let h = [f(Modelo::Haiku, None), f(Modelo::Haiku, None)];
        assert!(matches!(
            proximo_modelo(&h, 1, Modelo::Haiku),
            Decisao::Escalar(Modelo::Sonnet, _)
        ));
    }
}
