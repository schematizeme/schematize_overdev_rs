//! Camada Claude: `claude -p` headless, modelo barato, jaulado, com validação da saída.
//!
//! **O quê:** [`construir_prompt`] (prefixo estável + resumo do estado), [`executar`] (roda o
//! comando jaulado com timeout, valida/redige a saída, estima tokens) e [`backoff_s`]. Nunca
//! pergunta nada; falha/timeout/morte por OOM viram `Err(motivo)` — o motor registra e segue.
//! **Onde:** [`super::ciclo::Motor`] roda [`executar`] numa thread (nunca bloqueia o tick mecânico).
//! Não grava em `jaulas.jsonl` (o supervisor conta mortes ali para re-decomposição do overdev).

use super::config::Config;
use super::filtro::validar_notas;
use super::proc::{rodar, Pedido};
use crate::otimiza::cache::{cache_ainda_quente, TTL_PADRAO_SEG};
use crate::otimiza::ledger::Uso;
use schematize_plataforma::jaula::comando::comando_jaulado;
use schematize_plataforma::jaula::{detectar_backend, Backend, TipoExecucao};
use std::time::Duration;

/// Prefixo ESTÁVEL do prompt (byte a byte igual em toda chamada: é o que o prompt cache reaproveita).
pub const PREFIXO_ESTAVEL: &str = "Você escreve NOTAS DE HANDOFF de um projeto de software para a próxima sessão.\n\
O estado mecânico (git, checklist, relatórios) já está no documento; NÃO o repita.\n\
Responda SOMENTE em Markdown pt-BR, com estas 4 seções curtas e nesta ordem:\n\
### Decisões e motivos\n### Hipóteses em aberto\n### Próximo passo imediato\n### Armadilhas\n\
Regras: no máximo 1500 caracteres; nunca faça perguntas; se não houver informação, escreva \"sem informação\";\n\
nunca inclua segredos, chaves, tokens, senhas nem e-mails.\n\
--- estado atual do workspace (somente leitura) ---\n";

/// Limite do resumo enviado (bytes).
const MAX_RESUMO: usize = 8000;

/// Resultado de uma chamada.
#[derive(Debug)]
pub struct ResClaude {
    /// Notas validadas (e quantos segredos foram redigidos) ou o motivo da falha.
    pub resultado: Result<(String, usize), String>,
    /// Tokens estimados (para o ledger).
    pub uso: Uso,
    /// `true` se o processo chegou a rodar (entrada foi consumida).
    pub rodou: bool,
}

/// **O quê:** prefixo estável + `resumo` truncado em fronteira de char. **Onde:** [`super::ciclo`]. PURA.
pub fn construir_prompt(resumo: &str) -> String {
    let mut fim = resumo.len().min(MAX_RESUMO);
    while !resumo.is_char_boundary(fim) {
        fim -= 1;
    }
    format!("{PREFIXO_ESTAVEL}{}\n", &resumo[..fim])
}

/// **O quê:** espera após `falhas` falhas seguidas: `base * 2^falhas`, teto de 4 h. **Onde:** motor. PURA.
pub fn backoff_s(base: u64, falhas: u32) -> u64 {
    base.saturating_mul(1u64 << falhas.min(16)).min(4 * 3600)
}

/// **O quê:** estimativa de tokens (~4 bytes/token). **Onde:** ledger. PURA.
fn tokens(bytes: usize) -> u64 {
    (bytes as u64).div_ceil(4)
}

/// **O quê:** o argv completo (jaulado quando for o `claude` real) e o limite de `RLIMIT_AS`.
/// **Onde:** [`executar`]. `Err` acionável se não há como impor teto (nunca roda sem teto em silêncio).
fn montar_argv(cfg: &Config, unidade: &str) -> Result<(Vec<String>, Option<u64>), String> {
    if let Some(cmd) = &cfg.claude_cmd {
        return Ok((cmd.clone(), Some(cfg.claude_mem_mib)));
    }
    let claude = crate::agentrun::claude_path().ok_or("`claude` não encontrado no PATH")?;
    let argv: Vec<String> =
        [claude.to_string_lossy().as_ref(), "-p", "--model", &cfg.modelo, "--max-turns", "1"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
    let orc = crate::agentrun::jaulado::orcamento_para(TipoExecucao::AgenteLlm);
    match detectar_backend() {
        Backend::SystemdScope => Ok((comando_jaulado(&orc, &argv, unidade), None)),
        // RLIMIT_AS não serve ao `claude` (Node reserva endereço virtual enorme): só systemd-run é jaula.
        _ if crate::agentrun::jaulado::sem_jaula_por_env() => Ok((argv, None)),
        _ => Err(crate::agentrun::jaulado::mensagem_sem_backend(
            "não há `systemd-run --user` com cgroup v2.",
        )),
    }
}

/// **O quê:** roda o Claude com `prompt`, valida e redige a saída. `ultimo_uso` (unix s) decide
/// se o prefixo conta como cache quente no ledger. **Onde:** thread do motor. **Efeitos:**
/// processo filho (jaulado), cria o cwd neutro em `.schematize/handoff-vivo/claude-cwd`.
pub fn executar(cfg: &Config, prompt: &str, agora: u64, ultimo_uso: Option<u64>) -> ResClaude {
    let falha =
        |m: String, rodou: bool| ResClaude { resultado: Err(m), uso: Uso::default(), rodou };
    let cwd = cfg.dir_operacional().join("claude-cwd");
    if let Err(e) = std::fs::create_dir_all(&cwd) {
        return falha(format!("criar {}: {e}", cwd.display()), false);
    }
    let unidade = crate::agentrun::jaulado::nome_da_unidade(
        &cfg.projeto,
        &format!("hv-{}-{agora}", std::process::id()),
    );
    let (argv, limite_as) = match montar_argv(cfg, &unidade) {
        Ok(x) => x,
        Err(e) => return falha(e, false),
    };
    let env = [("SCHEMATIZE_HANDOFF_VIVO".to_string(), "1".to_string())];
    let max_bruto = cfg.max_notas.saturating_mul(4).max(4096);
    let ped = Pedido {
        argv: &argv,
        env: &env,
        cwd: &cwd,
        stdin: Some(prompt.as_bytes()),
        timeout: Duration::from_secs(cfg.claude_timeout_s),
        teto_stdout: max_bruto + 1,
        limite_as_mib: limite_as,
    };
    let saida = match rodar(&ped) {
        Ok(s) => s,
        Err(e) => return falha(e, false),
    };
    let quente = ultimo_uso.is_some_and(|u| cache_ainda_quente(u, agora, TTL_PADRAO_SEG));
    let pref = tokens(PREFIXO_ESTAVEL.len());
    let mut uso = Uso {
        input: tokens(prompt.len()).saturating_sub(pref),
        output: tokens(saida.stdout.len()),
        ..Uso::default()
    };
    if quente {
        uso.cache_read = pref
    } else {
        uso.cache_write = pref
    }
    let resultado = if saida.timeout {
        Err(format!("timeout após {}s (processo morto)", cfg.claude_timeout_s))
    } else if saida.sinal == Some(9) || saida.codigo == Some(137) {
        Err(format!(
            "morto por SIGKILL (provável estouro do teto de memória; {})",
            saida.stderr_cauda
        ))
    } else if saida.codigo != Some(0) {
        Err(format!("saiu com {:?}/sinal {:?}: {}", saida.codigo, saida.sinal, saida.stderr_cauda))
    } else {
        validar_notas(&saida.stdout, cfg.max_notas, max_bruto)
            .map_err(|r| format!("saída recusada: {r}"))
    };
    ResClaude { resultado, uso, rodou: true }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn prompt_tem_prefixo_estavel_e_trunca() {
        let a = construir_prompt("estado A");
        let b = construir_prompt("estado B");
        assert!(a.starts_with(PREFIXO_ESTAVEL) && b.starts_with(PREFIXO_ESTAVEL));
        assert!(
            construir_prompt(&"ã".repeat(20_000)).len() <= PREFIXO_ESTAVEL.len() + MAX_RESUMO + 1
        );
    }

    #[test]
    fn backoff_dobra_e_tem_teto() {
        assert_eq!(backoff_s(1800, 0), 1800);
        assert_eq!(backoff_s(1800, 1), 3600);
        assert_eq!(backoff_s(1800, 10), 4 * 3600);
        assert_eq!(backoff_s(u64::MAX, 3), 4 * 3600);
    }
}
