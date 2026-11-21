//! Integração do módulo `otimiza`: ledger -> cascata -> retomada, só via API pública.

use overdev_casa::otimiza::cascata::{proximo_modelo, Decisao, Modelo, Motivo, Tentativa};
use overdev_casa::otimiza::ledger::{anexar, custo_micro_usd, ler, Registro, Uso};
use overdev_casa::otimiza::retomada::{hash_brief, pendentes_do_plano, Checkpoint, Estado};

#[test]
fn ledger_truncado_no_fim_nao_derruba_leitura() {
    let dir = std::env::temp_dir().join(format!("otz-int-{}", std::process::id()));
    let p = dir.join("ledger.jsonl");
    let uso = Uso { input: 10, ..Uso::default() };
    let r = Registro {
        unidade: "u1".into(),
        onda: 1,
        modelo: "claude-haiku-5".into(),
        estado: "killed:oom".into(),
        uso,
        custo_micro_usd: custo_micro_usd("claude-haiku-5", &uso),
    };
    anexar(&p, &r).expect("anexar");
    let mut f = std::fs::OpenOptions::new().append(true).open(&p).expect("abrir");
    std::io::Write::write_all(&mut f, b"{\"unidade\":\"u2\",\"on").expect("truncar");
    let (rs, ruins) = ler(&p).expect("ler");
    assert_eq!((rs.len(), ruins), (1, 1));
    std::fs::remove_dir_all(&dir).expect("limpar");
}

#[test]
fn oom_relanca_so_a_unidade_morta_no_mesmo_modelo() {
    let plano: Vec<(String, String)> =
        (0..4).map(|i| (format!("u{i}"), format!("brief {i}"))).collect();
    let mut cp = Checkpoint::default();
    for (id, b) in &plano {
        cp.marcar(id, hash_brief(b), Estado::Entregue);
    }
    cp.marcar("u2", hash_brief("brief 2"), Estado::Morta);
    assert_eq!(pendentes_do_plano(&cp, &plano), vec!["u2".to_string()]);
    let h = [Tentativa { modelo: Modelo::Sonnet, ok: false, motivo: Some(Motivo::Oom) }];
    assert_eq!(proximo_modelo(&h, 0, Modelo::Sonnet), Decisao::Reexecutar(Modelo::Sonnet));
}
