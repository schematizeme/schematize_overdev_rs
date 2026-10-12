//! Os documentos `--json` do binário, lidos para o que a janela desenha — parsers PUROS.
//!
//! **O quê:** `fila --json`, `progresso --json`, `checklist --json` e `history --json` viram
//! structs; mais o fatiamento (filtro + página) do checklist.
//!
//! **Onde:** [`super::estado`] e as telas, que só desenham o que sai daqui.
//!
//! ## Parser PURO, e a razão tem nome
//!
//! Cada `ler_*` recebe **texto** e devolve a struct — nada de subprocesso. Assim o comportamento
//! é afirmável com JSON inválido, truncado, de outro idioma ou hostil, sem depender do que está
//! instalado na máquina de quem roda a suíte. O repo pagou duas vezes por casar **rótulo em
//! português** e devolver vazio nos outros idiomas sem erro nenhum: a decisão lê SLUG, nunca prosa.
//!
//! ## Vazio e ERRO são coisas diferentes
//!
//! `Ok(vec![])` é "não há pergunta". Um JSON ilegível é `Err`, e a tela mostra o erro — um
//! `default()` silencioso diria "nenhuma pergunta" a quem tem cinco esperando.

/// Uma pergunta, já no formato que a tela consome.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub id: String,
    pub titulo: String,
    pub contexto: String,
    pub kind: String,
    pub opcoes: Vec<Opc>,
    pub destrava: String,
    pub aberta: bool,
    pub aguarda_revisao: bool,
    pub resposta_texto: String,
    pub respondida_em: String,
    pub revisao: String,
}

/// Uma opção clicável.
#[derive(Debug, Clone, PartialEq)]
pub struct Opc {
    pub id: String,
    pub rotulo: String,
    pub detalhe: String,
    pub sugerida: bool,
}

/// **O quê:** as opções IMPLÍCITAS de um `kind`, que o JSON não declara.
///
/// **Onde:** [`ler`]. `aprovar` e `sim_nao_outro` têm sempre as mesmas, e o CLI as valida sem
/// que ninguém as escreva. A tela precisa DESENHÁ-LAS, então elas nascem aqui — com os mesmos
/// ids do CLI, senão o clique manda um id que ele recusa.
fn opcoes_implicitas(kind: &str) -> Vec<Opc> {
    let par = |id: &str, rotulo: String| Opc {
        id: id.to_string(),
        rotulo,
        detalhe: String::new(),
        sugerida: false,
    };
    match kind {
        "aprovar" => vec![par("aprovar", "Aprovar".to_string()), par("negar", "Negar".to_string())],
        "sim_nao_outro" => {
            vec![par("sim", "Sim".to_string()), par("nao", "Não".to_string())]
        }
        _ => vec![],
    }
}

/// **O quê:** epoch → `"24/09 20:59"` em hora LOCAL.
///
/// **Onde:** [`ler`], para o card dizer QUANDO a pessoa respondeu — pedido explícito.
///
/// **Hora local e não UTC:** quem lê é a pessoa que respondeu, e ela pensa no fuso dela. Um
/// epoch 0 (relógio torto) vira vazio em vez de "01/01 00:00", que pareceria uma data real.
fn hora_local(epoch: i64) -> String {
    if epoch <= 0 {
        return String::new();
    }
    use chrono::{Local, TimeZone};
    match Local.timestamp_opt(epoch, 0) {
        chrono::offset::LocalResult::Single(dt) => dt.format("%d/%m %H:%M").to_string(),
        _ => String::new(),
    }
}

/// **O quê:** o texto do `--json` → os cards, na ordem em que a tela os mostra.
///
/// **Onde:** a janela, ao abrir e depois de cada resposta. Função PURA.
///
/// **A ordem é deliberada:** primeiro as ABERTAS (é o que espera a pessoa), depois as que
/// aguardam a máquina, depois as revisadas. Ordenar por data poria uma pergunta respondida
/// ontem acima de uma aberta hoje.
pub fn ler(texto: &str) -> Result<Vec<Card>, String> {
    let v: serde_json::Value = serde_json::from_str(texto)
        .map_err(|e| format!("a fila de perguntas veio ilegível: {e}"))?;

    let mut cards = Vec::new();
    // A ordem dos grupos é a ordem da tela.
    for grupo in ["abertas", "aguardando_revisao", "revisadas"] {
        let Some(arr) = v.get(grupo).and_then(|x| x.as_array()) else { continue };
        for q in arr {
            let s = |k: &str| q.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let kind = s("kind");
            let auto = s("auto");

            let mut opcoes: Vec<Opc> = q
                .get("opcoes")
                .and_then(|x| x.as_array())
                .map(|a| {
                    a.iter()
                        .map(|o| {
                            let g = |k: &str| {
                                o.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
                            };
                            let id = g("id");
                            Opc {
                                sugerida: !auto.is_empty() && id == auto,
                                id,
                                rotulo: g("rotulo"),
                                detalhe: g("detalhe"),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            if opcoes.is_empty() {
                opcoes = opcoes_implicitas(&kind);
            }

            let resposta = q.get("resposta").filter(|r| !r.is_null());
            let revisada =
                resposta.and_then(|r| r.get("revisada")).and_then(|x| x.as_bool()).unwrap_or(false);
            let em = resposta.and_then(|r| r.get("em")).and_then(|x| x.as_i64()).unwrap_or(0);

            // `linka` vira texto corrido: a tela mostra o que a resposta destrava, e uma lista
            // de ids soltos não diz nada a quem não está com o checklist aberto do lado.
            let destrava = q
                .get("linka")
                .and_then(|x| x.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(" · "))
                .unwrap_or_default();

            cards.push(Card {
                id: s("id"),
                titulo: s("titulo"),
                contexto: s("contexto"),
                kind,
                opcoes,
                destrava,
                aberta: resposta.is_none(),
                aguarda_revisao: resposta.is_some() && !revisada,
                resposta_texto: resposta_em_texto(q, resposta),
                respondida_em: hora_local(em),
                revisao: resposta
                    .and_then(|r| r.get("revisao"))
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
            });
        }
    }
    Ok(cards)
}

/// **O quê:** a resposta em linguagem de gente — RÓTULO da opção, nunca o id.
///
/// **Onde:** [`ler`]. O id é contrato de máquina; quem lê o card é pessoa.
fn resposta_em_texto(q: &serde_json::Value, resposta: Option<&serde_json::Value>) -> String {
    let Some(r) = resposta else { return String::new() };
    let rotulo_de = |id: &str| -> String {
        q.get("opcoes")
            .and_then(|x| x.as_array())
            .and_then(|a| {
                a.iter()
                    .find(|o| o.get("id").and_then(|x| x.as_str()) == Some(id))
                    .and_then(|o| o.get("rotulo"))
                    .and_then(|x| x.as_str())
                    .map(String::from)
            })
            .or_else(|| {
                let kind = q.get("kind").and_then(|x| x.as_str()).unwrap_or("");
                opcoes_implicitas(kind).into_iter().find(|o| o.id == id).map(|o| o.rotulo)
            })
            .unwrap_or_else(|| id.to_string())
    };
    let mut partes: Vec<String> = r
        .get("escolhas")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).map(rotulo_de).collect())
        .unwrap_or_default();
    let txt = r.get("texto").and_then(|x| x.as_str()).unwrap_or("").trim();
    if !txt.is_empty() {
        partes.push(txt.to_string());
    }
    partes.join(" · ")
}

/// O progresso do run, como o `progresso --json` o diz.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Progresso {
    /// `active` | `stopped` | `done` | `blocked`; `None` = não há run neste diretório.
    pub modo: Option<String>,
    pub feitos: u64,
    pub abertos: u64,
    pub on_hold: u64,
    pub humanos: u64,
    pub recusados: u64,
    pub iteracoes: u64,
    /// Teto de ciclos; `None` sem run (o `0` seria lido como "teto zero").
    pub teto: Option<u64>,
}

/// **O quê:** o `progresso --json` → [`Progresso`].
///
/// **Onde:** a janela, ao abrir e no «Reler». Função PURA.
///
/// **Contagem ausente é ERRO, não zero:** "0 feitos" sobre um documento truncado diria à pessoa
/// que o run não andou. Só `modo` e `teto` podem ser `null` (sem run).
pub fn ler_progresso(texto: &str) -> Result<Progresso, String> {
    let v: serde_json::Value =
        serde_json::from_str(texto).map_err(|e| format!("o progresso veio ilegível: {e}"))?;
    let n = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_u64())
            .ok_or_else(|| format!("o progresso veio sem o número `{k}`"))
    };
    Ok(Progresso {
        modo: v.get("modo").and_then(|x| x.as_str()).map(String::from),
        feitos: n("feitos")?,
        abertos: n("abertos")?,
        on_hold: n("on_hold")?,
        humanos: n("humanos")?,
        recusados: n("recusados")?,
        iteracoes: n("iteracoes")?,
        teto: v.get("teto").and_then(|x| x.as_u64()),
    })
}

/// Um item do checklist, como o `checklist --json` o diz.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// `open` | `done` | `hold` | `cancel` | `hopen` | `hdone` | `hresp` | `hrec` | `nota`.
    pub tipo: String,
    pub texto: String,
    pub maquina: bool,
    pub sub: bool,
    /// Posição entre os humanos ABERTOS (1-based) — o argumento de `human --done`/`answer`/
    /// `refuse`; 0 nos demais.
    pub humano_n: usize,
}

/// O checklist inteiro do run, mais o cabeçalho que a janela mostra.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Checklist {
    pub modo: Option<String>,
    pub objetivo: Option<String>,
    pub itens: Vec<Item>,
}

/// **O quê:** o `checklist --json` → [`Checklist`]. Função PURA.
///
/// **Onde:** a janela. `itens` ausente é ERRO (sem run vem `[]`): lista vazia calada sobre um
/// documento sem a chave diria "checklist vazio" a quem tem 40 itens.
pub fn ler_checklist(texto: &str) -> Result<Checklist, String> {
    let v: serde_json::Value =
        serde_json::from_str(texto).map_err(|e| format!("o checklist veio ilegível: {e}"))?;
    let arr =
        v.get("itens").and_then(|x| x.as_array()).ok_or("o checklist veio sem a lista `itens`")?;
    let itens = arr
        .iter()
        .map(|i| Item {
            tipo: i.get("tipo").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            texto: i.get("texto").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            maquina: i.get("maquina").and_then(|x| x.as_bool()).unwrap_or(false),
            sub: i.get("sub").and_then(|x| x.as_bool()).unwrap_or(false),
            humano_n: i.get("humano_n").and_then(|x| x.as_u64()).unwrap_or(0) as usize,
        })
        .collect();
    Ok(Checklist {
        modo: v.get("modo").and_then(|x| x.as_str()).map(String::from),
        objetivo: v.get("objetivo").and_then(|x| x.as_str()).map(String::from),
        itens,
    })
}

/// Um snapshot do histórico.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub id: i64,
    pub arquivo: String,
    /// Epoch em segundos.
    pub em: i64,
    pub bytes: i64,
}

/// **O quê:** o `history --json` → os snapshots. Função PURA.
///
/// **Onde:** a aba Histórico. `snapshots` ausente é ERRO; `[]` é "sem histórico ainda".
pub fn ler_historico(texto: &str) -> Result<Vec<Snapshot>, String> {
    let v: serde_json::Value =
        serde_json::from_str(texto).map_err(|e| format!("o histórico veio ilegível: {e}"))?;
    let arr = v
        .get("snapshots")
        .and_then(|x| x.as_array())
        .ok_or("o histórico veio sem a lista `snapshots`")?;
    Ok(arr
        .iter()
        .map(|s| Snapshot {
            id: s.get("id").and_then(|x| x.as_i64()).unwrap_or(0),
            arquivo: s.get("arquivo").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            em: s.get("em").and_then(|x| x.as_i64()).unwrap_or(0),
            bytes: s.get("bytes").and_then(|x| x.as_i64()).unwrap_or(0),
        })
        .collect())
}

/// **O quê:** bytes → `"1.2 KB"`. **Onde:** a linha do snapshot.
pub fn tamanho(b: i64) -> String {
    match b {
        i64::MIN..=0 => "0 B".to_string(),
        1..=1023 => format!("{b} B"),
        1024..=1_048_575 => format!("{:.1} KB", b as f64 / 1024.0),
        _ => format!("{:.1} MB", b as f64 / 1_048_576.0),
    }
}

/// **O quê:** epoch → `"24/09 20:59"` para o snapshot; `0` vira vazio. **Onde:** a linha do
/// snapshot (mesma regra do card da pergunta).
pub fn quando(epoch: i64) -> String {
    hora_local(epoch)
}

/// Itens por página — a árvore de widgets do iced não é virtualizada, então a lista é FATIADA para o
/// número de elementos não crescer com o projeto (piso "software de massa").
pub const POR_PAGINA: usize = 100;

/// Filtros do checklist. O `i32` é o que a mensagem `Filtro` carrega.
/// 0 = todos · 1 = abertos (máquina) · 2 = feitos · 3 = on-hold · 4 = humanos abertos.
pub fn casa(tipo: &str, filtro: i32) -> bool {
    match filtro {
        1 => tipo == "open",
        // "Feitos" inclui respondido (a pendência humana acabou); NÃO inclui recusado, que é
        // resolvido e não feito — misturar mentiria no contador.
        2 => tipo == "done" || tipo == "hdone" || tipo == "hresp",
        3 => tipo == "hold",
        4 => tipo == "hopen",
        // Filtro desconhecido cai em "todos": nunca esconde item por engano.
        _ => true,
    }
}

/// **O quê:** quantas páginas há para `total` itens (sempre >= 1: a tela nunca diz "0 de 0").
pub fn paginas(total: usize) -> usize {
    total.div_ceil(POR_PAGINA).max(1)
}

/// **O quê:** a FATIA da página `pagina` (0-based, presa ao intervalo válido) sob `filtro`, e o
/// total filtrado.
///
/// **Onde:** a aba Checklist, a cada desenho. Empresta só o que vai à tela (sem clonar).
pub fn fatia(todos: &[Item], filtro: i32, pagina: usize) -> (Vec<&Item>, usize) {
    let total = todos.iter().filter(|i| casa(&i.tipo, filtro)).count();
    let p = pagina.min(paginas(total) - 1);
    let v = todos
        .iter()
        .filter(|i| casa(&i.tipo, filtro))
        .skip(p * POR_PAGINA)
        .take(POR_PAGINA)
        .collect();
    (v, total)
}

/// **O quê:** a linha de resumo do cabeçalho: `"ativo · 2 feitos · 1 aberto …"`. **Onde:** topo.
pub fn resumo(p: &Progresso) -> String {
    let modo = match p.modo.as_deref() {
        None => return "sem run neste diretório".to_string(),
        Some("active") => "ativo",
        Some("stopped") => "parado",
        Some("done") => "concluído",
        Some("blocked") => "bloqueado",
        Some(outro) => outro,
    };
    format!(
        "{modo} · {} feitos · {} abertos · {} em espera · {} humanos · {} recusados",
        p.feitos, p.abertos, p.on_hold, p.humanos, p.recusados
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    const PROG: &str = r#"{"raiz":"/p","modo":"active","feitos":2,"abertos":1,"on_hold":1,
        "humanos":1,"recusados":1,"iteracoes":7,"teto":40}"#;

    #[test]
    fn progresso_le_o_contrato() {
        let p = ler_progresso(PROG).expect("lê");
        assert_eq!((p.feitos, p.abertos, p.on_hold, p.humanos, p.recusados), (2, 1, 1, 1, 1));
        assert_eq!((p.modo.as_deref(), p.iteracoes, p.teto), (Some("active"), 7, Some(40)));
        assert!(resumo(&p).starts_with("ativo · 2 feitos"));
    }

    /// Sem run: `modo` e `teto` são `null` — e isso NÃO é erro.
    #[test]
    fn progresso_sem_run_e_estado_e_nao_erro() {
        let p = ler_progresso(
            r#"{"raiz":"/p","modo":null,"feitos":0,"abertos":0,"on_hold":0,"humanos":0,
            "recusados":0,"iteracoes":0,"teto":null}"#,
        )
        .expect("sem run é válido");
        assert_eq!((p.modo, p.teto), (None, None));
        assert_eq!(resumo(&Progresso::default()), "sem run neste diretório");
    }

    /// Contagem faltando é ERRO: zero calado diria que o run não andou.
    #[test]
    fn progresso_truncado_e_hostil_e_erro_e_nao_zero() {
        assert!(ler_progresso(r#"{"modo":"active","feitos":2}"#).is_err());
        assert!(ler_progresso(r#"{"feitos":"2","abertos":1}"#).is_err(), "string não é número");
        assert!(ler_progresso("").is_err());
        assert!(ler_progresso("[]").is_err());
        assert!(ler_progresso("{ nao e json").is_err());
    }

    #[test]
    fn checklist_le_itens_e_distingue_sem_run_de_erro() {
        let c = ler_checklist(
            r#"{"raiz":"/p","modo":"active","objetivo":"X","itens":[
            {"tipo":"done","texto":"a","maquina":true,"humano_n":0,"sub":false},
            {"tipo":"hopen","texto":"b","maquina":false,"humano_n":1,"sub":false}]}"#,
        )
        .expect("lê");
        assert_eq!((c.itens.len(), c.objetivo.as_deref()), (2, Some("X")));
        assert!(!c.itens[1].maquina);
        let vazio = ler_checklist(r#"{"modo":null,"objetivo":null,"itens":[]}"#).expect("vazio");
        assert!(vazio.itens.is_empty() && vazio.modo.is_none());
        assert!(ler_checklist("{}").is_err(), "sem `itens` é erro, não lista vazia");
        assert!(ler_checklist(r#"{"itens":"x"}"#).is_err());
        for lixo in ["", "null", "\u{0}", r#"{"itens":[null,1,"a",{"tipo":5}]}"#] {
            let _ = ler_checklist(lixo); // só não pode panicar
        }
    }

    #[test]
    fn historico_le_snapshots_e_vazio_nao_e_erro() {
        let h = ler_historico(
            r#"{"snapshots":[{"id":3,"arquivo":"CHECKLIST.md","em":1790297245,"bytes":2048}]}"#,
        )
        .expect("lê");
        assert_eq!((h[0].id, h[0].bytes), (3, 2048));
        assert!(ler_historico(r#"{"snapshots":[]}"#).expect("vazio").is_empty());
        assert!(ler_historico("{}").is_err());
        assert!(ler_historico("lixo").is_err());
        assert_eq!(tamanho(0), "0 B");
        assert_eq!(tamanho(512), "512 B");
        assert_eq!(tamanho(2048), "2.0 KB");
        assert_eq!(tamanho(-9), "0 B");
    }

    /// **O número do humano aberto vem do JSON** — é ele que vira `human --done N`/`answer N`.
    /// Ausente (CLI antigo) é 0, e 0 não é índice: a ação recusa em vez de fechar o item errado.
    #[test]
    fn checklist_traz_o_numero_do_humano_aberto() {
        let c = ler_checklist(
            r#"{"modo":"active","objetivo":null,"itens":[
            {"tipo":"hopen","texto":"A","maquina":false,"humano_n":1,"sub":false},
            {"tipo":"hopen","texto":"B","maquina":false,"humano_n":2,"sub":false},
            {"tipo":"hdone","texto":"C","maquina":false,"humano_n":0,"sub":false},
            {"tipo":"hopen","texto":"sem número","maquina":false,"sub":false}]}"#,
        )
        .expect("lê");
        let ns: Vec<usize> = c.itens.iter().map(|i| i.humano_n).collect();
        assert_eq!(ns, [1, 2, 0, 0]);
    }

    fn sample(n: usize) -> Vec<Item> {
        let kinds = ["done", "open", "hold", "hopen", "hdone"];
        (0..n)
            .map(|i| Item {
                tipo: kinds[i % 5].into(),
                texto: format!("item {i}"),
                maquina: i % 5 < 3,
                sub: false,
                humano_n: 0,
            })
            .collect()
    }

    /// Respondido conta como feito; recusado não entra em filtro de trabalho, mas aparece no "todos".
    #[test]
    fn respondido_conta_como_feito_recusado_nao() {
        assert!(casa("hresp", 2) && casa("hdone", 2));
        assert!(!casa("hrec", 2) && !casa("cancel", 2) && !casa("hrec", 1) && !casa("hrec", 4));
        assert!(casa("hrec", 0) && casa("cancel", 0));
        assert!(casa("open", 99), "filtro desconhecido não esconde item");
    }

    #[test]
    fn pagina_limita_o_que_vai_pra_tela() {
        let all = sample(700);
        assert_eq!(fatia(&all, 0, 0).0.len(), POR_PAGINA);
        assert_eq!(paginas(700), 7);
        assert_eq!(fatia(&all, 0, 99).0.len(), POR_PAGINA, "página além do fim é presa na última");
        let (p1, total) = fatia(&all, 1, 1);
        assert_eq!((p1.len(), total), (40, 140));
        assert!(p1.iter().all(|i| i.tipo == "open"));
        assert_eq!(paginas(0), 1);
        assert_eq!(fatia(&[], 0, 5), (vec![], 0));
    }

    const REAL: &str = r#"{
      "abertas": [{
        "id": "q-a-1", "criada_em": 1790297245, "fase": "FH",
        "titulo": "`services` merece tela própria, ou fica como lista secundária?",
        "contexto": "Hoje é seção colapsável.", "kind": "escolha",
        "opcoes": [
          {"id": "propria", "rotulo": "Tela própria", "detalhe": "quinta aba"},
          {"id": "secundaria", "rotulo": "Lista secundária", "detalhe": ""}
        ],
        "auto": "secundaria", "linka": ["E3-tela-services"], "resposta": null
      }],
      "aguardando_revisao": [{
        "id": "q-b-2", "criada_em": 1790297000, "fase": "", "titulo": "Corto as tags?",
        "contexto": "", "kind": "aprovar", "opcoes": [], "auto": "", "linka": [],
        "resposta": {"em": 1790297100, "por": "humano", "escolhas": ["aprovar"],
                     "texto": "", "revisada": false, "revisao": ""}
      }],
      "revisadas": [{
        "id": "q-c-3", "criada_em": 1790296000, "fase": "", "titulo": "Windows fica?",
        "contexto": "", "kind": "sim_nao_outro", "opcoes": [], "auto": "", "linka": [],
        "resposta": {"em": 1790296500, "por": "humano", "escolhas": [],
                     "texto": "pode fazer", "revisada": true, "revisao": "fica no -msvc"}
      }],
      "ilegiveis": []
    }"#;

    #[test]
    fn le_os_tres_grupos_na_ordem_da_tela() {
        let c = ler(REAL).expect("lê");
        assert_eq!(c.len(), 3);
        assert_eq!(
            (c[0].aberta, c[1].aguarda_revisao, c[2].aberta),
            (true, true, false),
            "aberta primeiro, depois a que espera a máquina, depois a fechada"
        );
        assert_eq!(c[0].id, "q-a-1");
    }

    /// A sugestão do agente marca a opção certa — e NÃO marca as outras.
    #[test]
    fn auto_marca_uma_opcao_so() {
        let c = ler(REAL).expect("lê");
        let sug: Vec<&str> =
            c[0].opcoes.iter().filter(|o| o.sugerida).map(|o| o.id.as_str()).collect();
        assert_eq!(sug, vec!["secundaria"]);
    }

    /// `aprovar`/`sim_nao_outro` não declaram opção, e a tela precisa desenhar as duas.
    #[test]
    fn kind_implicito_ganha_as_opcoes_que_o_json_nao_traz() {
        let c = ler(REAL).expect("lê");
        let ids: Vec<&str> = c[1].opcoes.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, vec!["aprovar", "negar"], "sem isto o card viria sem botão");
        let ids: Vec<&str> = c[2].opcoes.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, vec!["sim", "nao"]);
    }

    /// A resposta aparece por RÓTULO, não por id — inclusive a de opção implícita.
    #[test]
    fn resposta_vem_por_rotulo() {
        let c = ler(REAL).expect("lê");
        assert_eq!(c[1].resposta_texto, "Aprovar", "id `aprovar` → rótulo");
        assert_eq!(c[2].resposta_texto, "pode fazer", "o texto do `outro` entra como está");
    }

    /// **A lição que este repo pagou duas vezes:** a decisão não pode depender da prosa.
    #[test]
    fn a_decisao_nao_muda_com_o_idioma_da_prosa() {
        let jp = REAL
            .replace("Tela própria", "専用画面")
            .replace("Lista secundária", "副リスト")
            .replace("Hoje é seção colapsável.", "現在は折りたたみセクションです。");
        let a = ler(REAL).expect("pt");
        let b = ler(&jp).expect("ja");
        // O que É decisão: ids, kind, estado, sugestão.
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.id, y.id);
            assert_eq!(x.kind, y.kind);
            assert_eq!(x.aberta, y.aberta);
            assert_eq!(x.aguarda_revisao, y.aguarda_revisao);
            let ids = |c: &Card| c.opcoes.iter().map(|o| o.id.clone()).collect::<Vec<_>>();
            assert_eq!(ids(x), ids(y));
            let sug = |c: &Card| c.opcoes.iter().map(|o| o.sugerida).collect::<Vec<_>>();
            assert_eq!(sug(x), sug(y));
        }
        // E o que é PROSA mudou de verdade — senão o teste passaria por não ter trocado nada.
        assert_ne!(a[0].opcoes[0].rotulo, b[0].opcoes[0].rotulo);
    }

    /// Fila vazia é `Ok(vec![])`, e ilegível é `Err`. Não são o mesmo estado.
    #[test]
    fn vazio_e_erro_sao_estados_diferentes() {
        let vazio = ler(r#"{"abertas":[],"aguardando_revisao":[],"revisadas":[],"ilegiveis":[]}"#);
        assert_eq!(vazio.expect("vazio é Ok"), vec![]);
        assert!(ler("{ isto nao e json").is_err(), "ilegível tem de ser Err, não lista vazia");
        // Um objeto sem os grupos é Ok e vazio: é JSON válido que simplesmente não tem fila.
        assert_eq!(ler("{}").expect("objeto vazio"), vec![]);
    }

    /// Entrada hostil não panica — janela que morre ao abrir é pior que lista vazia.
    #[test]
    fn entrada_hostil_nao_panica() {
        let profundo = format!("{}{}", "[".repeat(5000), "]".repeat(5000));
        for lixo in [
            "",
            "null",
            "[]",
            "0",
            "\"texto\"",
            "\u{0}",
            &profundo,
            r#"{"abertas":"nao e lista"}"#,
            r#"{"abertas":[null]}"#,
            r#"{"abertas":[{"id":123,"kind":true,"opcoes":"x","linka":5,"resposta":7}]}"#,
            r#"{"abertas":[{"resposta":{"em":"nao e numero","escolhas":{}}}]}"#,
        ] {
            let _ = ler(lixo); // só não pode panicar
        }
    }

    /// Campo ausente não derruba a linha: ela vem com o campo vazio.
    #[test]
    fn campo_ausente_vira_vazio_em_vez_de_erro() {
        let c = ler(r#"{"abertas":[{"id":"q-x"}]}"#).expect("lê");
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].titulo.as_str(), c[0].kind.as_str()), ("", ""));
        assert!(c[0].aberta, "sem `resposta` é aberta");
        assert!(c[0].opcoes.is_empty(), "kind vazio não tem opção implícita");
    }

    /// Epoch 0 não vira uma data que parece real.
    #[test]
    fn epoch_zero_nao_inventa_data() {
        assert_eq!(hora_local(0), "");
        assert_eq!(hora_local(-5), "");
        assert!(!hora_local(1_790_297_245).is_empty());
    }
}
