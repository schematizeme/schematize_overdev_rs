//! Integração do módulo `otimiza`: ledger -> cascata -> retomada, só via API pública.

use overdev_casa::otimiza::cascata::{proximo_modelo, Decisao, Modelo, Motivo, Tentativa};
use overdev_casa::otimiza::ledger::{anexar, custo_micro_usd, ler, Registro, Uso};
use overdev_casa::otimiza::retomada::{
    hash_brief, pendentes_do_plano, unidades_pendentes, Checkpoint, Estado,
};

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

/// **O quê:** duas unidades (u0 Entregue, u1 em execução morta por OOM): só u1 vira Morta e é
/// relançada no MESMO modelo; u0 segue Entregue e fora dos pendentes. OOM repetido para em 3.
/// **Onde:** princípio "OOM de uma unidade não refaz as já entregues" (retomada + cascata).
#[test]
fn oom_relanca_so_a_unidade_morta_no_mesmo_modelo() {
    let plano: Vec<(String, String)> =
        (0..2).map(|i| (format!("u{i}"), format!("brief {i}"))).collect();
    let mut cp = Checkpoint::default();
    cp.marcar("u0", hash_brief("brief 0"), Estado::Entregue);
    cp.marcar("u1", hash_brief("brief 1"), Estado::Rodando);
    cp.marcar("u1", hash_brief("brief 1"), Estado::Morta);
    assert_eq!(cp.unidades["u0"].estado, Estado::Entregue);
    assert_eq!(cp.unidades["u1"].estado, Estado::Morta);
    assert_eq!(pendentes_do_plano(&cp, &plano), vec!["u1".to_string()]);
    assert_eq!(unidades_pendentes(&cp), vec!["u1".to_string()]);
    let oom = |n: usize| {
        vec![Tentativa { modelo: Modelo::Sonnet, ok: false, motivo: Some(Motivo::Oom) }; n]
    };
    assert_eq!(proximo_modelo(&oom(1), 0, Modelo::Sonnet), Decisao::Reexecutar(Modelo::Sonnet));
    assert_eq!(proximo_modelo(&oom(2), 0, Modelo::Sonnet), Decisao::Reexecutar(Modelo::Sonnet));
    assert_eq!(proximo_modelo(&oom(3), 0, Modelo::Sonnet), Decisao::Parkear);
}

/// **O quê:** negativo: marcar `Morta` (ou `Rodando`/`Falhou`) sobre `Entregue` com o mesmo hash
/// não muda nada. **Onde:** garante que `Entregue` nunca regride.
#[test]
fn marcar_morta_sobre_entregue_nao_muda_nada() {
    let h = hash_brief("brief");
    let plano = vec![("u".to_string(), "brief".to_string())];
    let mut cp = Checkpoint::default();
    cp.marcar("u", h, Estado::Entregue);
    for e in [Estado::Morta, Estado::Rodando, Estado::Falhou] {
        cp.marcar("u", h, e);
        assert_eq!(cp.unidades["u"].estado, Estado::Entregue);
        assert!(pendentes_do_plano(&cp, &plano).is_empty());
        assert!(unidades_pendentes(&cp).is_empty());
    }
}
