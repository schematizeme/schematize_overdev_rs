//! As LEITURAS `--json` da janela para execução, caixa de entrada, notas e arquivos.
//!
//! **O quê:** `caixa list --json`, `monitor --json`, `tokens --json`, `arquivos --json` e
//! `notas --json` — cada um um documento que a janela parseia, e nenhum deles escreve.
//!
//! **Onde:** a janela `schematize-overdev-gui` (abas Execução, Caixa e Notas), que absorveu o que
//! só existia na aba Overdev do hub. Despachados por `comandos::executar`.
//!
//! ## Por que são comandos e não a janela lendo o disco
//!
//! A janela fala com o binário por `--json` (ADR-0020) e nunca abre os arquivos do run: a
//! localização do dir (`.schematize/overdev` ou o legado `.overdev`), o formato do
//! `completions.json` e a pasta dos transcripts do Claude são do domínio. Duas leis para o mesmo
//! fato é o que esta extradição desfaz.
//!
//! ## Escrito à mão, como `saidajson`
//!
//! O documento é CONTRATO com outro processo; `tests/json_execucao.rs` fixa cada nome e tipo.
//! Ausente é `null` onde faz sentido (o `modelo` sem transcript), nunca `""`.

use crate::cli::saidajson::{esc, texto_ou_null};
use crate::overdev::caixa::Entrada;
use crate::{overdev, paths, usage};
use std::path::{Path, PathBuf};

/// Quantos itens abertos o monitor lista (o resto é só contagem no `progresso`).
const ITENS_DO_MONITOR: usize = 10;

/// Quantas conclusões o monitor devolve (as mais recentes) — o log inteiro pode ter milhares.
const CONCLUSOES_DO_MONITOR: usize = 30;

/// Os documentos editáveis do run, na ordem em que a janela os oferece.
const ARQUIVOS_EDITAVEIS: [&str; 3] = ["PLAN.md", "CHECKLIST.md", "NOTAS.md"];

/// **O quê:** o diretório do projeto atual. **Onde:** todos os comandos daqui.
fn raiz() -> Result<PathBuf, String> {
    crate::cli::comandos::cwd_project()
}

/// **O quê:** uma lista de strings JSON (item nunca vira `null`). **Onde:** os documentos daqui.
fn lista(v: &[String]) -> String {
    format!("[{}]", v.iter().map(|s| format!("\"{}\"", esc(s))).collect::<Vec<_>>().join(","))
}

/// **O quê:** uma demanda da caixa como objeto JSON. **Onde:** [`documento_caixa`].
fn entrada_json(e: &Entrada) -> String {
    format!(
        "{{\"id\":\"{}\",\"ts\":{},\"texto\":\"{}\",\"itens\":{}}}",
        esc(&e.id),
        e.ts,
        esc(&e.texto),
        lista(&e.itens)
    )
}

/// **O quê:** o documento `caixa list --json` — função PURA, para o teste travar o shape.
///
/// **Onde:** [`caixa_list`]. `pendentes` são as cruas (a organizar); `processadas` já viraram
/// itens e esperam o `merge`. Vazio é `[]`: caixa vazia é um estado, não um erro.
pub fn documento_caixa(pendentes: &[Entrada], processadas: &[Entrada]) -> String {
    let l = |v: &[Entrada]| v.iter().map(entrada_json).collect::<Vec<_>>().join(",");
    format!("{{\"pendentes\":[{}],\"processadas\":[{}]}}", l(pendentes), l(processadas))
}

/// **O quê:** imprime a caixa de entrada como JSON. **Onde:** `caixa list --json`.
pub fn caixa_list() -> Result<(), String> {
    let r = raiz()?;
    let p = overdev::caixa::pendentes(&r);
    let q = overdev::caixa::processadas(&r);
    println!("{}", documento_caixa(&p, &q));
    Ok(())
}

/// **O quê:** o documento `monitor --json` — itens de máquina abertos e conclusões recentes.
///
/// **Onde:** [`monitor`]. Função PURA. `em` é epoch em segundos: a janela formata na hora LOCAL
/// de quem olha, e o binário não adivinha o fuso dela.
pub fn documento_monitor(abertos: &[String], conclusoes: &[overdev::Completion]) -> String {
    let c = conclusoes
        .iter()
        .map(|c| format!("{{\"em\":{},\"texto\":\"{}\"}}", c.ts, esc(&c.text)))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"abertos\":{},\"conclusoes\":[{c}]}}", lista(abertos))
}

/// **O quê:** imprime o monitor do run (BARATO: só lê o checklist e o log de conclusões).
///
/// **Onde:** `monitor --json`; a janela chama a cada ~3 s enquanto «Acompanhar» está ligado.
/// Os tokens ficam em [`tokens`] à parte, porque esse é PESADO e a janela o chama a cada 30 s.
pub fn monitor() -> Result<(), String> {
    let r = raiz()?;
    // O domínio devolve a LINHA (`- [ ] texto`); o contrato é o TEXTO do item, sem marcador.
    let abertos: Vec<String> = overdev::open_items_at(&r, ITENS_DO_MONITOR)
        .iter()
        .map(|l| l.trim_start().trim_start_matches("- [ ]").trim().to_string())
        .collect();
    let todas = overdev::completions(&r);
    let ini = todas.len().saturating_sub(CONCLUSOES_DO_MONITOR);
    println!("{}", documento_monitor(&abertos, &todas[ini..]));
    Ok(())
}

/// **O quê:** o documento `tokens --json` — função PURA.
///
/// **Onde:** [`tokens`]. Sem transcript tudo é 0 e `modelo` é `null`: zero é verdade (ninguém
/// gastou nada ainda), e `null` diz "não há modelo" sem inventar um.
pub fn documento_tokens(u: &usage::Usage) -> String {
    format!(
        "{{\"entrada\":{},\"saida\":{},\"cache_leitura\":{},\"cache_escrita\":{},\"total\":{},\
         \"mensagens\":{},\"modelo\":{}}}",
        u.input,
        u.output,
        u.cache_read,
        u.cache_write,
        u.total,
        u.messages,
        texto_ou_null(u.main_model().unwrap_or(""))
    )
}

/// **O quê:** imprime os tokens do projeto. **Onde:** `tokens --json`.
///
/// **PESADO** — varre os transcripts do Claude (100 MB+). Por isso é comando à parte do
/// `monitor`, e a janela o chama fora do laço de 3 s e fora da thread da tela.
pub fn tokens() -> Result<(), String> {
    println!("{}", documento_tokens(&usage::agent_usage(&raiz()?)));
    Ok(())
}

/// **O quê:** o documento `arquivos --json` — função PURA sobre `(nome, caminho, existe)`.
///
/// **Onde:** [`arquivos`]. O `caminho` é ABSOLUTO: a janela o entrega ao editor do usuário.
pub fn documento_arquivos(dir: &Path, itens: &[(String, PathBuf, bool)]) -> String {
    let l = itens
        .iter()
        .map(|(n, c, e)| {
            format!(
                "{{\"nome\":\"{}\",\"caminho\":\"{}\",\"existe\":{e}}}",
                esc(n),
                esc(&c.display().to_string())
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"dir\":\"{}\",\"arquivos\":[{l}]}}", esc(&dir.display().to_string()))
}

/// **O quê:** imprime onde estão PLAN.md, CHECKLIST.md e NOTAS.md deste run.
///
/// **Onde:** `arquivos --json`. A janela NÃO edita arquivo (D6): ela abre o caminho no editor do
/// usuário. Quem sabe se o run mora em `.schematize/overdev` ou no `.overdev` legado é
/// [`paths::overdev_dir_at`] — a mesma regra do gate.
pub fn arquivos() -> Result<(), String> {
    let dir = paths::overdev_dir_at(&raiz()?);
    let itens: Vec<(String, PathBuf, bool)> = ARQUIVOS_EDITAVEIS
        .iter()
        .map(|n| {
            let c = dir.join(n);
            (n.to_string(), c.clone(), c.is_file())
        })
        .collect();
    println!("{}", documento_arquivos(&dir, &itens));
    Ok(())
}

/// **O quê:** imprime as notas do humano (`NOTAS.md`) como `{"texto": "…"}`.
///
/// **Onde:** `notas --json`. Sem notas o texto é `""`: é a única chave, e ela é sempre string.
pub fn notas() -> Result<(), String> {
    println!("{{\"texto\":\"{}\"}}", esc(&overdev::read_notes(&raiz()?)));
    Ok(())
}
