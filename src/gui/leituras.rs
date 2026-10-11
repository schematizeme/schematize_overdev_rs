//! Os documentos `--json` das abas Execução, Caixa e Notas — parsers PUROS.
//!
//! **O quê:** `agents --json`, `monitor --json`, `tokens --json`, `caixa list --json`,
//! `arquivos --json` e `notas --json` viram structs/texto; mais as regras PURAS do split
//! (quanto cabe, o que bloqueia).
//!
//! **Onde:** [`super::execucao`], [`super::caixa`] e [`super::notas`], que só desenham o que sai
//! daqui. Irmão de [`super::telas`], que cuida da fila, do progresso, do checklist e do histórico.
//!
//! ## Parser PURO e ERRO que aparece
//!
//! Cada `ler_*` recebe **texto** e devolve a struct, sem subprocesso — afirmável com JSON
//! truncado, hostil ou de outra versão do binário, sem nada instalado. Campo ESSENCIAL ausente
//! é `Err` (a tela mostra o motivo): "teto 0, livre 0" sobre um documento truncado diria à pessoa
//! que a máquina está cheia quando o que houve foi não ter conseguido perguntar.

/// A coluna Sistema: o que o governador de concorrência diz da máquina agora.
#[derive(Debug, Clone, PartialEq)]
pub struct Sistema {
    /// Teto seguro de claudes na máquina inteira (o menor entre CPU, RAM e load).
    pub teto: i64,
    /// Quantos cabem AINDA (teto − já rodando).
    pub livre: i64,
    /// Claudes rodando agora (esta janela, outras e subagents).
    pub rodando: i64,
    /// Load médio de 1 min, já formatado (`"1.25"`).
    pub load: String,
    /// O plano do split pedido com `--split K`; `None` sem o pedido.
    pub plano: Option<PlanoSplit>,
}

/// Quantos subagents cabem em cada claude de um split em `mains`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanoSplit {
    pub mains: i64,
    pub subagents_each: i64,
    pub total_used: i64,
}

/// **O quê:** o `agents --json [--split K]` → [`Sistema`]. Função PURA.
///
/// **Onde:** a coluna Sistema e o plano do split. `total_cap`, `available` e `running_claudes`
/// são essenciais; `load1` ausente vira `"—"` (o load é informativo, não decide nada).
pub fn ler_sistema(texto: &str) -> Result<Sistema, String> {
    let v: serde_json::Value =
        serde_json::from_str(texto).map_err(|e| format!("o sistema veio ilegível: {e}"))?;
    let n = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_i64())
            .ok_or_else(|| format!("o sistema veio sem o número `{k}`"))
    };
    let plano = v.get("split").filter(|p| !p.is_null()).map(|p| {
        let g = |k: &str| p.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
        PlanoSplit {
            mains: g("mains"),
            subagents_each: g("subagents_each"),
            total_used: g("total_used"),
        }
    });
    Ok(Sistema {
        teto: n("total_cap")?,
        livre: n("available")?,
        rodando: n("running_claudes")?,
        load: v
            .get("load1")
            .and_then(|x| x.as_f64())
            .map_or_else(|| "—".to_string(), |l| format!("{l:.2}")),
        plano,
    })
}

/// **O quê:** o plano do split em texto de gente: `"2 claudes · 3 subagents cada (6/10 do teto)"`.
///
/// **Onde:** logo abaixo do seletor de K.
pub fn plano_em_texto(p: &PlanoSplit, teto: i64) -> String {
    format!(
        "{} claudes · {} subagents cada ({}/{} do teto)",
        p.mains, p.subagents_each, p.total_used, teto
    )
}

/// **O quê:** o que IMPEDE o split, ou `None` se pode. Função PURA.
///
/// **Onde:** os botões «Dividir» e «Dividir + lançar», ANTES de abrir o terminal — o CLI também
/// recusa, mas dizer aqui poupa abrir um terminal só para ler a recusa.
///
/// **As duas regras são as do CLI:** K acima do teto trava a máquina; e lançar K claudes com
/// menos de K slots livres também (só o `--force` do CLI passa, e a janela não o oferece).
pub fn bloqueio_do_split(k: i64, dispatch: bool, teto: i64, livre: i64) -> Option<String> {
    if k < 2 {
        return Some("dividir exige ao menos 2 claudes".to_string());
    }
    if k > teto {
        return Some(format!("{k} claudes passa do teto seguro ({teto}). Reduza o K."));
    }
    if dispatch && livre < k {
        return Some(format!(
            "só {livre} slot(s) livre(s) agora (teto {teto}); lançar {k} travaria a máquina. \
             Espere liberar ou reduza o K."
        ));
    }
    None
}

/// **O quê:** o K preso ao intervalo válido `[2, max(teto, 2)]`. **Onde:** o seletor de K.
pub fn prender_k(k: i64, teto: i64) -> i64 {
    k.clamp(2, teto.max(2))
}

/// O monitor leve do run: o que a janela relê a cada ~3 s.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Monitor {
    /// Itens de máquina abertos (texto, sem marcador).
    pub abertos: Vec<String>,
    /// Conclusões recentes, já em `"HH:MM:SS  texto"` na hora LOCAL.
    pub conclusoes: Vec<String>,
}

/// **O quê:** o `monitor --json` → [`Monitor`]. Função PURA.
///
/// **Onde:** o monitor da aba Execução. `abertos` e `conclusoes` ausentes são ERRO (sem run vêm
/// `[]`): lista vazia calada diria "nada aberto" a quem tem 40 itens.
pub fn ler_monitor(texto: &str) -> Result<Monitor, String> {
    let v: serde_json::Value =
        serde_json::from_str(texto).map_err(|e| format!("o monitor veio ilegível: {e}"))?;
    let abertos = v
        .get("abertos")
        .and_then(|x| x.as_array())
        .ok_or("o monitor veio sem a lista `abertos`")?
        .iter()
        .filter_map(|x| x.as_str().map(String::from))
        .collect();
    let conclusoes = v
        .get("conclusoes")
        .and_then(|x| x.as_array())
        .ok_or("o monitor veio sem a lista `conclusoes`")?
        .iter()
        .map(|c| {
            let texto = c.get("texto").and_then(|x| x.as_str()).unwrap_or("");
            let hora = hms_local(c.get("em").and_then(|x| x.as_i64()).unwrap_or(0));
            if hora.is_empty() {
                texto.to_string()
            } else {
                format!("{hora}  {texto}")
            }
        })
        .collect();
    Ok(Monitor { abertos, conclusoes })
}

/// **O quê:** epoch → `"HH:MM:SS"` na hora local; `<= 0` vira vazio. **Onde:** [`ler_monitor`].
pub fn hms_local(epoch: i64) -> String {
    if epoch <= 0 {
        return String::new();
    }
    use chrono::{Local, TimeZone};
    match Local.timestamp_opt(epoch, 0) {
        chrono::offset::LocalResult::Single(dt) => dt.format("%H:%M:%S").to_string(),
        _ => String::new(),
    }
}

/// **O quê:** o `tokens --json` → a linha do monitor. Função PURA.
///
/// **Onde:** o monitor da aba Execução.
/// `"Tokens: 1.234 (in 1.000 / out 234 · cache-read 5.000) · Modelo: claude-x"`.
pub fn linha_de_tokens(texto: &str) -> Result<String, String> {
    let v: serde_json::Value =
        serde_json::from_str(texto).map_err(|e| format!("os tokens vieram ilegíveis: {e}"))?;
    let n = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_u64())
            .ok_or_else(|| format!("os tokens vieram sem o número `{k}`"))
    };
    let modelo = v.get("modelo").and_then(|x| x.as_str()).unwrap_or("—");
    Ok(format!(
        "Tokens: {} (in {} / out {} · cache-read {}) · Modelo: {modelo}",
        milhar(n("total")?),
        milhar(n("entrada")?),
        milhar(n("saida")?),
        milhar(n("cache_leitura")?)
    ))
}

/// **O quê:** inteiro com ponto de milhar (`1234567` → `"1.234.567"`). **Onde:** [`linha_de_tokens`].
pub fn milhar(n: u64) -> String {
    let d = n.to_string();
    let mut o = String::with_capacity(d.len() + d.len() / 3);
    for (i, c) in d.chars().enumerate() {
        if i > 0 && (d.len() - i).is_multiple_of(3) {
            o.push('.');
        }
        o.push(c);
    }
    o
}

/// Uma demanda da caixa de entrada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Demanda {
    pub id: String,
    /// O que a pessoa escreveu, CRU.
    pub texto: String,
    /// Itens que o agente extraiu; vazio enquanto não foi organizada.
    pub itens: Vec<String>,
}

/// A caixa de entrada: o que falta organizar e o que falta fundir.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Caixa {
    pub pendentes: Vec<Demanda>,
    pub processadas: Vec<Demanda>,
}

/// **O quê:** o `caixa list --json` → [`Caixa`]. Função PURA.
///
/// **Onde:** a aba Caixa. Os DOIS grupos são essenciais: caixa vazia é `[]`/`[]`, e um documento
/// sem eles é erro, não "caixa vazia".
pub fn ler_caixa(texto: &str) -> Result<Caixa, String> {
    let v: serde_json::Value =
        serde_json::from_str(texto).map_err(|e| format!("a caixa veio ilegível: {e}"))?;
    let grupo = |k: &str| -> Result<Vec<Demanda>, String> {
        let arr = v
            .get(k)
            .and_then(|x| x.as_array())
            .ok_or_else(|| format!("a caixa veio sem a lista `{k}`"))?;
        Ok(arr
            .iter()
            .map(|d| {
                let s = |c: &str| d.get(c).and_then(|x| x.as_str()).unwrap_or("").to_string();
                Demanda {
                    id: s("id"),
                    texto: s("texto"),
                    itens: d
                        .get("itens")
                        .and_then(|x| x.as_array())
                        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                        .unwrap_or_default(),
                }
            })
            .collect())
    };
    Ok(Caixa { pendentes: grupo("pendentes")?, processadas: grupo("processadas")? })
}

/// Um documento editável do run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arquivo {
    pub nome: String,
    pub caminho: String,
    pub existe: bool,
}

/// **O quê:** o `arquivos --json` → os documentos. Função PURA.
///
/// **Onde:** a aba Notas. Item sem `nome` ou sem `caminho` é descartado — um botão «Abrir» sem
/// caminho não abriria nada.
pub fn ler_arquivos(texto: &str) -> Result<Vec<Arquivo>, String> {
    let v: serde_json::Value =
        serde_json::from_str(texto).map_err(|e| format!("os arquivos vieram ilegíveis: {e}"))?;
    let arr = v
        .get("arquivos")
        .and_then(|x| x.as_array())
        .ok_or("o documento veio sem a lista `arquivos`")?;
    Ok(arr
        .iter()
        .filter_map(|a| {
            let s = |c: &str| a.get(c).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let (nome, caminho) = (s("nome"), s("caminho"));
            (!nome.is_empty() && !caminho.is_empty()).then(|| Arquivo {
                nome,
                caminho,
                existe: a.get("existe").and_then(|x| x.as_bool()).unwrap_or(false),
            })
        })
        .collect())
}

/// **O quê:** o `notas --json` → o texto das notas. Função PURA.
///
/// **Onde:** a aba Notas. A chave `texto` é essencial (sem notas vem `""`).
pub fn ler_notas(texto: &str) -> Result<String, String> {
    let v: serde_json::Value =
        serde_json::from_str(texto).map_err(|e| format!("as notas vieram ilegíveis: {e}"))?;
    v.get("texto")
        .and_then(|x| x.as_str())
        .map(String::from)
        .ok_or_else(|| "as notas vieram sem o campo `texto`".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const AGENTS: &str = r#"{"total_cap":10,"available":7,"cpu_cap":12,"ram_cap":10,"load_cap":14,
        "threads":16,"mem_available_mb":20000,"load1":1.2543,"running_claudes":3,"ram_tight":false,
        "split":{"mains":2,"subagents_each":3,"total_used":8}}"#;

    #[test]
    fn sistema_le_o_contrato_e_o_plano() {
        let s = ler_sistema(AGENTS).expect("lê");
        assert_eq!((s.teto, s.livre, s.rodando), (10, 7, 3));
        assert_eq!(s.load, "1.25");
        assert_eq!(s.plano, Some(PlanoSplit { mains: 2, subagents_each: 3, total_used: 8 }));
        let sem = ler_sistema(r#"{"total_cap":4,"available":0,"running_claudes":4}"#).expect("lê");
        assert_eq!((sem.plano, sem.load.as_str()), (None, "—"));
    }

    /// Campo essencial ausente é ERRO — zero calado diria "máquina cheia".
    #[test]
    fn sistema_truncado_e_hostil_e_erro() {
        assert!(ler_sistema(r#"{"total_cap":4}"#).is_err());
        assert!(ler_sistema(r#"{"total_cap":"4","available":1,"running_claudes":0}"#).is_err());
        assert!(ler_sistema("").is_err());
        assert!(ler_sistema("[]").is_err());
        assert!(ler_sistema("{ nao e json").is_err());
    }

    #[test]
    fn plano_fala_de_gente() {
        let p = PlanoSplit { mains: 2, subagents_each: 3, total_used: 8 };
        assert_eq!(plano_em_texto(&p, 10), "2 claudes · 3 subagents cada (8/10 do teto)");
    }

    /// As regras do split são as do CLI: acima do teto trava; lançar sem slots livres também.
    #[test]
    fn split_bloqueia_acima_do_teto_e_sem_slot_livre() {
        assert_eq!(bloqueio_do_split(4, false, 10, 0), None, "dividir sem lançar não ocupa slot");
        assert_eq!(bloqueio_do_split(4, true, 10, 4), None, "cabe exatamente");
        assert!(bloqueio_do_split(11, false, 10, 10).expect("acima do teto").contains("teto"));
        let sem_slot = bloqueio_do_split(4, true, 10, 3).expect("sem slot");
        assert!(sem_slot.contains("3 slot"), "{sem_slot}");
        assert!(bloqueio_do_split(1, false, 10, 10).is_some(), "dividir em 1 não é dividir");
        assert!(bloqueio_do_split(0, false, 10, 10).is_some());
        assert!(bloqueio_do_split(-3, true, 10, 10).is_some());
    }

    #[test]
    fn k_fica_entre_2_e_o_teto() {
        assert_eq!(prender_k(1, 10), 2);
        assert_eq!(prender_k(99, 10), 10);
        assert_eq!(prender_k(5, 10), 5);
        assert_eq!(prender_k(5, 0), 2, "teto desconhecido não zera o K");
        assert_eq!(prender_k(-4, 3), 2);
    }

    #[test]
    fn monitor_le_abertos_e_conclusoes_com_hora() {
        let m = ler_monitor(
            r#"{"abertos":["a","b"],"conclusoes":[{"em":1790000000,"texto":"feito um"},
            {"em":0,"texto":"sem hora"}]}"#,
        )
        .expect("lê");
        assert_eq!(m.abertos, ["a", "b"]);
        assert_eq!(m.conclusoes.len(), 2);
        assert!(m.conclusoes[0].ends_with("  feito um"), "{:?}", m.conclusoes);
        assert_eq!(m.conclusoes[0].len(), "HH:MM:SS  feito um".len());
        assert_eq!(m.conclusoes[1], "sem hora", "epoch 0 não inventa hora");
    }

    #[test]
    fn monitor_sem_as_listas_e_erro_e_vazio_nao() {
        assert_eq!(
            ler_monitor(r#"{"abertos":[],"conclusoes":[]}"#).expect("vazio"),
            Monitor::default()
        );
        assert!(ler_monitor(r#"{"abertos":[]}"#).is_err());
        assert!(ler_monitor("lixo").is_err());
    }

    #[test]
    fn milhar_usa_ponto() {
        assert_eq!(milhar(0), "0");
        assert_eq!(milhar(999), "999");
        assert_eq!(milhar(1000), "1.000");
        assert_eq!(milhar(1234567), "1.234.567");
    }

    #[test]
    fn tokens_viram_uma_linha() {
        let l = linha_de_tokens(
            r#"{"entrada":1000,"saida":234,"cache_leitura":5000,"cache_escrita":9,"total":1234,
            "mensagens":3,"modelo":"claude-x"}"#,
        )
        .expect("lê");
        assert_eq!(l, "Tokens: 1.234 (in 1.000 / out 234 · cache-read 5.000) · Modelo: claude-x");
        let sem = linha_de_tokens(
            r#"{"entrada":0,"saida":0,"cache_leitura":0,"cache_escrita":0,"total":0,
            "mensagens":0,"modelo":null}"#,
        )
        .expect("lê");
        assert!(sem.ends_with("Modelo: —"), "{sem}");
        assert!(linha_de_tokens(r#"{"total":1}"#).is_err(), "contagem ausente é erro");
        assert!(linha_de_tokens("").is_err());
    }

    #[test]
    fn caixa_le_os_dois_grupos() {
        let c = ler_caixa(
            r#"{"pendentes":[{"id":"d1","ts":1,"texto":"cru","itens":[]}],
            "processadas":[{"id":"d2","ts":2,"texto":"x","itens":["um","dois"]}]}"#,
        )
        .expect("lê");
        assert_eq!(c.pendentes[0].texto, "cru");
        assert_eq!(c.processadas[0].itens, ["um", "dois"]);
        assert_eq!(
            ler_caixa(r#"{"pendentes":[],"processadas":[]}"#).expect("vazia"),
            Caixa::default()
        );
        assert!(ler_caixa(r#"{"pendentes":[]}"#).is_err());
        assert!(ler_caixa("nope").is_err());
    }

    #[test]
    fn arquivos_descartam_item_sem_caminho() {
        let a = ler_arquivos(
            r#"{"dir":"/p/.schematize/overdev","arquivos":[
            {"nome":"PLAN.md","caminho":"/p/.schematize/overdev/PLAN.md","existe":true},
            {"nome":"NOTAS.md","caminho":"/p/.schematize/overdev/NOTAS.md","existe":false},
            {"nome":"SEM.md"},{"caminho":"/x"}]}"#,
        )
        .expect("lê");
        assert_eq!(a.len(), 2);
        assert!(a[0].existe && !a[1].existe);
        assert!(ler_arquivos(r#"{"dir":"/p"}"#).is_err());
    }

    #[test]
    fn notas_exigem_a_chave_texto() {
        assert_eq!(ler_notas(r#"{"texto":""}"#).expect("vazio é válido"), "");
        assert_eq!(ler_notas(r#"{"texto":"a\nb"}"#).expect("lê"), "a\nb");
        assert!(ler_notas("{}").is_err());
        assert!(ler_notas(r#"{"texto":3}"#).is_err());
    }
}
