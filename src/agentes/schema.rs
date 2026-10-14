//! Forma da config de agentes + os DEFAULTS embutidos (tabela do CONTRATO, ADR-0024).
//!
//! **O quê:** [`Motor`], [`Papel`], [`Agentes`], a validação de modelo e a serialização TOML.
//! **Onde:** `carga` (parse/merge), `delegar`, o prompt do overdev e a janela.

use super::erro::ErroAgentes;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Motores suportados (linha de comando não-interativa de cada um em [`super::linha_de_comando`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Motor {
    Claude,
    Codex,
    Cursor,
}

impl Motor {
    /// Todos os motores, na ordem de exibição.
    pub const TODOS: [Motor; 3] = [Motor::Claude, Motor::Codex, Motor::Cursor];

    /// **O quê:** o nome do motor (= nome do binário principal, exceto cursor).
    /// **Onde:** TOML, JSON da CLI e tabela do prompt.
    pub fn nome(self) -> &'static str {
        match self {
            Motor::Claude => "claude",
            Motor::Codex => "codex",
            Motor::Cursor => "cursor",
        }
    }

    /// **O quê:** o binário que o motor executa (`cursor` → `cursor-agent`).
    /// **Onde:** resolução de disponibilidade e execução.
    pub fn binario(self) -> &'static str {
        match self {
            Motor::Claude => "claude",
            Motor::Codex => "codex",
            Motor::Cursor => "cursor-agent",
        }
    }

    /// **O quê:** parse estrito do nome (minúsculo exato). **Onde:** `carga`.
    pub fn tentar(s: &str) -> Option<Motor> {
        Motor::TODOS.into_iter().find(|m| m.nome() == s)
    }
}

/// Um papel: quem executa (motor+modelo), se está ligado e para quê serve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Papel {
    pub motor: Motor,
    pub modelo: String,
    pub ativo: bool,
    #[serde(default)]
    pub descricao: String,
}

/// A config efetiva: escada de escalonamento, roteamento por tipo de tarefa e os papéis.
///
/// A ordem dos campos importa para o TOML: valores simples antes das tabelas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agentes {
    pub versao: u32,
    pub escada: Vec<String>,
    pub roteamento: BTreeMap<String, String>,
    pub papeis: BTreeMap<String, Papel>,
}

/// **O quê:** valida o nome de um papel (`[a-z0-9-]{1,32}`). **Onde:** `carga` e `validar`.
pub(crate) fn nome_valido(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 32
        && n.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

impl Agentes {
    /// **O quê:** os defaults embutidos — IDÊNTICOS à tabela do contrato.
    /// **Onde:** base do merge e fallback quando a config é inválida.
    pub fn padrao() -> Agentes {
        let p = |motor, modelo: &str, d: &str| Papel {
            motor,
            modelo: modelo.to_string(),
            ativo: true,
            descricao: d.to_string(),
        };
        let papeis = BTreeMap::from([
            ("principal", p(Motor::Claude, "opus", "Orquestrador: planeja, decompõe, despacha, revisa. NÃO gera código de entrega.")),
            ("trivial", p(Motor::Claude, "haiku", "Task muito simples/mecânica (localizar, renomear, editar 1-2 linhas). ~1/100 do custo do Opus.")),
            ("executor", p(Motor::Claude, "sonnet", "Padrão de toda micro-task de código. Até 2 rodadas de correção.")),
            ("fallback-1", p(Motor::Codex, "gpt-5.6-terra", "Executor falhou 2 rodadas.")),
            ("fallback-cursor", p(Motor::Cursor, "auto", "Codex falhou. Pulado com aviso se cursor-agent não estiver instalado/logado.")),
            ("fallback-2", p(Motor::Claude, "opus", "Último recurso (subagent Opus). Motivo registrado no checkpoint.")),
            ("visual", p(Motor::Codex, "gpt-5.6-terra", "UI, layout, CSS, tema, ícone, regressão visual. Claude não faz visual.")),
            ("imagem", p(Motor::Codex, "gpt-5.6-terra", "Gerar/editar imagem PEDINDO ao Codex Terra (cota da assinatura). PROIBIDO endpoint/API de imagem pago.")),
            ("complementar", p(Motor::Claude, "sonnet", "2ª opinião, revisão, testes, docs.")),
        ])
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        Agentes {
            versao: 1,
            escada: ["trivial", "executor", "fallback-1", "fallback-cursor", "fallback-2"]
                .map(String::from)
                .to_vec(),
            roteamento: BTreeMap::from([
                ("visual".to_string(), "visual".to_string()),
                ("imagem".to_string(), "imagem".to_string()),
            ]),
            papeis,
        }
    }

    /// **O quê:** o papel `nome`, se existir. **Onde:** `delegar`, runner, janela.
    pub fn papel(&self, nome: &str) -> Option<&Papel> {
        self.papeis.get(nome)
    }

    /// **O quê:** a escada de escalonamento (ordem de tentativa). **Onde:** prompt e `delegar`.
    pub fn escada(&self) -> &[String] {
        &self.escada
    }

    /// **O quê:** o modelo do papel `principal` (cai em `opus` se, por construção inválida,
    /// ele não existir — `validar` já barra isso). **Onde:** `--model` do runner.
    pub fn modelo_principal(&self) -> &str {
        self.papel("principal").map(|p| p.modelo.as_str()).unwrap_or("opus")
    }

    /// **O quê:** valida invariantes da config já montada. Nunca entra em pânico.
    /// **Onde:** `carregar`, `salvar_*` e a janela (antes de oferecer "salvar").
    pub fn validar(&self) -> Result<(), ErroAgentes> {
        if self.versao != 1 {
            return Err(ErroAgentes::Versao(i64::from(self.versao)));
        }
        if !self.papeis.contains_key("principal") {
            return Err(ErroAgentes::SemPrincipal);
        }
        for (nome, p) in &self.papeis {
            if !nome_valido(nome) {
                return Err(ErroAgentes::NomeInvalido(nome.clone()));
            }
            if p.modelo.trim().is_empty() {
                return Err(ErroAgentes::ModeloVazio(nome.clone()));
            }
        }
        for r in &self.escada {
            if !self.papeis.contains_key(r) {
                return Err(ErroAgentes::PapelAusente { onde: "escada".into(), papel: r.clone() });
            }
        }
        for (tipo, r) in &self.roteamento {
            if !nome_valido(tipo) {
                return Err(ErroAgentes::NomeInvalido(tipo.clone()));
            }
            if !self.papeis.contains_key(r) {
                return Err(ErroAgentes::PapelAusente {
                    onde: format!("roteamento.{tipo}"),
                    papel: r.clone(),
                });
            }
        }
        Ok(())
    }

    /// **O quê:** serializa para TOML (um campo por linha, como o arquivo real).
    /// **Onde:** `salvar_*` e testes de roundtrip.
    pub fn para_toml(&self) -> Result<String, ErroAgentes> {
        toml::to_string_pretty(self).map_err(|e| ErroAgentes::Serializar(e.to_string()))
    }
}
