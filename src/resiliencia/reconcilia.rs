//! Reconciliação de unidades órfãs na retomada.
//!
//! **O quê:** para cada unidade que o estado diz estar em voo, decide: REANEXAR (o processo
//! sobreviveu à morte do orquestrador, ou já terminou deixando o `resfile`), MATAR só aquela
//! unidade (item fora do plano ou `--matar-orfas`) ou DEVOLVER à fila (nunca lançada). O pid é
//! confirmado por `starttime`: PID reciclado nunca é morto. Cada decisão é gravada no journal
//! ANTES de ser executada (write-ahead).
//! **Onde:** `executor`, no boot de `executar`/`retomar`.

use super::despacho::{Filho, FilhoAdotado};
use super::duravel::Duravel;
use super::modelo::{Evento, UnidadeVoo};
use super::proc;
use schematize_plataforma::persistencia::Erro;

/// Resultado da reconciliação.
#[derive(Default)]
pub struct Reconciliacao {
    /// Unidades reanexadas (id, filho adotado): o executor as acompanha até o fim.
    pub adotados: Vec<(String, Box<dyn Filho>)>,
    /// Texto de cada decisão (para o relatório da retomada).
    pub notas: Vec<String>,
    /// Quantas voltaram à fila (nunca lançadas ou mortas por pedido).
    pub devolvidas: usize,
}

/// **O quê:** o que fazer com uma unidade. **Onde:** [`decidir`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Acao {
    /// Nunca chegou a ter processo: volta à fila.
    Devolver,
    /// Reanexa (vivo ou terminado com `resfile`).
    Reanexar,
    /// Mata só esta unidade e devolve à fila.
    Matar,
}

/// **O quê:** decisão pura: sem pid -> `Devolver`; vivo verificado e (item fora do plano ou
/// `matar_todas`) -> `Matar`; demais -> `Reanexar`. **Onde:** [`reconciliar`]. PURA.
pub fn decidir(tem_pid: bool, vivo: bool, no_plano: bool, matar_todas: bool) -> Acao {
    match (tem_pid, vivo && (matar_todas || !no_plano)) {
        (false, _) => Acao::Devolver,
        (true, true) => Acao::Matar,
        (true, false) => Acao::Reanexar,
    }
}

/// **O quê:** descreve a unidade para o journal. **Onde:** `Observacao` de reconciliação.
fn descricao(u: &UnidadeVoo, vivo: bool, acao: Acao) -> String {
    format!(
        "{}: pid {:?} starttime {:?} unidade '{}' vivo={vivo} -> {acao:?}",
        u.id, u.pid, u.starttime, u.unidade
    )
}

/// **O quê:** reconcilia todas as unidades em voo. Falha de journal sobe (nada é executado sem
/// registro); falha ao parar um scope vira nota. **Onde:** boot do executor.
pub fn reconciliar(d: &mut Duravel, matar_todas: bool) -> Result<Reconciliacao, Erro> {
    let unidades: Vec<UnidadeVoo> = d.estado().voo.values().cloned().collect();
    let mut r = Reconciliacao::default();
    for u in unidades {
        let no_plano = d.estado().itens.contains_key(&u.id);
        let vivo =
            matches!((u.pid, u.starttime), (Some(p), Some(s)) if proc::vivo_verificado(p, s));
        let acao = decidir(u.pid.is_some() && u.starttime.is_some(), vivo, no_plano, matar_todas);
        let nota = descricao(&u, vivo, acao);
        d.registrar(&Evento::Observacao { tipo: "reconcilia".into(), detalhe: nota.clone() })?;
        r.notas.push(nota);
        aplicar(d, &u, acao, &mut r)?;
    }
    Ok(r)
}

/// **O quê:** executa a ação já registrada. **Onde:** [`reconciliar`].
fn aplicar(d: &mut Duravel, u: &UnidadeVoo, acao: Acao, r: &mut Reconciliacao) -> Result<(), Erro> {
    let mut adotado = adotar(u);
    match acao {
        Acao::Reanexar => r.adotados.push((u.id.clone(), Box::new(adotado))),
        Acao::Matar | Acao::Devolver => {
            if acao == Acao::Matar {
                adotado.matar();
            } else if !u.unidade.is_empty() {
                if let Err(e) = proc::parar_scope(&u.unidade) {
                    r.notas.push(format!("{}: {e}", u.id));
                }
            }
            let motivo = format!("reconciliação: {acao:?}");
            d.registrar(&Evento::Redespachada { id: u.id.clone(), motivo })?;
            r.devolvidas += 1;
        }
    }
    Ok(())
}

/// **O quê:** o [`FilhoAdotado`] da unidade gravada (pid 0 se nunca lançada). **Onde:** `aplicar`.
fn adotar(u: &UnidadeVoo) -> FilhoAdotado {
    FilhoAdotado::novo(
        u.pid.unwrap_or(0),
        u.starttime.unwrap_or(0),
        std::path::PathBuf::from(&u.resfile),
        u.unidade.clone(),
        u.teto_mib,
    )
}

#[cfg(test)]
mod testes;
