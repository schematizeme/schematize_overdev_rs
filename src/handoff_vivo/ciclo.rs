//! O motor do handoff vivo: tick mecânico (hash, só reescreve se mudou) e camada Claude assíncrona.
//!
//! **O quê:** [`Motor`] segura o lock de escritor único, o estado persistido (`estado.hv`, 3
//! gerações) e a conversa com a thread do Claude. [`Motor::tick_mecanico`] coleta + renderiza +
//! grava `ULTIMO-context.md`/`ULTIMO-checklist.md` (3 gerações cada) e os datados
//! `<carimbo>-context.md`/`-checklist.md`; [`Motor::tick`] soma a camada Claude (dispara em thread,
//! nunca bloqueia, não empilha, backoff persistido).
//! **Onde:** [`super::laco`] (loop `run`), [`super::comandos`] (`agora`) e os testes.

use super::atomico::{escrever_atomico, limpar_tmp, Armazem};
use super::claude::{backoff_s, construir_prompt, executar, ResClaude};
use super::coleta::coletar;
use super::config::Config;
use super::formato::{checksum, codificar};
use super::render::{checklist, com_hora, contexto, Notas};
use super::trava::Trava;
use crate::otimiza::ledger;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;

/// Relógio injetável (segundos Unix) — testes o fazem andar para trás.
pub type Relogio = Arc<dyn Fn() -> u64 + Send + Sync>;

/// **O quê:** relógio de parede real. **Onde:** `executar`/`run`.
pub fn relogio_real() -> Relogio {
    Arc::new(crate::nucleo::util::now_unix)
}

/// Estado persistido em `.schematize/handoff-vivo/estado.hv`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EstadoVivo {
    /// Hash do corpo estável do context gravado.
    pub hash_context: String,
    /// Idem do checklist.
    pub hash_checklist: String,
    /// `seq` do `ULTIMO-context.md` gravado.
    pub seq_context: u64,
    /// `seq` do `ULTIMO-checklist.md` gravado.
    pub seq_checklist: u64,
    /// Últimas notas do Claude.
    pub notas: Option<Notas>,
    /// Hash do estado mecânico quando o último handoff do Claude foi pedido com sucesso.
    pub hash_no_claude: Option<String>,
    /// Não chamar o Claude antes deste instante (unix s).
    pub proximo_claude_em: u64,
    /// Falhas seguidas da camada Claude.
    pub falhas: u32,
    /// Motivo da última falha.
    pub ultimo_erro: Option<String>,
    /// Última chamada (unix s), para o cache do ledger.
    pub ultimo_claude_em: Option<u64>,
}

/// O que um tick mecânico fez.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tick {
    /// Gravou context e/ou checklist.
    pub escreveu: bool,
    /// `seq` atual do context.
    pub seq_context: u64,
    /// `seq` atual do checklist.
    pub seq_checklist: u64,
}

/// Um dos dois arquivos de saída (context ou checklist) com seu hash.
struct Alvo {
    arm: Armazem,
    sufixo: &'static str,
    hash: String,
    seq: u64,
}

impl Alvo {
    /// **O quê:** grava `estavel` (com hora) no ponteiro de 3 gerações e no datado, SE o hash mudou
    /// ou o ponteiro não está são. `Ok(true)` se gravou. **Onde:** [`Motor::tick_mecanico`].
    fn gravar(
        &mut self,
        dir: &Path,
        stamp: &str,
        quando: &str,
        estavel: &str,
        forcar: bool,
    ) -> Result<bool, String> {
        let h = format!("{:016x}", checksum(estavel.as_bytes()));
        let seq = self.seq;
        let saudavel = self.arm.melhor().is_some_and(|(i, d)| i == 0 && d.seq == seq);
        if !forcar && h == self.hash && saudavel {
            return Ok(false);
        }
        let corpo = com_hora(quando, estavel);
        let nova = self.arm.proxima_seq();
        self.arm.gravar_seq(nova, corpo.as_bytes())?;
        let datado = dir.join(format!("{stamp}-{}.md", self.sufixo));
        escrever_atomico(&datado, &codificar(nova, corpo.as_bytes()))?;
        self.hash = h;
        self.seq = nova;
        Ok(true)
    }
}

/// Camada Claude em voo.
struct EmVoo {
    rx: Receiver<ResClaude>,
    hash: String,
}

/// O motor (único escritor).
pub struct Motor {
    cfg: Config,
    relogio: Relogio,
    trava: Trava,
    st: EstadoVivo,
    ctx: Alvo,
    chk: Alvo,
    arm_est: Armazem,
    json_salvo: String,
    stamp: String,
    hash_estado: String,
    resumo: String,
    em_voo: Option<EmVoo>,
}

/// **O quê:** carimbo `YYYY-MM-DD-HH-MM-SS` (hora local). **Onde:** nomes datados.
pub fn ts_arquivo(secs: u64) -> String {
    ts_fmt(secs, "%Y-%m-%d-%H-%M-%S")
}

/// **O quê:** `YYYY-MM-DD HH:MM` local. **Onde:** título das notas e linha de hora.
pub fn ts_humano(secs: u64) -> String {
    ts_fmt(secs, "%Y-%m-%d %H:%M")
}

fn ts_fmt(secs: u64, f: &str) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(i64::try_from(secs).unwrap_or(0), 0)
        .single()
        .map_or_else(|| format!("t{secs}"), |d| d.format(f).to_string())
}

impl Motor {
    /// **O quê:** adquire o lock, limpa `.tmp` órfãos e carrega o estado (melhor geração válida).
    /// **Onde:** `run`/`agora`/testes. `Err` se há outro escritor vivo ou E/S falhou.
    pub fn abrir(cfg: Config, relogio: Relogio) -> Result<Motor, String> {
        let op = cfg.dir_operacional();
        let trava = Trava::adquirir(&op.join("escritor.lock")).map_err(|e| e.to_string())?;
        let dctx = cfg.dir_context();
        std::fs::create_dir_all(&dctx).map_err(|e| format!("criar {}: {e}", dctx.display()))?;
        let arm_est = Armazem::novo(&op, "estado.hv");
        let st: EstadoVivo = arm_est
            .melhor()
            .and_then(|(_, d)| serde_json::from_slice(&d.corpo).ok())
            .unwrap_or_default();
        let alvo = |sufixo: &'static str, hash: &str, seq: u64| Alvo {
            arm: Armazem::novo(&dctx, &format!("ULTIMO-{sufixo}.md")),
            sufixo,
            hash: hash.to_string(),
            seq,
        };
        let (ctx, chk) = (
            alvo("context", &st.hash_context, st.seq_context),
            alvo("checklist", &st.hash_checklist, st.seq_checklist),
        );
        let m = Motor {
            cfg,
            relogio,
            trava,
            ctx,
            chk,
            arm_est,
            st,
            json_salvo: String::new(),
            stamp: String::new(),
            hash_estado: String::new(),
            resumo: String::new(),
            em_voo: None,
        };
        for d in [&dctx, &op] {
            if let Err(e) = limpar_tmp(d) {
                m.registrar_erro(&format!("limpeza de .tmp: {e}"));
            }
        }
        if let Some(o) = m.trava.orfao_retomado {
            m.registrar_erro(&format!("lock órfão do pid {} retomado", o.pid));
        }
        Ok(m)
    }

    /// **O quê:** estado persistido atual (cópia de leitura). **Onde:** testes e `status`.
    pub fn estado(&self) -> &EstadoVivo {
        &self.st
    }

    /// **O quê:** a camada Claude está em voo? **Onde:** `run` (não empilha) e testes.
    pub fn claude_em_voo(&self) -> bool {
        self.em_voo.is_some()
    }

    /// **O quê:** acrescenta uma linha datada a `erros.log` (rotaciona em 256 KiB); falha de E/S
    /// vai ao stderr. **Onde:** qualquer falha registrável.
    pub fn registrar_erro(&self, msg: &str) {
        let log = self.cfg.dir_operacional().join("erros.log");
        if std::fs::metadata(&log).map(|m| m.len() > 256 * 1024).unwrap_or(false) {
            if let Err(e) = std::fs::rename(&log, log.with_extension("log.1")) {
                eprintln!("handoff-vivo: não rotacionei erros.log: {e}");
            }
        }
        let linha = format!("[{}] {}\n", ts_humano((self.relogio)()), msg.replace('\n', " "));
        let r = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log)
            .and_then(|mut f| std::io::Write::write_all(&mut f, linha.as_bytes()));
        if let Err(e) = r {
            eprintln!("handoff-vivo: {msg} (e não gravei erros.log: {e})");
        }
    }

    /// **O quê:** carimbo único da sessão (não reaproveita nome existente, mesmo com relógio
    /// voltando). **Onde:** [`Self::tick_mecanico`].
    fn garantir_stamp(&mut self, agora: u64) {
        if !self.stamp.is_empty() {
            return;
        }
        let base = ts_arquivo(agora);
        let d = self.cfg.dir_context();
        let mut cand = base.clone();
        let mut n = 1;
        while d.join(format!("{cand}-context.md")).exists()
            || d.join(format!("{cand}-checklist.md")).exists()
        {
            n += 1;
            cand = format!("{base}-{n}");
        }
        self.stamp = cand;
    }

    /// **O quê:** persiste o estado (3 gerações) só se mudou. **Onde:** fim de tick e de aplicação.
    fn salvar_estado(&mut self) -> Result<(), String> {
        self.st.hash_context = self.ctx.hash.clone();
        self.st.hash_checklist = self.chk.hash.clone();
        self.st.seq_context = self.ctx.seq;
        self.st.seq_checklist = self.chk.seq;
        let json =
            serde_json::to_string(&self.st).map_err(|e| format!("serializar estado: {e}"))?;
        if json == self.json_salvo {
            return Ok(());
        }
        self.arm_est.gravar(json.as_bytes())?;
        self.json_salvo = json;
        Ok(())
    }

    /// **O quê:** UM tick mecânico: coleta, renderiza e grava só o que mudou (`forcar` ignora o
    /// hash). Falha de gravação mantém a geração anterior, registra e devolve `Err`. **Onde:** loop.
    pub fn tick_mecanico(&mut self, forcar: bool) -> Result<Tick, String> {
        let agora = (self.relogio)();
        self.garantir_stamp(agora);
        let c = coletar(&self.cfg, agora);
        let base = contexto(&c, None);
        let chk = checklist(&c);
        self.hash_estado = format!("{:016x}", checksum(format!("{base}\u{0}{chk}").as_bytes()));
        let ctx = contexto(&c, self.st.notas.as_ref());
        let (dir, stamp, quando) = (self.cfg.dir_context(), self.stamp.clone(), ts_humano(agora));
        let r1 = self.ctx.gravar(&dir, &stamp, &quando, &ctx, forcar);
        let r2 = self.chk.gravar(&dir, &stamp, &quando, &chk, forcar);
        self.resumo = base;
        let salvo = self.salvar_estado();
        let escreveu = *r1.as_ref().unwrap_or(&false) || *r2.as_ref().unwrap_or(&false);
        if let Some(e) = [r1.err(), r2.err(), salvo.err()].into_iter().flatten().next() {
            self.registrar_erro(&format!("tick mecânico: {e}"));
            return Err(e);
        }
        Ok(Tick { escreveu, seq_context: self.ctx.seq, seq_checklist: self.chk.seq })
    }

    /// **O quê:** `true` se a camada Claude está devida: ligada, ociosa, passou do `proximo` e o
    /// estado mudou desde o último handoff do Claude. **Onde:** [`Self::tick`]. PURA.
    fn claude_devido(&self, agora: u64) -> bool {
        !self.cfg.sem_claude
            && self.em_voo.is_none()
            && agora >= self.st.proximo_claude_em
            && self.st.hash_no_claude.as_deref() != Some(self.hash_estado.as_str())
    }

    /// **O quê:** dispara a camada Claude numa thread; `false` se já há uma em voo (não empilha).
    /// **Onde:** [`Self::tick`]. Nunca bloqueia.
    pub fn disparar_claude(&mut self) -> bool {
        if self.em_voo.is_some() {
            return false;
        }
        let (cfg, agora, ultimo) = (self.cfg.clone(), (self.relogio)(), self.st.ultimo_claude_em);
        let prompt = construir_prompt(&self.resumo);
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(executar(&cfg, &prompt, agora, ultimo)); // receptor ausente = motor encerrou.
        });
        self.st.ultimo_claude_em = Some(agora);
        self.em_voo = Some(EmVoo { rx, hash: self.hash_estado.clone() });
        true
    }

    /// **O quê:** recolhe o resultado da thread, se já chegou, e aplica. **Onde:** início de [`Self::tick`].
    pub fn coletar_claude(&mut self) {
        let Some(v) = self.em_voo.as_ref() else { return };
        let (res, hash) = match v.rx.try_recv() {
            Ok(r) => (r, v.hash.clone()),
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                let r = ResClaude {
                    resultado: Err("thread do claude morreu".into()),
                    uso: Default::default(),
                    rodou: false,
                };
                (r, v.hash.clone())
            }
        };
        self.em_voo = None;
        self.aplicar_claude(res, &hash);
    }

    /// **O quê:** sucesso -> notas novas + carimbo novo + zera falhas; falha -> conta, guarda o
    /// motivo e agenda o backoff. Sempre anota no ledger e persiste. **Onde:** [`Self::coletar_claude`].
    pub fn aplicar_claude(&mut self, res: ResClaude, hash: &str) {
        let agora = (self.relogio)();
        let base = self.cfg.claude_s;
        let estado = match &res.resultado {
            Ok((texto, n)) => {
                self.st.notas = Some(Notas { quando: ts_humano(agora), texto: texto.clone() });
                (self.st.hash_no_claude, self.st.falhas, self.st.ultimo_erro) =
                    (Some(hash.to_string()), 0, None);
                self.st.proximo_claude_em = agora.saturating_add(base);
                self.stamp.clear();
                if *n > 0 {
                    self.registrar_erro(&format!(
                        "notas do Claude: {n} trecho(s) sensível(is) redigido(s)"
                    ));
                }
                "done".to_string()
            }
            Err(motivo) => {
                self.st.falhas = self.st.falhas.saturating_add(1);
                self.st.proximo_claude_em = agora.saturating_add(backoff_s(base, self.st.falhas));
                self.st.ultimo_erro = Some(motivo.clone());
                self.registrar_erro(&format!(
                    "camada Claude falhou ({}x): {motivo}",
                    self.st.falhas
                ));
                "failed:claude".to_string()
            }
        };
        self.anotar_ledger(&res, estado);
        if let Err(e) = self.tick_mecanico(false) {
            self.registrar_erro(&format!("gravar após claude: {e}"));
        }
    }

    /// **O quê:** uma linha no `ledger.jsonl` do overdev com os tokens estimados. **Onde:**
    /// [`Self::aplicar_claude`] (só se o processo rodou).
    fn anotar_ledger(&self, res: &ResClaude, estado: String) {
        if !res.rodou {
            return;
        }
        let r = ledger::Registro {
            unidade: "handoff-vivo".into(),
            onda: 0,
            modelo: self.cfg.modelo.clone(),
            estado,
            uso: res.uso,
            custo_micro_usd: ledger::custo_micro_usd(&self.cfg.modelo, &res.uso),
        };
        if let Err(e) = ledger::anexar(&self.cfg.dir_overdev().join("ledger.jsonl"), &r) {
            self.registrar_erro(&format!("ledger: {e}"));
        }
    }

    /// **O quê:** tick completo: recolhe Claude, tick mecânico, dispara Claude se devido. **Onde:** loop.
    pub fn tick(&mut self) -> Result<Tick, String> {
        self.coletar_claude();
        let agora = (self.relogio)();
        if self.st.proximo_claude_em == 0 && !self.cfg.sem_claude {
            self.st.proximo_claude_em = agora.saturating_add(self.cfg.claude_s);
        }
        let t = self.tick_mecanico(false);
        if self.claude_devido(agora) {
            self.disparar_claude();
        }
        t
    }

    /// **O quê:** ciclo imediato (`agora`): mecânico + Claude SÍNCRONO (ignora intervalo e backoff;
    /// respeita `--sem-claude`). **Onde:** `overdev handoff-vivo agora`.
    pub fn agora(&mut self) -> Result<Tick, String> {
        self.tick_mecanico(false)?;
        if !self.cfg.sem_claude && self.em_voo.is_none() {
            let (agora, hash) = ((self.relogio)(), self.hash_estado.clone());
            let res = executar(
                &self.cfg,
                &construir_prompt(&self.resumo),
                agora,
                self.st.ultimo_claude_em,
            );
            self.st.ultimo_claude_em = Some(agora);
            self.aplicar_claude(res, &hash);
        }
        self.tick_mecanico(false)
    }

    /// **O quê:** último handoff ao sair: recolhe o Claude se já terminou e grava forçado.
    /// **Onde:** fim do loop (SIGTERM/SIGINT).
    pub fn finalizar(&mut self) -> Result<Tick, String> {
        self.coletar_claude();
        self.tick_mecanico(true)
    }
}
