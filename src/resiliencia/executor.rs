//! O executor: despacha as unidades do plano sob governador de memória, com TODA transição
//! gravada no journal ANTES do efeito, e retoma exatamente de onde parou após SIGKILL/OOM.
//!
//! **O quê:** laço único (colher -> governar -> observar -> dormir). Na subida reconcilia as
//! unidades órfãs; depois acompanha cada unidade (própria ou adotada) até o veredito, aplica a
//! cascata (reexecutar/re-decompor/escalar/parkear) e alimenta o histórico de picos.
//! **Onde:** `overdev executar|retomar`. O `schematize-vigia` o relança com `--resume`.
//! Os efeitos de despacho/fim moram em [`super::execucao`].

use super::controle::Controle;
use super::despacho::{Despachante, Filho};
use super::duravel::Duravel;
use super::modelo::{Evento, StatusItem};
use super::oom::proteger_orquestrador;
use super::reconcilia::reconciliar;
use super::vigilancia::Vigilancia;
use crate::agentrun::jaulado::{anexar, Registro};
use schematize_plataforma::memoria::{
    Alvo, ConfigMonitor, Decisao, FonteDir, PedidoAdmissao, UnidadeAtiva,
};
use schematize_plataforma::persistencia::Erro;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Pedido de parada (SIGTERM/SIGINT): o executor para de despachar, grava snapshot e sai SEM matar
/// as unidades (elas são reanexadas na retomada).
pub static PARAR: AtomicBool = AtomicBool::new(false);

/// Opções do executor.
#[derive(Clone, Debug)]
pub struct Opcoes {
    /// Roda cada unidade numa jaula `systemd-run --scope` (teto duro de memória).
    pub jaulado: bool,
    /// Na retomada, mata as unidades órfãs em vez de reanexar.
    pub matar_orfas: bool,
    /// Tempo máximo de parede por unidade.
    pub timeout: Option<Duration>,
    /// `CHECKLIST.md` onde o item parkeado vira `- [~]`.
    pub checklist: Option<PathBuf>,
    /// Passo do laço.
    pub tick: Duration,
    /// Intervalo mínimo entre decisões do governador.
    pub governo_cada: Duration,
    /// Sem nada rodando e sem admissão por este tempo, admite UMA unidade (anti-deadlock).
    pub forcar_admissao_apos: Duration,
    /// Liga o monitor dos cgroups das unidades.
    pub monitorar: bool,
}

impl Default for Opcoes {
    /// **O quê:** sem jaula, reanexa órfãs, tick 20 ms, governo 50 ms, forçar após 30 s.
    /// **Onde:** ponto de partida da CLI e dos testes.
    fn default() -> Self {
        Opcoes {
            jaulado: false,
            matar_orfas: false,
            timeout: None,
            checklist: None,
            tick: Duration::from_millis(20),
            governo_cada: Duration::from_millis(50),
            forcar_admissao_apos: Duration::from_secs(30),
            monitorar: false,
        }
    }
}

/// Unidade viva acompanhada pelo executor.
pub struct Ativo {
    /// O processo (próprio ou adotado).
    pub filho: Box<dyn Filho>,
    /// Tipo de tarefa (chave do histórico de picos).
    pub tipo: String,
    /// Teto (MiB) com que foi admitida.
    pub teto_mib: u64,
    /// O governador pediu que encolhesse/parasse: se morrer com falha, foi pressão de memória.
    pub pedido_encolher: bool,
}

/// Resultado de um run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resumo {
    /// Itens do plano.
    pub total: usize,
    /// Entregues.
    pub entregues: usize,
    /// Parkeados (`- [~]`).
    pub parkeados: usize,
    /// Parou por pedido (SIGTERM) com trabalho restante.
    pub interrompido: bool,
    /// Decisões da reconciliação na subida.
    pub reconciliacao: Vec<String>,
    /// Unidades devolvidas à fila na reconciliação.
    pub devolvidas: usize,
}

impl Resumo {
    /// **O quê:** tudo resolvido (entregue ou parkeado) e sem interrupção. **Onde:** exit code.
    pub fn completo(&self) -> bool {
        !self.interrompido && self.entregues + self.parkeados == self.total
    }
}

/// O executor.
pub struct Executor {
    pub(super) d: Duravel,
    pub(super) ctl: Controle,
    pub(super) desp: Box<dyn Despachante>,
    pub(super) ativos: BTreeMap<String, Ativo>,
    pub(super) op: Opcoes,
    pub(super) od: PathBuf,
    vig: Option<Vigilancia>,
    alvos: Vec<u32>,
    inicio: Instant,
    ultimo_governo: Option<Instant>,
    sem_admissao_desde: Option<Instant>,
    notas: Vec<String>,
    devolvidas: usize,
}

impl Executor {
    /// **O quê:** monta o executor sobre o estado durável. `od` = control-plane (`jaulas.jsonl`,
    /// `ledger.jsonl`). **Onde:** CLI e testes.
    pub fn novo(
        d: Duravel,
        ctl: Controle,
        desp: Box<dyn Despachante>,
        op: Opcoes,
        od: PathBuf,
    ) -> Self {
        Executor {
            d,
            ctl,
            desp,
            ativos: BTreeMap::new(),
            op,
            od,
            vig: None,
            alvos: Vec::new(),
            inicio: Instant::now(),
            ultimo_governo: None,
            sem_admissao_desde: None,
            notas: Vec::new(),
            devolvidas: 0,
        }
    }

    /// **O quê:** o estado durável (leitura). **Onde:** CLI e testes.
    pub fn duravel(&self) -> &Duravel {
        &self.d
    }

    /// **O quê:** roda até tudo ser entregue/parkeado (ou a parada ser pedida). **Onde:** CLI.
    /// Erro de journal sobe: sem registro não há efeito, o vigia relança e a retomada continua.
    pub fn rodar(mut self) -> Result<Resumo, Erro> {
        self.iniciar()?;
        while !PARAR.load(Ordering::Relaxed) {
            self.colher()?;
            if self.d.estado().concluido() && self.ativos.is_empty() {
                break;
            }
            self.passo_governo()?;
            self.observar_monitor()?;
            std::thread::sleep(self.op.tick);
        }
        self.encerrar()
    }

    /// **O quê:** protege o orquestrador (oom_score_adj), liga o monitor e reconcilia órfãs.
    /// **Onde:** início de [`Self::rodar`].
    fn iniciar(&mut self) -> Result<(), Erro> {
        let t = proteger_orquestrador();
        self.d.registrar(&Evento::Observacao { tipo: "oom_adj".into(), detalhe: t })?;
        if self.op.monitorar {
            match Vigilancia::iniciar(&ConfigMonitor::default(), Arc::new(FonteDir::real())) {
                Ok(v) => self.vig = Some(v),
                Err(e) => self.avisar("monitor_indisponivel", &e)?,
            }
        }
        let r = reconciliar(&mut self.d, self.op.matar_orfas)?;
        self.devolvidas = r.devolvidas;
        self.notas = r.notas;
        for (id, filho) in r.adotados {
            let it = self.d.estado().itens.get(&id).cloned();
            let teto = self.d.estado().voo.get(&id).map_or(0, |u| u.teto_mib);
            let tipo = it.map(|i| i.plano.tipo).unwrap_or_default();
            self.ativos.insert(id, Ativo { filho, tipo, teto_mib: teto, pedido_encolher: false });
        }
        Ok(())
    }

    /// **O quê:** pergunta a cada unidade viva se terminou e finaliza as que terminaram.
    /// **Onde:** laço de [`Self::rodar`].
    fn colher(&mut self) -> Result<(), Erro> {
        let ids: Vec<String> = self.ativos.keys().cloned().collect();
        for id in ids {
            let fim = self.ativos.get_mut(&id).and_then(|a| a.filho.tentar_esperar());
            if let Some(fim) = fim {
                self.finalizar(&id, fim)?;
                self.ultimo_governo = None;
            }
        }
        Ok(())
    }

    /// **O quê:** unidades ativas para o governador (memória atual lida agora). **Onde:** governo.
    fn ativas(&mut self) -> Vec<UnidadeAtiva> {
        let mut v = Vec::new();
        for (id, a) in self.ativos.iter_mut() {
            let atual_mib = a.filho.atual_mib().unwrap_or(0);
            v.push(UnidadeAtiva {
                id: id.clone(),
                teto_mib: a.teto_mib,
                atual_mib,
                mib_por_s: 0.0,
            });
        }
        v
    }

    /// **O quê:** fila de admissão (itens pendentes na ordem do plano, com o teto de cada um).
    /// **Onde:** governo.
    fn fila(&self, agora_ms: u64) -> Vec<PedidoAdmissao> {
        let max_host = self.ctl.max_host_mib(agora_ms);
        let pend = self.d.estado().pendentes();
        let pedido = |i: &super::modelo::Item| PedidoAdmissao {
            id: i.plano.id.clone(),
            teto_mib: self.ctl.teto_para(&i.plano.tipo, i.plano.teto_mib, max_host),
        };
        pend.into_iter().filter(|i| !self.ativos.contains_key(&i.plano.id)).map(pedido).collect()
    }

    /// **O quê:** um tick do governador: parkeia esgotados, decide admissão/estrangulamento/pedidos
    /// e despacha. **Onde:** laço de [`Self::rodar`].
    fn passo_governo(&mut self) -> Result<(), Erro> {
        if self.ultimo_governo.is_some_and(|t| t.elapsed() < self.op.governo_cada) {
            return Ok(());
        }
        self.ultimo_governo = Some(Instant::now());
        self.parkear_esgotados()?;
        let agora_ms = self.inicio.elapsed().as_millis() as u64;
        let (ativas, fila) = (self.ativas(), self.fila(agora_ms));
        let (decisoes, _) = self.ctl.decidir(agora_ms, &ativas, &fila);
        let mut admitiu = false;
        for dec in decisoes {
            admitiu |= self.aplicar_decisao(dec, &fila)?;
        }
        self.garantir_progresso(admitiu, &fila)
    }

    /// **O quê:** efetiva uma decisão do governador (registrando ANTES); devolve se admitiu.
    /// **Onde:** [`Self::passo_governo`].
    fn aplicar_decisao(&mut self, dec: Decisao, fila: &[PedidoAdmissao]) -> Result<bool, Erro> {
        match dec {
            Decisao::Admitir { quantas } => {
                for p in fila.iter().take(quantas) {
                    self.despachar(&p.id, p.teto_mib)?;
                }
                return Ok(quantas > 0);
            }
            Decisao::Estrangular { paralelismo_max } => {
                self.avisar("estrangular", &format!("paralelismo_max={paralelismo_max}"))?
            }
            Decisao::Liberar => self.avisar("liberar", "fim do aperto")?,
            Decisao::PedirEncolher { id, motivo } => {
                self.pedir(&id, "encolher", &format!("{motivo:?}"))?
            }
            Decisao::PedirParar { id, motivo } => {
                self.pedir(&id, "parar", &format!("{motivo:?}"))?
            }
            Decisao::DeixarTetoAgir { id } => self.avisar("teto_duro", &id)?,
            Decisao::Manter => {}
        }
        Ok(false)
    }

    /// **O quê:** registra e envia o pedido gracioso (SIGTERM) à unidade. **Onde:** `aplicar_decisao`.
    fn pedir(&mut self, id: &str, o_que: &str, motivo: &str) -> Result<(), Erro> {
        self.avisar(&format!("pedir_{o_que}"), &format!("{id} ({motivo})"))?;
        if let Some(a) = self.ativos.get_mut(id) {
            a.pedido_encolher = true;
            a.filho.pedir_parar();
        }
        Ok(())
    }

    /// **O quê:** anti-deadlock: nada rodando, fila não vazia e sem admissão por muito tempo
    /// admite UMA unidade (o teto duro da jaula protege o host). **Onde:** `passo_governo`.
    fn garantir_progresso(&mut self, admitiu: bool, fila: &[PedidoAdmissao]) -> Result<(), Erro> {
        if admitiu || !self.ativos.is_empty() || fila.is_empty() {
            self.sem_admissao_desde = None;
            return Ok(());
        }
        let desde = *self.sem_admissao_desde.get_or_insert_with(Instant::now);
        if desde.elapsed() >= self.op.forcar_admissao_apos {
            self.sem_admissao_desde = None;
            self.avisar(
                "admissao_forcada",
                &format!("{}: host apertado há muito tempo", fila[0].id),
            )?;
            self.despachar(&fila[0].id, fila[0].teto_mib)?;
        }
        Ok(())
    }

    /// **O quê:** registra a observação no journal e a linha `aviso` em `jaulas.jsonl`.
    /// **Onde:** governador, monitor, parkeamento. Falha do jsonl vai ao stderr (nunca engolida).
    pub(super) fn avisar(&mut self, tipo: &str, detalhe: &str) -> Result<(), Erro> {
        self.d.registrar(&Evento::Observacao { tipo: tipo.into(), detalhe: detalhe.into() })?;
        let r = Registro {
            unidade: "orquestrador".into(),
            classe: "-".into(),
            teto_mib: 0,
            pico_mib: None,
            veredito: "aviso".into(),
            geracao: 0,
            timestamp: crate::nucleo::util::now_unix(),
            aviso: Some(format!("{tipo}: {detalhe}")),
        };
        if let Err(e) = anexar(&self.od, &r) {
            eprintln!("jaulas.jsonl: não consegui registrar o aviso {tipo}: {e}");
        }
        Ok(())
    }

    /// **O quê:** atualiza os alvos do monitor e traduz os eventos em avisos. **Onde:** laço.
    fn observar_monitor(&mut self) -> Result<(), Erro> {
        let Some(v) = &self.vig else { return Ok(()) };
        let pids: Vec<u32> = self.ativos.values().filter_map(|a| a.filho.pid()).collect();
        if pids != self.alvos {
            let alvos =
                self.ativos.iter().filter_map(|(id, a)| Some(Alvo::pid(id, a.filho.pid()?)));
            v.alvos(alvos.collect());
            self.alvos = pids;
        }
        let eventos = v.drenar();
        for (tipo, detalhe) in eventos {
            self.avisar(&format!("monitor_{tipo}"), &detalhe)?;
        }
        Ok(())
    }

    /// **O quê:** snapshot final, para o monitor e devolve o resumo. **Onde:** fim de `rodar`.
    fn encerrar(mut self) -> Result<Resumo, Erro> {
        self.d.snapshot();
        if let Some(v) = self.vig.take() {
            if let Err(e) = v.parar() {
                eprintln!("overdev: monitor: {e}");
            }
        }
        let e = self.d.estado();
        let conta = |s: StatusItem| e.itens.values().filter(|i| i.status == s).count();
        Ok(Resumo {
            total: e.itens.len(),
            entregues: conta(StatusItem::Entregue),
            parkeados: conta(StatusItem::Parkeado),
            interrompido: PARAR.load(Ordering::Relaxed) && !e.concluido(),
            reconciliacao: std::mem::take(&mut self.notas),
            devolvidas: self.devolvidas,
        })
    }
}
