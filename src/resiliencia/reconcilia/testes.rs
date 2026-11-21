//! Testes da reconciliação: órfã viva reanexada/morta, PID reciclado preservado, nunca lançada.

use super::*;
use crate::resiliencia::duravel::PoliticaSnapshot;
use crate::resiliencia::modelo::{ItemPlano, StatusItem};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

/// Dir temporário limpo. **Onde:** testes.
fn tmp(nome: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("od-rec-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Estado com os itens `ids` despachados; devolve o `Duravel`. **Onde:** testes.
fn estado_com(d: &std::path::Path, ids: &[&str]) -> Duravel {
    let mut dur = Duravel::abrir(&d.join("estado"), PoliticaSnapshot::default()).unwrap();
    let itens = ids
        .iter()
        .map(|i| ItemPlano {
            id: i.to_string(),
            brief: format!("b-{i}"),
            tipo: "t".into(),
            teto_mib: None,
            marca: None,
            argv: vec![],
        })
        .collect();
    dur.registrar(&Evento::Plano { prefixo: String::new(), modelo_base: "sonnet".into(), itens })
        .unwrap();
    for i in ids {
        dur.registrar(&Evento::Despachada {
            id: i.to_string(),
            modelo: "sonnet".into(),
            geracao: 0,
            despacho: 1,
            unidade: String::new(),
            resfile: d.join(format!("{i}.res")).to_string_lossy().into_owned(),
            teto_mib: 500,
            inicio: 1,
        })
        .unwrap();
    }
    dur
}

/// Sobe um `sleep` em grupo próprio (a "unidade órfã"). **Onde:** testes.
fn orfa() -> Child {
    Command::new("sleep").arg("60").stdin(Stdio::null()).process_group(0).spawn().unwrap()
}

/// Registra pid/starttime da órfã no estado. **Onde:** testes.
fn lancou(dur: &mut Duravel, id: &str, c: &Child, starttime: u64) {
    dur.registrar(&Evento::Lancada { id: id.into(), pid: c.id(), starttime }).unwrap();
}

/// Órfã viva e verificada é REANEXADA e continua viva; a outra (nunca lançada) volta à fila.
#[test]
fn orfa_viva_e_reanexada_e_nunca_lancada_volta_a_fila() {
    let d = tmp("reanexa");
    let mut dur = estado_com(&d, &["a", "b"]);
    let mut c = orfa();
    lancou(&mut dur, "a", &c, proc::starttime(c.id()).unwrap());
    let r = reconciliar(&mut dur, false).unwrap();
    assert_eq!(r.adotados.len(), 1);
    assert_eq!(r.adotados[0].0, "a");
    assert_eq!(r.devolvidas, 1);
    assert_eq!(dur.estado().itens["b"].status, StatusItem::Pendente);
    assert_eq!(dur.estado().itens["a"].status, StatusItem::EmVoo);
    assert!(proc::ler_stat(c.id()).is_some_and(|s| s.estado != 'Z'), "a órfã precisa seguir viva");
    c.kill().unwrap();
    c.wait().unwrap();
    std::fs::remove_dir_all(&d).unwrap();
}

/// `matar_orfas` mata as unidades registradas e SÓ elas: um vizinho fora do estado segue vivo.
#[test]
fn matar_orfas_mata_so_as_unidades_registradas() {
    let d = tmp("mata");
    let mut dur = estado_com(&d, &["a", "b"]);
    let (mut ca, mut cb, mut vizinho) = (orfa(), orfa(), orfa());
    lancou(&mut dur, "a", &ca, proc::starttime(ca.id()).unwrap());
    lancou(&mut dur, "b", &cb, proc::starttime(cb.id()).unwrap());
    let r = reconciliar(&mut dur, true).unwrap();
    assert_eq!((r.adotados.len(), r.devolvidas), (0, 2));
    ca.wait().unwrap();
    cb.wait().unwrap();
    assert_eq!(dur.estado().itens["a"].redespachos, 1);
    assert_eq!(dur.estado().itens["b"].status, StatusItem::Pendente);
    assert!(proc::ler_stat(vizinho.id()).is_some_and(|s| s.estado != 'Z'), "o vizinho morreu");
    vizinho.kill().unwrap();
    vizinho.wait().unwrap();
    std::fs::remove_dir_all(&d).unwrap();
}

/// PID reciclado (starttime diferente) NUNCA é morto, mesmo com `matar_orfas`.
#[test]
fn pid_reciclado_nao_e_morto() {
    let d = tmp("pidreuse");
    let mut dur = estado_com(&d, &["a"]);
    let mut c = orfa();
    let st = proc::starttime(c.id()).unwrap();
    lancou(&mut dur, "a", &c, st + 12345);
    let r = reconciliar(&mut dur, true).unwrap();
    assert!(proc::ler_stat(c.id()).is_some_and(|s| s.estado != 'Z'), "o processo alheio foi morto");
    assert_eq!(
        r.adotados.len(),
        1,
        "sem starttime batendo vira 'terminada': reanexa e resolve pelo resfile"
    );
    c.kill().unwrap();
    c.wait().unwrap();
    std::fs::remove_dir_all(&d).unwrap();
}

/// A decisão pura cobre as combinações.
#[test]
fn decisao_pura() {
    assert_eq!(decidir(false, false, true, false), Acao::Devolver);
    assert_eq!(decidir(true, true, true, false), Acao::Reanexar);
    assert_eq!(decidir(true, true, false, false), Acao::Matar);
    assert_eq!(decidir(true, true, true, true), Acao::Matar);
    assert_eq!(decidir(true, false, true, true), Acao::Reanexar);
}
