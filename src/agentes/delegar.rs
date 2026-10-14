//! Execução de uma delegação: papel → motor/modelo → processo filho, com tudo gravado.
//!
//! **O quê:** resolve o papel, checa disponibilidade, roda o motor com `cwd = projeto`
//! (argv, nunca shell) e grava `delegacoes/<ts>-<papel>.{brief.md,saida.json,log}`.
//! **Onde:** o subcomando `schematize-overdev delegar` (e qualquer agente que o chame).

use super::erro::ErroAgentes;
use super::motores::{disponibilidade, linha_de_comando, resolver_binario};
use super::schema::{Agentes, Motor};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Instant;

/// Quantos caracteres do resultado vão no `saida_resumo`.
const RESUMO_MAX: usize = 600;

/// O que o chamador pede.
#[derive(Debug, Clone)]
pub struct Pedido {
    pub papel: String,
    pub brief: String,
    pub projeto: PathBuf,
}

/// Resultado de uma execução (ok ou motor falhou).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resultado {
    pub ok: bool,
    pub papel: String,
    pub motor: Motor,
    pub modelo: String,
    /// Código de saída do motor (`-1` se morto por sinal).
    pub rc: i32,
    pub duracao_ms: u128,
    pub saida_resumo: String,
    /// Prefixo dos arquivos gravados, no formato `<dir>/<ts>-<papel>.{brief.md,saida.json,log}`.
    pub arquivos: String,
}

/// Desfecho de [`delegar`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Desfecho {
    /// O motor rodou (`ok` diz se saiu com 0).
    Executado(Resultado),
    /// Motor indisponível ou papel inativo — nada foi executado; `proximo` é a sugestão.
    Indisponivel { detalhe: String, proximo: Option<String> },
}

/// **O quê:** o próximo papel ATIVO da escada depois de `papel` (ou `None` se `papel` não está
/// na escada ou era o último). **Onde:** `delegar` (rc 3) e a CLI.
pub fn proximo_ativo(ag: &Agentes, papel: &str) -> Option<String> {
    let pos = ag.escada.iter().position(|p| p == papel)?;
    ag.escada[pos + 1..].iter().find(|p| ag.papel(p).is_some_and(|x| x.ativo)).cloned()
}

/// **O quê:** como [`proximo_ativo`], mas também pula os papéis cujo motor NÃO está disponível
/// (`disponivel` é injetado: a CLI passa [`disponibilidade`], os testes passam um closure fixo).
/// **Onde:** [`delegar`] (campo `proximo` do rc 3), para o orquestrador não bater em rc 3 em loop.
pub fn proximo_disponivel(
    ag: &Agentes,
    papel: &str,
    disponivel: impl Fn(Motor) -> bool,
) -> Option<String> {
    let pos = ag.escada.iter().position(|p| p == papel)?;
    ag.escada[pos + 1..]
        .iter()
        .find(|p| ag.papel(p).is_some_and(|x| x.ativo && disponivel(x.motor)))
        .cloned()
}

/// **O quê:** carimbo `AAAAMMDD-HHMMSSmmm` local, para nome de arquivo único e ordenável.
/// **Onde:** [`delegar`].
fn carimbo() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S%3f").to_string()
}

/// **O quê:** corta `s` em `max` caracteres (em fronteira de char). **Onde:** [`resumo`].
fn corta(s: &str, max: usize) -> String {
    let t: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        format!("{t}…")
    } else {
        t
    }
}

/// **O quê:** o resumo legível da saída do motor: o campo `result` do JSON do claude/cursor,
/// o último `agent_message` do JSONL do codex, ou o começo do texto cru.
/// **Onde:** [`delegar`].
fn resumo(saida: &str, log: &str) -> String {
    let doc = serde_json::from_str::<serde_json::Value>(saida.trim()).ok();
    if let Some(r) = doc.as_ref().and_then(|d| d.get("result")).and_then(|r| r.as_str()) {
        return corta(r, RESUMO_MAX);
    }
    let ultimo_codex = saida
        .lines()
        .rev()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find_map(|v| {
            let it = v.get("item")?;
            (it.get("type")?.as_str()? == "agent_message")
                .then(|| it.get("text")?.as_str().map(String::from))?
        });
    if let Some(t) = ultimo_codex {
        return corta(&t, RESUMO_MAX);
    }
    let base = if saida.trim().is_empty() { log } else { saida };
    corta(base.trim(), RESUMO_MAX)
}

/// **O quê:** executa a delegação. `Err` = uso/config inválida (rc 2 na CLI); o resto é
/// [`Desfecho`]. **Onde:** CLI `delegar`.
pub fn delegar(ag: &Agentes, pedido: &Pedido) -> Result<Desfecho, ErroAgentes> {
    let papel = ag
        .papel(&pedido.papel)
        .ok_or_else(|| ErroAgentes::PapelDesconhecido(pedido.papel.clone()))?;
    if !pedido.projeto.is_dir() {
        return Err(ErroAgentes::Execucao(format!(
            "projeto `{}` não é um diretório",
            pedido.projeto.display()
        )));
    }
    let proximo = || proximo_disponivel(ag, &pedido.papel, |m| disponibilidade(m).ok());
    if !papel.ativo {
        return Ok(Desfecho::Indisponivel { detalhe: "papel inativo".into(), proximo: proximo() });
    }
    let disp = disponibilidade(papel.motor);
    let Some(bin) = resolver_binario(papel.motor).filter(|_| disp.ok()) else {
        let d = format!("motor {}: {}", papel.motor.nome(), disp.detalhe());
        return Ok(Desfecho::Indisponivel { detalhe: d, proximo: proximo() });
    };

    let dir = crate::paths::overdev_dir_at(&pedido.projeto).join("delegacoes");
    let io = |caminho: &Path, e: std::io::Error| ErroAgentes::Io {
        caminho: caminho.to_path_buf(),
        msg: e.to_string(),
    };
    std::fs::create_dir_all(&dir).map_err(|e| io(&dir, e))?;
    let base = format!("{}-{}", carimbo(), pedido.papel);
    let (f_brief, f_saida, f_log) = (
        dir.join(format!("{base}.brief.md")),
        dir.join(format!("{base}.saida.json")),
        dir.join(format!("{base}.log")),
    );
    std::fs::write(&f_brief, &pedido.brief).map_err(|e| io(&f_brief, e))?;
    let saida = std::fs::File::create(&f_saida).map_err(|e| io(&f_saida, e))?;
    let log = std::fs::File::create(&f_log).map_err(|e| io(&f_log, e))?;

    let argv = linha_de_comando(papel.motor, &papel.modelo, &pedido.projeto, &pedido.brief);
    let t0 = Instant::now();
    let status = std::process::Command::new(&bin)
        .args(&argv)
        .current_dir(&pedido.projeto)
        .stdin(Stdio::null())
        .stdout(saida)
        .stderr(log)
        .status()
        .map_err(|e| ErroAgentes::Execucao(format!("spawn de {}: {e}", bin.display())))?;
    let duracao_ms = t0.elapsed().as_millis();

    let txt_saida =
        std::fs::read_to_string(&f_saida).unwrap_or_else(|e| format!("(saída ilegível: {e})"));
    let txt_log =
        std::fs::read_to_string(&f_log).unwrap_or_else(|e| format!("(log ilegível: {e})"));
    Ok(Desfecho::Executado(Resultado {
        ok: status.success(),
        papel: pedido.papel.clone(),
        motor: papel.motor,
        modelo: papel.modelo.clone(),
        rc: status.code().unwrap_or(-1),
        duracao_ms,
        saida_resumo: resumo(&txt_saida, &txt_log),
        arquivos: format!("{}/{base}.{{brief.md,saida.json,log}}", dir.display()),
    }))
}
