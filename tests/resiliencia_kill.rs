//! E2E REAL: o binário `schematize-overdev` executa um checklist de 20 unidades e leva SIGKILL em
//! momentos aleatórios; a cada religação (`--resume`) NENHUMA unidade `Entregue` é re-executada,
//! NENHUMA se perde e o estado final é IDÊNTICO ao de uma execução sem kill.
//!
//! Reproduzir uma falha: `PERSIST_SEED=<seed impressa> cargo test --test resiliencia_kill -- --nocapture`.

mod resiliencia_comum;

use overdev_casa::resiliencia::duravel::{Duravel, PoliticaSnapshot};
use overdev_casa::resiliencia::modelo::StatusItem;
use resiliencia_comum::*;
use schematize_plataforma::persistencia::Rng;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;

/// Número de unidades do checklist do teste.
const UNIDADES: usize = 20;

/// **O quê:** abre o estado (após o kill: o lock já foi solto pelo kernel) e devolve os ids
/// `Entregue`. **Onde:** invariante "Entregue nunca é re-executada".
fn entregues(dir: &Path) -> BTreeSet<String> {
    let d = Duravel::abrir(&dir.join("ov/estado"), PoliticaSnapshot::default()).expect("abrir estado");
    let e = d.estado();
    e.itens.iter().filter(|(_, i)| i.status == StatusItem::Entregue).map(|(k, _)| k.clone()).collect()
}

/// **O quê:** projeção LÓGICA final do estado (a que deve ser idêntica com ou sem kill).
/// **Onde:** comparação com a execução de referência.
fn logico(dir: &Path) -> Vec<String> {
    let d = Duravel::abrir(&dir.join("ov/estado"), PoliticaSnapshot::default()).expect("abrir estado");
    assert!(d.estado().concluido(), "estado final não concluído");
    d.estado().resumo_logico()
}

/// **O quê:** confere que nenhuma unidade do conjunto `ja` iniciou de novo desde `antes`.
/// **Onde:** a cada fim de fase do laço de kills.
fn sem_reexecucao(ja: &BTreeSet<String>, antes: &BTreeMap<String, usize>, agora: &BTreeMap<String, usize>, ctx: &str) {
    for id in ja {
        assert_eq!(antes.get(id), agora.get(id), "{ctx}: a unidade Entregue {id} foi RE-EXECUTADA");
    }
}

/// **O quê:** execução de referência (sem kill): devolve a projeção lógica. **Onde:** o teste.
fn referencia() -> Vec<String> {
    let dir = tmp("base");
    let plano = escrever_plano(&dir, UNIDADES, 30, 2);
    let (code, out, err) = esperar(lanca(&dir, &plano, false, &[]), Duration::from_secs(60));
    assert_eq!(code, Some(0), "referência falhou\n{out}\n{err}");
    assert!(inicios(&dir).values().all(|n| *n == 1) && inicios(&dir).len() == UNIDADES);
    let l = logico(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    l
}

/// **O quê:** uma iteração: 1 a 4 kills em instantes aleatórios, religa com `--resume` até
/// concluir e confere as invariantes. Devolve (kills efetivos, órfãs reanexadas, unidades
/// devolvidas à fila) vistos nas reconciliações. **Onde:** o teste, `n_iter` vezes.
fn iteracao(it: usize, rng: &mut Rng, base: &[String]) -> (usize, usize, usize) {
    let (mut reanexos, mut devolvidas) = (0, 0);
    let dir = tmp(&format!("it{it}"));
    let plano = escrever_plano(&dir, UNIDADES, 30, 2);
    let (kills, mut feitos, mut resume) = (1 + rng.ate(4) as usize, 0, false);
    let mut prev: Option<(BTreeSet<String>, BTreeMap<String, usize>)> = None;
    loop {
        let mut c = lanca(&dir, &plano, resume, &[]);
        resume = true;
        if feitos < kills {
            std::thread::sleep(atraso(rng, 3, 350));
            if matches!(c.try_wait(), Ok(Some(_))) {
                break;
            }
            matar(&mut c);
            feitos += 1;
        } else {
            let (code, out, err) = esperar(c, Duration::from_secs(60));
            assert_eq!(code, Some(0), "it{it}: retomada falhou\n{out}\n{err}");
            reanexos += out.matches("Reanexar").count();
            devolvidas += out.matches("Devolver").count();
        }
        if let Some((ja, antes)) = &prev {
            sem_reexecucao(ja, antes, &inicios(&dir), &format!("it{it}"));
        }
        prev = Some((entregues(&dir), inicios(&dir)));
        if feitos >= kills && prev.as_ref().is_some_and(|(e, _)| e.len() == UNIDADES) {
            break;
        }
    }
    confere_final(it, &dir, &plano, base);
    let _ = std::fs::remove_dir_all(&dir);
    (feitos, reanexos, devolvidas)
}

/// **O quê:** invariantes finais: todas entregues, cada uma iniciou UMA vez, marcadores
/// presentes, estado lógico idêntico à referência e uma retomada extra não faz nada.
/// **Onde:** fim de cada iteração.
fn confere_final(it: usize, dir: &Path, plano: &Path, base: &[String]) {
    let (code, out, err) = esperar(lanca(dir, plano, true, &[]), Duration::from_secs(60));
    assert_eq!(code, Some(0), "it{it}: retomada final falhou\n{out}\n{err}");
    let ini = inicios(dir);
    assert_eq!(ini.len(), UNIDADES, "it{it}: unidade PERDIDA: {ini:?}");
    for (id, n) in &ini {
        assert_eq!(*n, 1, "it{it}: unidade {id} iniciou {n} vezes (duplicada)");
        assert!(dir.join("saida").join(id).exists(), "it{it}: sem marcador de {id}");
    }
    assert_eq!(logico(dir), base, "it{it}: estado final diferente da execução sem kill");
    let d = Duravel::abrir(&dir.join("ov/estado"), PoliticaSnapshot::default()).expect("reabrir");
    assert_eq!(d.abertura.relatorio.bytes_descartados, 0, "it{it}: journal com cauda torn no fim");
}

/// 30 iterações de kill aleatório (`OD_KILL_ITER` muda o número) sobre 20 unidades.
#[test]
fn e2e_sigkill_aleatorio_retoma_sem_perder_nem_duplicar() {
    let seed = semente("kill");
    let mut rng = Rng::novo(seed);
    let n_iter: usize = std::env::var("OD_KILL_ITER").ok().and_then(|v| v.parse().ok()).unwrap_or(30);
    let base = referencia();
    let (mut total_kills, mut reanexos, mut devolvidas) = (0, 0, 0);
    for it in 0..n_iter {
        let (k, r, d) = iteracao(it, &mut rng, &base);
        (total_kills, reanexos, devolvidas) = (total_kills + k, reanexos + r, devolvidas + d);
    }
    eprintln!(
        "[kill] {n_iter} iterações, {total_kills} SIGKILL(s) efetivos, {reanexos} órfã(s) \
         reanexada(s), {devolvidas} unidade(s) devolvida(s) à fila, seed {seed}"
    );
    assert!(total_kills >= n_iter / 2, "poucos kills pegaram o processo vivo (seed {seed})");
}

/// Roda o orquestrador até a unidade `u00` iniciar, mata-o e devolve o diretório e o plano.
/// **Onde:** os testes de órfã.
fn orquestrador_morto_com_unidade_em_voo(nome: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = tmp(nome);
    let plano = escrever_plano(&dir, 1, 900, 1);
    let mut c = lanca(&dir, &plano, false, &[]);
    for _ in 0..500 {
        if inicios(&dir).contains_key("u00") {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(inicios(&dir).contains_key("u00"), "a unidade não iniciou");
    matar(&mut c);
    (dir, plano)
}

/// Órfã que sobreviveu ao SIGKILL do orquestrador é REANEXADA: roda UMA vez e é entregue.
#[test]
fn orfa_sobrevivente_e_reanexada_sem_reexecutar() {
    let (dir, plano) = orquestrador_morto_com_unidade_em_voo("orfa-reanexa");
    let (code, out, err) = esperar(lanca(&dir, &plano, true, &[]), Duration::from_secs(30));
    assert_eq!(code, Some(0), "{out}\n{err}");
    assert!(out.contains("Reanexar"), "esperava reanexar a órfã: {out}");
    assert_eq!(inicios(&dir)["u00"], 1, "a órfã foi re-executada");
    assert!(dir.join("saida/u00").exists());
}

/// Com `--matar-orfas` só aquela unidade morre e é re-despachada (roda 2 vezes no total).
#[test]
fn orfa_com_matar_orfas_morre_e_e_redespachada() {
    let (dir, plano) = orquestrador_morto_com_unidade_em_voo("orfa-mata");
    let (code, out, err) = esperar(lanca(&dir, &plano, true, &["--matar-orfas"]), Duration::from_secs(30));
    assert_eq!(code, Some(0), "{out}\n{err}");
    assert!(out.contains("Matar") && out.contains("\"devolvidas\":1"), "{out}");
    assert_eq!(inicios(&dir)["u00"], 2, "mata a órfã e re-executa uma vez");
    assert!(dir.join("saida/u00").exists());
}
