//! `handoff-vivo verificar` e `status`: valida todas as gerações e mostra o estado do serviço.
//!
//! **O quê:** [`verificar`] lê as 3 gerações de cada arquivo lógico, reporta `OK`/`INVALIDA`,
//! lista `.tmp` órfãos e (com `reparar`, sob o lock) promove a melhor geração válida e limpa os
//! `.tmp`. [`status`] resume lock, seq, notas do Claude, falhas e próximo disparo.
//! **Onde:** [`super::comandos`].

use super::atomico::{limpar_tmp, tmps_orfaos, Armazem};
use super::config::Config;
use super::trava::{dono_vivo, ler_dono, Trava};

/// Resultado do `verificar`.
#[derive(Debug)]
pub struct Relatorio {
    /// Texto para o terminal.
    pub texto: String,
    /// Todo arquivo lógico tem ao menos uma geração válida.
    pub ok: bool,
}

/// **O quê:** os três arquivos lógicos (dir, nome). **Onde:** [`verificar`].
fn logicos(cfg: &Config) -> Vec<Armazem> {
    let (c, o) = (cfg.dir_context(), cfg.dir_operacional());
    vec![
        Armazem::novo(&c, "ULTIMO-context.md"),
        Armazem::novo(&c, "ULTIMO-checklist.md"),
        Armazem::novo(&o, "estado.hv"),
    ]
}

/// **O quê:** valida todas as gerações; `reparar` promove a melhor e limpa órfãos (exige o lock).
/// **Onde:** `handoff-vivo verificar [--reparar]`.
pub fn verificar(cfg: &Config, reparar: bool) -> Result<Relatorio, String> {
    let _lock = if reparar {
        Some(
            Trava::adquirir(&cfg.dir_operacional().join("escritor.lock"))
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };
    let (mut t, mut ok) = (String::new(), true);
    for a in logicos(cfg) {
        let mut algum = false;
        for g in a.geracoes() {
            let nome =
                g.caminho.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            match &g.lida {
                Ok(d) => {
                    algum = true;
                    t.push_str(&format!(
                        "OK        {nome} seq={} corpo={}B\n",
                        d.seq,
                        d.corpo.len()
                    ));
                }
                Err(e) if !g.caminho.exists() => t.push_str(&format!("AUSENTE   {nome} ({e})\n")),
                Err(e) => t.push_str(&format!("INVALIDA  {nome}: {e}\n")),
            }
        }
        if reparar && a.reparar()? {
            t.push_str("REPARADO  geração 0 restaurada da melhor válida\n");
        }
        ok &= algum;
    }
    for d in [cfg.dir_context(), cfg.dir_operacional()] {
        for p in tmps_orfaos(&d) {
            t.push_str(&format!("TMP-ORFAO {}\n", p.display()));
        }
        if reparar {
            let n = limpar_tmp(&d)?;
            if n > 0 {
                t.push_str(&format!("LIMPO     {n} .tmp órfão(s) em {}\n", d.display()));
            }
        }
    }
    t.push_str(if ok {
        "RESULTADO OK\n"
    } else {
        "RESULTADO FALHA: arquivo sem nenhuma geração válida\n"
    });
    Ok(Relatorio { texto: t, ok })
}

/// **O quê:** resumo do serviço (dono do lock, seq, falhas, próximo disparo). **Onde:** `status`.
pub fn status(cfg: &Config) -> String {
    let op = cfg.dir_operacional();
    let dono = match ler_dono(&op.join("escritor.lock")) {
        Some(d) if dono_vivo(&d) => format!("ativo (pid {})", d.pid),
        Some(d) => format!("parado (último pid {} morto)", d.pid),
        None => "nunca rodou".to_string(),
    };
    let mut t = format!("escritor: {dono}\narchive: {}\n", cfg.dir_context().display());
    match Armazem::novo(&op, "estado.hv").melhor() {
        Some((_, d)) => match serde_json::from_slice::<super::ciclo::EstadoVivo>(&d.corpo) {
            Ok(e) => t.push_str(&format!(
                "seq context={} checklist={}\nfalhas do Claude: {}{}\nnotas: {}\n",
                e.seq_context,
                e.seq_checklist,
                e.falhas,
                e.ultimo_erro.map(|x| format!(" (último: {x})")).unwrap_or_default(),
                e.notas.map_or("nenhuma".to_string(), |n| n.quando),
            )),
            Err(x) => t.push_str(&format!("estado ilegível: {x}\n")),
        },
        None => t.push_str("sem estado gravado\n"),
    }
    t
}
