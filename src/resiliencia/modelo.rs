//! Modelo de estado durável do overdev ([`EstadoOverdev`]) e seus eventos idempotentes.
//!
//! **O quê:** o que precisa sobreviver a SIGKILL/OOM: itens do checklist e status (com
//! `Entregue` que nunca regride e `Parkeado`), unidades em voo (id, modelo, geração, tentativa,
//! pid + starttime, unidade systemd, início), vereditos, cascata de modelos e custo acumulado.
//! **Onde:** implementa `persistencia::Estado`; a `Maquina<EstadoOverdev>` faz
//! `estado = snapshot + replay`. `aplicar` é PURO e determinístico (sem relógio, sem I/O): todo
//! tempo entra no evento. Codificação: JSON (serde) — legível no diagnóstico e tolerante a campos novos.

use schematize_plataforma::persistencia::Estado;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Versão do esquema do snapshot; mudou o formato, mude este número.
pub const VERSAO_ESQUEMA: u32 = 1;
/// Quantas observações recentes o snapshot guarda (limita o tamanho do estado).
const MAX_OBSERVACOES: usize = 32;

/// Status de um item do checklist.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatusItem {
    /// Ainda não despachado (ou devolvido à fila após perder a unidade).
    Pendente,
    /// Há uma unidade em voo para ele.
    EmVoo,
    /// Entregue: NUNCA regride.
    Entregue,
    /// Última tentativa falhou (aguarda a decisão da cascata).
    Falhou,
    /// Esgotou a escada: `- [~]`, não se pergunta nada.
    Parkeado,
}

/// Veredito de uma unidade.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Veredito {
    /// Terminou com sucesso.
    Concluiu,
    /// Terminou com erro.
    Falhou,
    /// Morta por estouro de memória (OOM do cgroup).
    EstourouMemoria,
    /// Estourou o tempo de parede.
    Timeout,
}

/// Uma tentativa já feita num item (modelo usado + como terminou).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TentativaReg {
    /// Modelo usado (`haiku`/`sonnet`/`opus` ou o id completo).
    pub modelo: String,
    /// Como terminou.
    pub veredito: Veredito,
}

/// Item do plano, como entra no evento `Plano`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemPlano {
    /// Id estável do item.
    pub id: String,
    /// Brief completo (guardado para re-despachar sem o arquivo de plano).
    pub brief: String,
    /// Tipo de tarefa (chave do histórico de picos de memória).
    #[serde(default = "tipo_padrao")]
    pub tipo: String,
    /// Teto de memória (MiB) por ADR; sobrepõe a recomendação do histórico.
    #[serde(default)]
    pub teto_mib: Option<u64>,
    /// Trecho da linha do `CHECKLIST.md` (para marcar `- [~]` ao parkear).
    #[serde(default)]
    pub marca: Option<String>,
    /// Comando da unidade (argv). Vazio = o despachante padrão decide.
    #[serde(default)]
    pub argv: Vec<String>,
}

/// **O quê:** tipo de tarefa quando o plano não diz. **Onde:** `serde(default)` de `ItemPlano`.
fn tipo_padrao() -> String {
    "geral".to_string()
}

/// Item no estado: o plano + o que aconteceu com ele.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// Dados do plano.
    pub plano: ItemPlano,
    /// Hash FNV-1a do brief (idempotência: brief mudou = item novo).
    pub brief_hash: u64,
    /// Status atual.
    pub status: StatusItem,
    /// Tentativas que terminaram sem entregar.
    pub tentativas: u32,
    /// Geração de re-decomposição.
    pub geracao: u32,
    /// Quantos despachos já foram feitos (identifica o `resfile` de cada um).
    pub despachos: u32,
    /// Quantas vezes a unidade foi perdida e devolvida à fila (não conta como tentativa).
    pub redespachos: u32,
    /// Último modelo usado.
    pub modelo: String,
    /// Cascata: tentativas anteriores.
    pub historico: Vec<TentativaReg>,
    /// Maior pico de memória observado (MiB).
    pub pico_mib: Option<u64>,
    /// Motivo do parkeamento.
    pub motivo_park: Option<String>,
}

/// Unidade em execução (o que a reconciliação precisa para achar órfãs).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnidadeVoo {
    /// Id do item.
    pub id: String,
    /// Modelo.
    pub modelo: String,
    /// Geração de re-decomposição.
    pub geracao: u32,
    /// Número do despacho (1, 2, ...).
    pub despacho: u32,
    /// Nome da unidade systemd (scope), se jaulada.
    pub unidade: String,
    /// Arquivo onde o invólucro grava o código de saída.
    pub resfile: String,
    /// Teto de memória (MiB) com que foi despachada.
    pub teto_mib: u64,
    /// Início (Unix s).
    pub inicio: u64,
    /// Pid do líder (preenchido em `Lancada`).
    pub pid: Option<u32>,
    /// `starttime` do pid em `/proc` (verifica PID reciclado).
    pub starttime: Option<u64>,
}

/// Evento do domínio. Todo evento é idempotente (o seq garante replay único).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum Evento {
    /// Define/atualiza o plano (idempotente: mesmo brief mantém o status).
    Plano { prefixo: String, modelo_base: String, itens: Vec<ItemPlano> },
    /// Despacho de uma unidade (write-ahead: ANTES de lançar o processo).
    Despachada {
        id: String,
        modelo: String,
        geracao: u32,
        despacho: u32,
        unidade: String,
        resfile: String,
        teto_mib: u64,
        inicio: u64,
    },
    /// O processo da unidade existe (pid + starttime).
    Lancada { id: String, pid: u32, starttime: u64 },
    /// Fim da unidade com veredito, pico e custo.
    Concluida {
        id: String,
        veredito: Veredito,
        pico_mib: Option<u64>,
        custo_micro_usd: u64,
        fim: u64,
    },
    /// Esgotou a escada: parkeia o item.
    Parkeada { id: String, motivo: String },
    /// A unidade em voo se perdeu (morta/órfã): volta à fila sem contar tentativa.
    Redespachada { id: String, motivo: String },
    /// Observação operacional (governador, monitor, reconciliação, oom_score_adj).
    Observacao { tipo: String, detalhe: String },
    /// Relançamento do agente pelo supervisor.
    Relancamento { n: u32 },
    /// Unidade jaulada apurada pelo supervisor.
    JaulaApurada { unidade: String, veredito: String, pico_mib: Option<u64>, geracao: u32 },
}

/// O estado inteiro do overdev.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EstadoOverdev {
    /// Itens por id.
    pub itens: BTreeMap<String, Item>,
    /// Ordem do plano (ids).
    pub ordem: Vec<String>,
    /// Unidades em voo por id do item.
    pub voo: BTreeMap<String, UnidadeVoo>,
    /// Prefixo de prompt estável (reaproveitado em todo despacho: cache de prompt).
    pub prefixo: String,
    /// Modelo inicial da escada.
    pub modelo_base: String,
    /// Custo acumulado (micro-USD) do ledger.
    pub custo_micro_usd: u64,
    /// Relançamentos do supervisor.
    pub relancamentos: u32,
    /// Jaulas apuradas / mortas por OOM.
    pub jaulas_apuradas: u32,
    /// Mortes por OOM.
    pub ooms: u32,
    /// Últimas observações.
    pub observacoes: Vec<String>,
}

/// **O quê:** hash FNV-1a do brief. **Onde:** idempotência do evento `Plano`.
pub fn hash_brief(brief: &str) -> u64 {
    crate::otimiza::fnv64(brief.as_bytes())
}

impl Item {
    /// **O quê:** item novo `Pendente` a partir do plano. **Onde:** evento `Plano`.
    fn novo(p: &ItemPlano, modelo_base: &str) -> Item {
        Item {
            brief_hash: hash_brief(&p.brief),
            plano: p.clone(),
            status: StatusItem::Pendente,
            tentativas: 0,
            geracao: 0,
            despachos: 0,
            redespachos: 0,
            modelo: modelo_base.to_string(),
            historico: Vec::new(),
            pico_mib: None,
            motivo_park: None,
        }
    }

    /// **O quê:** `true` se o item ainda aceita transições (não é `Entregue` nem `Parkeado`).
    /// **Onde:** `aplicar` — `Entregue` nunca regride.
    fn aberto(&self) -> bool {
        !matches!(self.status, StatusItem::Entregue | StatusItem::Parkeado)
    }
}

impl EstadoOverdev {
    /// **O quê:** aplica o evento `Plano`. **Onde:** `aplicar`. Mesmo hash mantém; hash novo recria.
    fn plano(&mut self, prefixo: &str, modelo_base: &str, itens: &[ItemPlano]) {
        if !prefixo.is_empty() {
            self.prefixo = prefixo.to_string();
        }
        if !modelo_base.is_empty() {
            self.modelo_base = modelo_base.to_string();
        }
        for p in itens {
            let h = hash_brief(&p.brief);
            match self.itens.get(&p.id) {
                Some(i) if i.brief_hash == h => continue,
                Some(_) => {
                    self.voo.remove(&p.id);
                }
                None => self.ordem.push(p.id.clone()),
            }
            self.itens.insert(p.id.clone(), Item::novo(p, &self.modelo_base));
        }
    }

    /// **O quê:** aplica `Despachada`. **Onde:** `aplicar`. Ignora item inexistente/encerrado.
    fn despachada(&mut self, ev: &Evento) {
        let Evento::Despachada {
            id,
            modelo,
            geracao,
            despacho,
            unidade,
            resfile,
            teto_mib,
            inicio,
        } = ev
        else {
            return;
        };
        let Some(i) = self.itens.get_mut(id).filter(|i| i.aberto()) else { return };
        i.status = StatusItem::EmVoo;
        i.modelo = modelo.clone();
        i.geracao = *geracao;
        i.despachos = *despacho;
        self.voo.insert(
            id.clone(),
            UnidadeVoo {
                id: id.clone(),
                modelo: modelo.clone(),
                geracao: *geracao,
                despacho: *despacho,
                unidade: unidade.clone(),
                resfile: resfile.clone(),
                teto_mib: *teto_mib,
                inicio: *inicio,
                pid: None,
                starttime: None,
            },
        );
    }

    /// **O quê:** aplica `Concluida`. **Onde:** `aplicar`. `Entregue` não regride; só soma custo.
    fn concluida(&mut self, id: &str, v: Veredito, pico: Option<u64>, custo: u64) {
        self.custo_micro_usd = self.custo_micro_usd.saturating_add(custo);
        self.voo.remove(id);
        let Some(i) = self.itens.get_mut(id) else { return };
        i.pico_mib = i.pico_mib.max(pico);
        if !i.aberto() {
            return;
        }
        i.historico.push(TentativaReg { modelo: i.modelo.clone(), veredito: v });
        if v == Veredito::Concluiu {
            i.status = StatusItem::Entregue;
        } else {
            i.tentativas += 1;
            i.status = StatusItem::Falhou;
        }
    }

    /// **O quê:** guarda a observação (anel de [`MAX_OBSERVACOES`]). **Onde:** `aplicar`.
    fn observar(&mut self, linha: String) {
        self.observacoes.push(linha);
        if self.observacoes.len() > MAX_OBSERVACOES {
            self.observacoes.remove(0);
        }
    }

    /// **O quê:** aplica `Lancada`/`Parkeada`/`Redespachada`. **Onde:** `aplicar`.
    fn marca_unidade(&mut self, ev: &Evento) {
        match ev {
            Evento::Lancada { id, pid, starttime } => {
                if let Some(u) = self.voo.get_mut(id) {
                    u.pid = Some(*pid);
                    u.starttime = Some(*starttime);
                }
            }
            Evento::Parkeada { id, motivo } => {
                self.voo.remove(id);
                if let Some(i) = self.itens.get_mut(id).filter(|i| i.aberto()) {
                    i.status = StatusItem::Parkeado;
                    i.motivo_park = Some(motivo.clone());
                }
            }
            Evento::Redespachada { id, .. } => {
                self.voo.remove(id);
                if let Some(i) = self.itens.get_mut(id).filter(|i| i.aberto()) {
                    i.status = StatusItem::Pendente;
                    i.redespachos += 1;
                }
            }
            _ => {}
        }
    }

    /// **O quê:** itens a despachar, na ordem do plano (`Pendente`/`Falhou`). **Onde:** executor.
    pub fn pendentes(&self) -> Vec<&Item> {
        let abertos = [StatusItem::Pendente, StatusItem::Falhou];
        let achados = self.ordem.iter().filter_map(|id| self.itens.get(id));
        achados.filter(|i| abertos.contains(&i.status)).collect()
    }

    /// **O quê:** nada mais a fazer (tudo `Entregue` ou `Parkeado`, nada em voo). **Onde:** fim do run.
    pub fn concluido(&self) -> bool {
        self.voo.is_empty() && self.itens.values().all(|i| !i.aberto())
    }

    /// **O quê:** projeção LÓGICA do resultado (sem pid/tempo/redespachos): uma linha por item.
    /// **Onde:** compara a execução com kills contra a sem kill (deve ser idêntica).
    pub fn resumo_logico(&self) -> Vec<String> {
        let mut v = Vec::new();
        for id in &self.ordem {
            let Some(i) = self.itens.get(id) else { continue };
            let fim = i.historico.last().map(|t| format!("{:?}", t.veredito)).unwrap_or_default();
            let linha = format!(
                "{id}|{:?}|t={}|g={}|m={}|h={:016x}|{fim}",
                i.status, i.tentativas, i.geracao, i.modelo, i.brief_hash
            );
            v.push(linha);
        }
        v
    }
}

impl Estado for EstadoOverdev {
    type Evento = Evento;
    const VERSAO_ESQUEMA: u32 = VERSAO_ESQUEMA;

    /// **O quê:** estado vazio. **Onde:** antes de qualquer evento.
    fn inicial() -> Self {
        EstadoOverdev::default()
    }

    /// **O quê:** aplica um evento (determinístico, sem I/O). **Onde:** `Maquina::registrar` e replay.
    fn aplicar(&mut self, ev: &Evento) {
        match ev {
            Evento::Plano { prefixo, modelo_base, itens } => {
                self.plano(prefixo, modelo_base, itens)
            }
            Evento::Despachada { .. } => self.despachada(ev),
            Evento::Concluida { id, veredito, pico_mib, custo_micro_usd, .. } => {
                self.concluida(id, *veredito, *pico_mib, *custo_micro_usd)
            }
            Evento::Observacao { tipo, detalhe } => self.observar(format!("{tipo}: {detalhe}")),
            Evento::Relancamento { n } => self.relancamentos = self.relancamentos.max(*n),
            Evento::JaulaApurada { veredito, .. } => {
                self.jaulas_apuradas += 1;
                self.ooms += u32::from(veredito == "oom");
            }
            _ => self.marca_unidade(ev),
        }
    }

    /// **O quê:** snapshot em JSON. **Onde:** `Maquina::snapshot`.
    fn codificar(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// **O quê:** lê o snapshot. **Onde:** `Maquina::abrir`. Erro vira texto (nunca panic).
    fn decodificar(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(bytes).map_err(|e| format!("estado do overdev ilegível: {e}"))
    }

    /// **O quê:** evento em JSON. **Onde:** payload do journal.
    fn codificar_evento(e: &Evento) -> Vec<u8> {
        serde_json::to_vec(e).unwrap_or_default()
    }

    /// **O quê:** lê um evento. **Onde:** replay.
    fn decodificar_evento(bytes: &[u8]) -> Result<Evento, String> {
        serde_json::from_slice(bytes).map_err(|e| format!("evento ilegível: {e}"))
    }
}

#[cfg(test)]
mod testes;
