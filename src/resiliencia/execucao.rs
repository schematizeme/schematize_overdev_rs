//! Efeitos do executor: despachar uma unidade (write-ahead), finalizá-la e decidir o próximo passo
//! (cascata de modelos, re-decomposição por OOM, parkeamento).
//!
//! **O quê:** `impl Executor` — `despachar` grava `Despachada` ANTES de lançar e `Lancada` ANTES de
//! abrir o portão; `finalizar` grava `Concluida`/`Redespachada` e só depois toca ledger, picos e
//! `jaulas.jsonl`. A decisão de modelo/geração é recalculada do estado (determinística), então
//! sobrevive a uma morte entre a falha e o redespacho.
//! **Onde:** chamado por `executor::passo_governo` e `executor::colher`.

use super::despacho::{expandir, resfile_de, Fim, Pedido};
use super::executor::{Ativo, Executor};
use super::modelo::{Evento, Item, StatusItem, Veredito};
use super::proc;
use crate::agentrun::jaulado::{
    anexar, nome_da_unidade, prompt_redecomposicao, reagir, ReacaoAoVeredito, Registro,
};
use crate::otimiza::cascata::{proximo_modelo, Decisao, Modelo, Motivo, Tentativa};
use crate::otimiza::ledger::{self, uso_de_json, Uso};
use schematize_plataforma::jaula::{Classe, Orcamento, Veredito as VeredJaula};
use schematize_plataforma::persistencia::Erro;
use std::path::Path;

/// O que fazer com um item que vai ser (re)despachado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Proximo {
    /// Despachar neste modelo/geração, com uma nota opcional anexada ao brief.
    Despachar { modelo: String, geracao: u32, nota: Option<String> },
    /// Esgotou a escada: parkear com este motivo.
    Parkear(String),
}

/// **O quê:** `Modelo` da cascata a partir do texto (`haiku`/`opus` por substring, senão Sonnet).
/// **Onde:** [`decidir_proximo`]. PURA.
pub fn parse_modelo(s: &str) -> Modelo {
    let s = s.to_ascii_lowercase();
    if s.contains("haiku") {
        Modelo::Haiku
    } else if s.contains("opus") {
        Modelo::Opus
    } else {
        Modelo::Sonnet
    }
}

/// **O quê:** nome curto de um `Modelo` (alias aceito pelo `claude --model`). **Onde:** escalada.
fn nome(m: Modelo) -> &'static str {
    match m {
        Modelo::Haiku => "haiku",
        Modelo::Sonnet => "sonnet",
        Modelo::Opus => "opus",
    }
}

/// **O quê:** motivo de cascata de um veredito que não concluiu. **Onde:** [`decidir_proximo`]. PURA.
fn motivo_de(v: Veredito) -> Option<Motivo> {
    match v {
        Veredito::Concluiu => None,
        Veredito::Falhou => Some(Motivo::GateVermelho),
        Veredito::EstourouMemoria => Some(Motivo::Oom),
        Veredito::Timeout => Some(Motivo::Timeout),
    }
}

/// **O quê:** decide o próximo passo de um item a partir do estado (sem relógio, sem I/O):
/// sem histórico -> despacha; OOM/timeout -> re-decompõe (até `MAX_GERACOES`) ou parkeia;
/// demais falhas -> cascata (reexecuta, re-decompõe, escala, parkeia). **Onde:** `despachar` e
/// `parkear_esgotados`. PURA.
pub fn decidir_proximo(it: &Item, modelo_base: &str) -> Proximo {
    let Some(ult) = it.historico.last() else {
        return Proximo::Despachar { modelo: it.modelo.clone(), geracao: it.geracao, nota: None };
    };
    if matches!(ult.veredito, Veredito::EstourouMemoria | Veredito::Timeout) {
        let v = VeredJaula::EstourouMemoria { pico_mib: it.pico_mib, teto_mib: 0 };
        return match reagir(&v, it.geracao) {
            ReacaoAoVeredito::Redecompor { geracao } => {
                let n = prompt_redecomposicao(geracao, &it.plano.id, it.pico_mib, 0);
                Proximo::Despachar { modelo: it.modelo.clone(), geracao, nota: Some(n) }
            }
            _ => Proximo::Parkear(format!(
                "{:?} recorrente após {} gerações",
                ult.veredito, it.geracao
            )),
        };
    }
    let hist: Vec<Tentativa> = it
        .historico
        .iter()
        .map(|t| Tentativa {
            modelo: parse_modelo(&t.modelo),
            ok: t.veredito == Veredito::Concluiu,
            motivo: motivo_de(t.veredito),
        })
        .collect();
    cascata(it, proximo_modelo(&hist, it.geracao, parse_modelo(modelo_base)))
}

/// **O quê:** traduz a decisão da cascata em [`Proximo`]. **Onde:** [`decidir_proximo`]. PURA.
fn cascata(it: &Item, d: Decisao) -> Proximo {
    let go = |modelo: String, geracao: u32, nota: Option<String>| Proximo::Despachar {
        modelo,
        geracao,
        nota,
    };
    match d {
        Decisao::Iniciar(_) | Decisao::Reexecutar(_) => go(it.modelo.clone(), it.geracao, None),
        Decisao::Redecompor => {
            let nota = "Re-decomponha o item em pedaços menores e execute um por vez.".to_string();
            go(it.modelo.clone(), it.geracao + 1, Some(nota))
        }
        Decisao::Escalar(m, motivo) => go(
            nome(m).to_string(),
            it.geracao,
            Some(format!("Escalada para {} ({motivo:?}).", nome(m))),
        ),
        Decisao::Concluida | Decisao::Parkear | Decisao::ExigeMotivo => {
            Proximo::Parkear("escada de modelos esgotada".to_string())
        }
    }
}

/// **O quê:** `Uso` e custo (micro-USD) lidos da última linha JSON do log (`claude -p
/// --output-format json` imprime `usage`). Sem JSON = uso vazio. **Onde:** `finalizar`.
pub fn uso_do_log(log: &Path, modelo: &str) -> (Uso, u64) {
    let texto = std::fs::read_to_string(log).unwrap_or_default();
    let linha = texto.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
    let v: Option<serde_json::Value> = serde_json::from_str(linha).ok();
    let uso = v.as_ref().and_then(|v| v.get("usage")).map(uso_de_json).unwrap_or_default();
    (uso, ledger::custo_micro_usd(modelo, &uso).unwrap_or(0))
}

/// **O quê:** tag do veredito para `jaulas.jsonl`/ledger. **Onde:** `finalizar`. PURA.
fn tag(v: Veredito) -> &'static str {
    match v {
        Veredito::Concluiu => "concluiu",
        Veredito::Falhou => "falhou",
        Veredito::EstourouMemoria => "oom",
        Veredito::Timeout => "timeout",
    }
}

impl Executor {
    /// **O quê:** parkeia os itens `Falhou` cuja escada esgotou. **Onde:** início de cada tick de governo.
    pub(super) fn parkear_esgotados(&mut self) -> Result<(), Erro> {
        let base = self.d.estado().modelo_base.clone();
        let falhos: Vec<Item> = self
            .d
            .estado()
            .pendentes()
            .into_iter()
            .filter(|i| i.status == StatusItem::Falhou)
            .cloned()
            .collect();
        for it in falhos {
            if let Proximo::Parkear(m) = decidir_proximo(&it, &base) {
                self.parkear(&it, &m)?;
            }
        }
        Ok(())
    }

    /// **O quê:** registra `Parkeada` e marca `- [~]` no checklist (se há marca). Nunca pergunta.
    /// **Onde:** `parkear_esgotados` e `despachar`.
    pub(super) fn parkear(&mut self, it: &Item, motivo: &str) -> Result<(), Erro> {
        self.d
            .registrar(&Evento::Parkeada { id: it.plano.id.clone(), motivo: motivo.to_string() })?;
        self.avisar("parkeado", &format!("{}: {motivo}", it.plano.id))?;
        if let (Some(cl), Some(marca)) = (&self.op.checklist, &it.plano.marca) {
            if let Err(e) = super::checklist::parkear_no_arquivo(cl, marca) {
                eprintln!("overdev: não marquei `- [~]` no checklist: {e}");
            }
        }
        Ok(())
    }

    /// **O quê:** despacha `id`: `Despachada` (journal) -> lança preso no portão -> `Lancada`
    /// (journal) -> abre o portão. **Onde:** `aplicar_decisao`/`garantir_progresso`.
    pub(super) fn despachar(&mut self, id: &str, teto_mib: u64) -> Result<(), Erro> {
        let Some(it) = self.d.estado().itens.get(id).cloned() else { return Ok(()) };
        let base = self.d.estado().modelo_base.clone();
        let (modelo, geracao, nota) = match decidir_proximo(&it, &base) {
            Proximo::Despachar { modelo, geracao, nota } => (modelo, geracao, nota),
            Proximo::Parkear(m) => return self.parkear(&it, &m),
        };
        let n = it.despachos + 1;
        let unidade = if self.op.jaulado {
            nome_da_unidade(
                "ovd",
                &format!("{:08x}-{n}", crate::otimiza::fnv64(id.as_bytes()) as u32),
            )
        } else {
            String::new()
        };
        let resfile = resfile_de(self.d.dir(), id, n);
        self.d.registrar(&Evento::Despachada {
            id: id.to_string(),
            modelo: modelo.clone(),
            geracao,
            despacho: n,
            unidade: unidade.clone(),
            resfile: resfile.to_string_lossy().into_owned(),
            teto_mib,
            inicio: crate::nucleo::util::now_unix(),
        })?;
        let brief = nota.map_or(it.plano.brief.clone(), |n| format!("{}\n\n{n}", it.plano.brief));
        let prefixo = self.d.estado().prefixo.clone();
        let pedido = Pedido {
            id: id.to_string(),
            unidade,
            log: resfile.with_extension("log"),
            resfile,
            argv: expandir(&it.plano.argv, id, &modelo, &prefixo, &brief),
            orcamento: self.op.jaulado.then(|| orcamento(teto_mib)),
            timeout: self.op.timeout,
            env: vec![
                ("OVERDEV_UNIDADE_ID".into(), id.to_string()),
                ("OVERDEV_MODELO".into(), modelo),
                ("OVERDEV_GERACAO".into(), geracao.to_string()),
                ("OVERDEV_DESPACHO".into(), n.to_string()),
            ],
        };
        self.lancar(&it, teto_mib, &pedido)
    }

    /// **O quê:** lança, grava `Lancada` e abre o portão; falha de lançamento vira tentativa
    /// falha (a cascata decide). **Onde:** [`Self::despachar`].
    fn lancar(&mut self, it: &Item, teto_mib: u64, p: &Pedido) -> Result<(), Erro> {
        let id = it.plano.id.clone();
        let mut filho = match self.desp.lancar(p) {
            Ok(f) => f,
            Err(e) => return self.lancamento_falhou(&id, &e),
        };
        let pid = filho.pid().unwrap_or(0);
        let Some(starttime) = proc::starttime(pid) else {
            filho.matar();
            return self.lancamento_falhou(&id, "o processo sumiu antes de ser registrado");
        };
        self.d.registrar(&Evento::Lancada { id: id.clone(), pid, starttime })?;
        if let Err(e) = filho.liberar() {
            filho.matar();
            return self.lancamento_falhou(&id, &e);
        }
        self.ativos.insert(id, Ativo { filho, tipo: it.plano.tipo.clone(), teto_mib });
        Ok(())
    }

    /// **O quê:** conta o lançamento falho como tentativa `Falhou`. **Onde:** [`Self::lancar`].
    fn lancamento_falhou(&mut self, id: &str, erro: &str) -> Result<(), Erro> {
        eprintln!("overdev: lançamento de {id} falhou: {erro}");
        self.avisar("lancamento_falhou", &format!("{id}: {erro}"))?;
        let fim = crate::nucleo::util::now_unix();
        let ev = Evento::Concluida {
            id: id.into(),
            veredito: Veredito::Falhou,
            pico_mib: None,
            custo_micro_usd: 0,
            fim,
        };
        self.d.registrar(&ev).map(|_| ())
    }

    /// **O quê:** fecha uma unidade: `Concluida`/`Redespachada` no journal e SÓ DEPOIS ledger,
    /// picos, `jaulas.jsonl` e limpeza do `resfile`. **Onde:** `executor::colher`.
    pub(super) fn finalizar(&mut self, id: &str, fim: Fim) -> Result<(), Erro> {
        let Some(ativo) = self.ativos.remove(id) else { return Ok(()) };
        let voo = self.d.estado().voo.get(id).cloned();
        let s = match fim {
            Fim::Saida(s) => s,
            Fim::Perdida(motivo) => {
                self.d
                    .registrar(&Evento::Redespachada { id: id.into(), motivo: motivo.clone() })?;
                return self.avisar("unidade_perdida", &format!("{id}: {motivo}"));
            }
        };
        let (modelo, resfile) = voo.map(|u| (u.modelo, u.resfile)).unwrap_or_default();
        let log = Path::new(&resfile).with_extension("log");
        let (uso, custo) = uso_do_log(&log, &modelo);
        let pico = s.pico_mib.or(ativo.filho.pico_mib());
        let ev = Evento::Concluida {
            id: id.into(),
            veredito: s.veredito,
            pico_mib: pico,
            custo_micro_usd: custo,
            fim: crate::nucleo::util::now_unix(),
        };
        self.d.registrar(&ev)?;
        self.depois_de_concluir(id, &ativo, (&modelo, s.veredito, pico), (uso, custo));
        if let Err(e) = std::fs::remove_file(&resfile) {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("overdev: não removi {resfile}: {e}");
            }
        }
        Ok(())
    }

    /// **O quê:** efeitos pós-journal: linha do ledger, pico no histórico e `jaulas.jsonl`.
    /// Falhas vão ao stderr (nunca engolidas); o estado já está salvo. **Onde:** `finalizar`.
    fn depois_de_concluir(
        &mut self,
        id: &str,
        a: &Ativo,
        r: (&str, Veredito, Option<u64>),
        c: (Uso, u64),
    ) {
        let (modelo, v, pico) = r;
        let estado =
            if v == Veredito::Concluiu { "done".to_string() } else { format!("failed:{}", tag(v)) };
        let reg = ledger::Registro {
            unidade: id.into(),
            onda: 0,
            modelo: modelo.into(),
            estado,
            uso: c.0,
            custo_micro_usd: ledger::custo_micro_usd(modelo, &c.0),
        };
        if let Err(e) = ledger::anexar(&self.od.join("ledger.jsonl"), &reg) {
            eprintln!("overdev: ledger: {e}");
        }
        if let Some(p) = pico {
            if let Err(e) = self.ctl.registrar_pico(&a.tipo, p) {
                eprintln!("overdev: picos: {e}");
            }
        }
        let geracao = self.d.estado().itens.get(id).map_or(0, |i| i.geracao);
        let j = Registro {
            unidade: id.into(),
            classe: "-".into(),
            teto_mib: a.teto_mib,
            pico_mib: pico,
            veredito: tag(v).into(),
            geracao,
            timestamp: crate::nucleo::util::now_unix(),
            aviso: None,
        };
        if let Err(e) = anexar(&self.od, &j) {
            eprintln!("jaulas.jsonl: não consegui registrar {id}: {e}");
        }
    }
}

/// **O quê:** orçamento da jaula para `teto_mib` (acima de 2048 só com ADR do plano; se recusar,
/// usa a classe pesada). **Onde:** [`Executor::despachar`].
fn orcamento(teto_mib: u64) -> Orcamento {
    let adr = (teto_mib > 2048).then(|| "ADR-do-plano".to_string());
    Orcamento::novo(teto_mib, adr).unwrap_or_else(|_| Orcamento::da_classe(Classe::Pesado))
}

#[cfg(test)]
mod testes;
