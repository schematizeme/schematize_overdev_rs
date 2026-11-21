//! Checkpoint idempotente por unidade (IA-01/IA-02): não repetir trabalho já entregue.
//!
//! **O quê:** mapa unidade -> (hash do brief, estado) persistido em JSON atômico.
//! **Onde:** orquestrador, no início do run e após a morte de uma unidade (OOM de uma não
//! refaz as outras). Uma unidade `Entregue` só é reaproveitada se o hash do brief não mudou.

use super::fnv64;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Estado de uma unidade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Estado {
    /// Ainda não lançada.
    Pendente,
    /// Lançada (se o orquestrador morreu, está obsoleta).
    Rodando,
    /// Entregue e revisada.
    Entregue,
    /// Falhou.
    Falhou,
    /// Morta por sinal/OOM.
    Morta,
}

/// Entrada de uma unidade.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entrada {
    /// Hash do brief com que foi lançada.
    pub brief_hash: u64,
    /// Estado atual.
    pub estado: Estado,
}

/// Checkpoint de um run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Unidades por id.
    pub unidades: BTreeMap<String, Entrada>,
}

/// Hash do brief (FNV-1a 64). **Onde:** chave de idempotência.
pub fn hash_brief(brief: &str) -> u64 {
    fnv64(brief.as_bytes())
}

impl Checkpoint {
    /// Marca o estado de uma unidade (idempotente).
    ///
    /// **O quê:** com o MESMO hash, `Entregue` nunca regride (reentrega/relançamento tardio é
    /// ignorado); com hash diferente, o brief mudou e a entrada é substituída.
    /// **Onde:** antes de lançar (`Rodando`) e ao fim (`Entregue`/`Falhou`/`Morta`).
    pub fn marcar(&mut self, id: &str, brief_hash: u64, estado: Estado) {
        if let Some(e) = self.unidades.get(id) {
            if e.brief_hash == brief_hash && e.estado == Estado::Entregue {
                return;
            }
        }
        self.unidades.insert(id.to_string(), Entrada { brief_hash, estado });
    }

    /// Carrega do disco. Arquivo ausente = checkpoint vazio; corrompido = `Err` (não apaga nada).
    /// **Onde:** início do run.
    pub fn carregar(caminho: &Path) -> Result<Checkpoint, String> {
        match std::fs::read_to_string(caminho) {
            Ok(s) => serde_json::from_str(&s).map_err(|e| format!("checkpoint corrompido: {e}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Checkpoint::default()),
            Err(e) => Err(format!("checkpoint ilegível: {e}")),
        }
    }

    /// Salva de forma atômica (tmp + rename). **Onde:** a cada mudança de estado.
    pub fn salvar(&self, caminho: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = caminho.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| format!("gravar {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, caminho).map_err(|e| format!("renomear: {e}"))
    }
}

/// Unidades NÃO entregues do checkpoint (pendente, rodando obsoleta, falhou, morta).
/// **Onde:** retomada sem os briefs à mão.
pub fn unidades_pendentes(c: &Checkpoint) -> Vec<String> {
    c.unidades
        .iter()
        .filter(|(_, e)| e.estado != Estado::Entregue)
        .map(|(k, _)| k.clone())
        .collect()
}

/// Pendentes para o plano atual: `plano` = (id, brief). Unidade desconhecida, com brief alterado
/// ou não entregue é pendente; entregue com mesmo hash é pulada. Ordem do plano preservada.
/// **Onde:** replay do plano após morte de unidade/orquestrador.
pub fn pendentes_do_plano(c: &Checkpoint, plano: &[(String, String)]) -> Vec<String> {
    plano
        .iter()
        .filter(|(id, brief)| {
            !matches!(c.unidades.get(id), Some(e)
                if e.estado == Estado::Entregue && e.brief_hash == hash_brief(brief))
        })
        .map(|(id, _)| id.clone())
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn plano() -> Vec<(String, String)> {
        vec![
            ("a".into(), "brief a".into()),
            ("b".into(), "brief b".into()),
            ("c".into(), "ç".into()),
        ]
    }

    #[test]
    fn oom_de_uma_nao_refaz_as_outras() {
        let mut c = Checkpoint::default();
        for (id, b) in plano() {
            c.marcar(&id, hash_brief(&b), Estado::Rodando);
        }
        c.marcar("a", hash_brief("brief a"), Estado::Entregue);
        c.marcar("b", hash_brief("brief b"), Estado::Morta);
        assert_eq!(pendentes_do_plano(&c, &plano()), vec!["b", "c"]);
        assert_eq!(unidades_pendentes(&c), vec!["b", "c"]);
    }

    #[test]
    fn brief_alterado_invalida_entregue() {
        let mut c = Checkpoint::default();
        c.marcar("a", hash_brief("velho"), Estado::Entregue);
        assert_eq!(pendentes_do_plano(&c, &[("a".into(), "novo".into())]), vec!["a"]);
    }

    #[test]
    fn entregue_nao_regride_com_mesmo_hash() {
        let mut c = Checkpoint::default();
        let h = hash_brief("x");
        c.marcar("a", h, Estado::Entregue);
        c.marcar("a", h, Estado::Rodando);
        assert_eq!(c.unidades["a"].estado, Estado::Entregue);
        c.marcar("a", hash_brief("y"), Estado::Rodando);
        assert_eq!(c.unidades["a"].estado, Estado::Rodando);
    }

    #[test]
    fn vazio_e_persistencia() {
        assert!(unidades_pendentes(&Checkpoint::default()).is_empty());
        assert!(pendentes_do_plano(&Checkpoint::default(), &[]).is_empty());
        let dir = std::env::temp_dir().join(format!("otz-ret-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("cp.json");
        assert_eq!(Checkpoint::carregar(&p).unwrap(), Checkpoint::default());
        let mut c = Checkpoint::default();
        c.marcar("ü", 1, Estado::Entregue);
        c.salvar(&p).unwrap();
        assert_eq!(Checkpoint::carregar(&p).unwrap(), c);
        std::fs::write(&p, "{truncado").unwrap();
        assert!(Checkpoint::carregar(&p).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
