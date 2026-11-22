//! IA-18 — caminho Messages API para tarefas sem agentic (juiz, resumo, classificação).
//!
//! **STUB: NÃO IMPLEMENTADO DE PROPÓSITO.** Este caminho exige chamada de rede PAGA à Messages
//! API (e à Batch API), fora do `claude -p`/assinatura; o overdev não chama API paga. Aqui ficam
//! só a interface ([`ClienteMensagens`]), os tipos e a decisão pura sync-vs-batch.
//! **O que falta para ligar** (nada disto existe neste repo):
//! 1. um cliente HTTP (nenhuma dep de rede) e leitura segura de `ANTHROPIC_API_KEY`;
//! 2. serialização da requisição com breakpoint de `cache_control` e `effort`, e polling do lote;
//! 3. teste com servidor simulado (aceite do IA-18): requisição leva breakpoint e effort, Batch
//!    quando `urgente = false`, usage mapeado ao ledger por `ledger::uso_de_json`;
//! 4. orçamento/teto e decisão do operador (custo real, ZDR).
//!
//! **Onde:** módulo de julgamento/triagem do orquestrador, fora do `claude -p`.

use super::ledger::Uso;

/// Via de envio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    /// Resposta imediata.
    Sincrona,
    /// Batch API (não urgente; ~50% mais barato, assíncrono).
    Batch,
}

/// Pedido de uma tarefa sem agentic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pedido {
    /// Modelo.
    pub modelo: String,
    /// Effort.
    pub effort: String,
    /// Bloco estático (vai antes do breakpoint de cache).
    pub prefixo_estatico: String,
    /// Parte variável.
    pub entrada: String,
    /// Precisa de resposta agora?
    pub urgente: bool,
}

/// Resposta com o usage para o ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resposta {
    /// Texto devolvido.
    pub texto: String,
    /// Tokens (mapeados ao ledger).
    pub uso: Uso,
}

/// Erro do cliente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroApi {
    /// Caminho não implementado (stub): mensagem acionável.
    NaoImplementado(String),
    /// Falha de transporte ou do servidor.
    Falha(String),
}

/// Cliente da Messages API. **Onde:** implementado por um cliente real FORA deste repo/escopo.
pub trait ClienteMensagens {
    /// Envia um pedido (síncrono ou lote, conforme [`via_para`]).
    fn enviar(&mut self, p: &Pedido) -> Result<Resposta, ErroApi>;
}

/// `Batch` quando não urgente; `Sincrona` quando urgente. **O quê:** a única lógica do IA-18
/// que é pura. **Onde:** cliente real, antes de enviar.
pub fn via_para(p: &Pedido) -> Via {
    if p.urgente {
        Via::Sincrona
    } else {
        Via::Batch
    }
}

/// Cliente-stub: toda chamada devolve `NaoImplementado` e NUNCA toca a rede.
#[derive(Debug, Default, Clone, Copy)]
pub struct ClienteStub;

impl ClienteMensagens for ClienteStub {
    fn enviar(&mut self, _p: &Pedido) -> Result<Resposta, ErroApi> {
        Err(ErroApi::NaoImplementado(
            "IA-18: Messages API paga nao esta ligada; use `claude -p` (assinatura) ou implemente \
             ClienteMensagens fora do overdev (ver docs do modulo)"
                .into(),
        ))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn p(urgente: bool) -> Pedido {
        Pedido {
            modelo: "haiku".into(),
            effort: "low".into(),
            prefixo_estatico: "P".into(),
            entrada: "ç".into(),
            urgente,
        }
    }

    #[test]
    fn via_segue_a_urgencia() {
        assert_eq!(via_para(&p(true)), Via::Sincrona);
        assert_eq!(via_para(&p(false)), Via::Batch);
    }

    #[test]
    fn stub_recusa_sem_rede() {
        let e = ClienteStub.enviar(&p(false)).unwrap_err();
        assert!(matches!(e, ErroApi::NaoImplementado(m) if m.contains("IA-18")));
    }
}
