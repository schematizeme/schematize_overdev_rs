//! O plano de um run: arquivo JSON com os itens (id, brief, tipo, ...) e as opções de execução.
//!
//! **O quê:** [`Plano`] (lido de `--plano <arquivo>`), validado: ids únicos e sem NUL.
//! **Onde:** `overdev executar`. Na retomada o plano NÃO é necessário: o estado já guarda os briefs.

use super::modelo::{Evento, ItemPlano};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Plano de execução.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plano {
    /// Prefixo de prompt estável (cacheável) comum a todas as unidades.
    #[serde(default)]
    pub prefixo: String,
    /// Modelo inicial da escada.
    #[serde(default = "modelo_padrao")]
    pub modelo: String,
    /// Unidades em paralelo no máximo.
    #[serde(default = "paralelo_padrao")]
    pub paralelo: usize,
    /// Margem do host (MiB) que nunca se compromete; padrão = `sistema::margem_segura`.
    #[serde(default)]
    pub margem_mib: Option<u64>,
    /// Itens do checklist.
    pub itens: Vec<ItemPlano>,
}

/// **O quê:** modelo inicial padrão. **Onde:** `serde(default)`.
fn modelo_padrao() -> String {
    "sonnet".to_string()
}

/// **O quê:** paralelismo padrão. **Onde:** `serde(default)`.
fn paralelo_padrao() -> usize {
    2
}

impl Plano {
    /// **O quê:** valida o plano (ids vazios/duplicados/com NUL, paralelo 0). **Onde:** `carregar`.
    pub fn validar(&self) -> Result<(), String> {
        let mut vistos = std::collections::BTreeSet::new();
        for i in &self.itens {
            if i.id.trim().is_empty() || i.id.contains('\0') {
                return Err(format!("item com id inválido: {:?}", i.id));
            }
            if !vistos.insert(i.id.as_str()) {
                return Err(format!("id duplicado no plano: {}", i.id));
            }
        }
        if self.paralelo == 0 {
            return Err("paralelo precisa ser >= 1".to_string());
        }
        Ok(())
    }

    /// **O quê:** lê e valida o arquivo. **Onde:** `overdev executar --plano`. Erros acionáveis.
    pub fn carregar(caminho: &Path) -> Result<Plano, String> {
        let t = std::fs::read_to_string(caminho)
            .map_err(|e| format!("não consegui ler o plano {}: {e}", caminho.display()))?;
        let p: Plano = serde_json::from_str(&t)
            .map_err(|e| format!("plano {} inválido (JSON esperado): {e}", caminho.display()))?;
        p.validar()?;
        Ok(p)
    }

    /// **O quê:** o evento `Plano` deste plano. **Onde:** registrado no início do `executar`.
    pub fn evento(&self) -> Evento {
        Evento::Plano {
            prefixo: self.prefixo.clone(),
            modelo_base: self.modelo.clone(),
            itens: self.itens.clone(),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Plano mínimo carrega com defaults; id duplicado/vazio e JSON ruim viram erro.
    #[test]
    fn carrega_defaults_e_rejeita_hostil() {
        let ok = r#"{"itens":[{"id":"a","brief":"x"},{"id":"b","brief":"y","argv":["true"]}]}"#;
        let p: Plano = serde_json::from_str(ok).unwrap();
        assert!(p.validar().is_ok());
        assert_eq!(
            (p.modelo.as_str(), p.paralelo, p.itens[0].tipo.as_str()),
            ("sonnet", 2, "geral")
        );
        let dup = r#"{"itens":[{"id":"a","brief":"x"},{"id":"a","brief":"y"}]}"#;
        assert!(serde_json::from_str::<Plano>(dup).unwrap().validar().is_err());
        let vazio = r#"{"itens":[{"id":" ","brief":"x"}]}"#;
        assert!(serde_json::from_str::<Plano>(vazio).unwrap().validar().is_err());
        assert!(Plano::carregar(Path::new("/nao/existe.json")).is_err());
    }
}
