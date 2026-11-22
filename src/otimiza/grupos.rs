//! Flags fixas por grupo de cache (IA-08).
//!
//! **O quê:** dentro de um grupo de cache, modelo, effort, tools e MCPs são imutáveis; mudar
//! qualquer um CRIA outro grupo (ou, ao admitir direto, é rejeitado com a diferença).
//! **Onde:** agrupador de unidades da onda, antes do lançamento escalonado (`escalonado`):
//! mudar effort/tools no meio invalida partes do cache de prompt.

use std::collections::BTreeSet;

/// Tudo que invalida o prefixo em cache se mudar.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Chave {
    /// Modelo (normalizado: aparado, minúsculo).
    pub modelo: String,
    /// Effort (normalizado).
    pub effort: String,
    /// Ferramentas habilitadas.
    pub tools: BTreeSet<String>,
    /// MCPs habilitados.
    pub mcps: BTreeSet<String>,
}

impl Chave {
    /// Constrói normalizando texto e conjuntos. **Onde:** quem monta unidades da onda.
    pub fn nova(modelo: &str, effort: &str, tools: &[&str], mcps: &[&str]) -> Chave {
        let set = |l: &[&str]| l.iter().map(|s| s.trim().to_lowercase()).collect();
        Chave {
            modelo: modelo.trim().to_lowercase(),
            effort: effort.trim().to_lowercase(),
            tools: set(tools),
            mcps: set(mcps),
        }
    }
}

/// Unidade candidata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unidade {
    /// Id.
    pub id: String,
    /// Flags de cache da unidade.
    pub chave: Chave,
}

/// Um grupo: mesma chave, ids na ordem de entrada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grupo {
    /// Chave imutável do grupo.
    pub chave: Chave,
    /// Unidades do grupo.
    pub ids: Vec<String>,
}

/// Admissão recusada: o que difere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mistura {
    /// Unidade recusada.
    pub id: String,
    /// Campos diferentes (`modelo`, `effort`, `tools`, `mcps`).
    pub diferencas: Vec<&'static str>,
}

/// Campos que diferem entre duas chaves. **Onde:** [`Grupo::admitir`].
pub fn diferencas(a: &Chave, b: &Chave) -> Vec<&'static str> {
    let mut d = Vec::new();
    for (nome, difere) in [
        ("modelo", a.modelo != b.modelo),
        ("effort", a.effort != b.effort),
        ("tools", a.tools != b.tools),
        ("mcps", a.mcps != b.mcps),
    ] {
        if difere {
            d.push(nome);
        }
    }
    d
}

impl Grupo {
    /// Admite `u` só se a chave é idêntica; senão `Err(Mistura)` e o grupo fica intacto.
    /// **Onde:** quem monta grupos à mão (a CLI) em vez de [`agrupar`].
    pub fn admitir(&mut self, u: &Unidade) -> Result<(), Mistura> {
        let d = diferencas(&self.chave, &u.chave);
        if d.is_empty() {
            self.ids.push(u.id.clone());
            Ok(())
        } else {
            Err(Mistura { id: u.id.clone(), diferencas: d })
        }
    }
}

/// Separa as unidades em grupos por chave, preservando a ordem da primeira aparição.
///
/// **O quê:** determinístico (sem HashMap). **Onde:** início da onda.
pub fn agrupar(us: &[Unidade]) -> Vec<Grupo> {
    let mut gs: Vec<Grupo> = Vec::new();
    for u in us {
        match gs.iter_mut().find(|g| g.chave == u.chave) {
            Some(g) => g.ids.push(u.id.clone()),
            None => gs.push(Grupo { chave: u.chave.clone(), ids: vec![u.id.clone()] }),
        }
    }
    gs
}

#[cfg(test)]
mod testes {
    use super::*;

    fn u(id: &str, effort: &str, tools: &[&str]) -> Unidade {
        Unidade { id: id.into(), chave: Chave::nova("Sonnet", effort, tools, &[]) }
    }

    #[test]
    fn effort_diferente_separa_em_outro_grupo() {
        let gs = agrupar(&[
            u("a", "low", &["Read"]),
            u("b", "high", &["Read"]),
            u("c", "LOW ", &["read"]),
        ]);
        assert_eq!(gs.len(), 2);
        assert_eq!(gs[0].ids, ["a", "c"], "normalizacao junta LOW/low");
        assert_eq!(gs[1].ids, ["b"]);
    }

    #[test]
    fn misturar_no_mesmo_grupo_e_rejeitado_e_nao_suja() {
        let mut g = Grupo { chave: u("a", "low", &["Read"]).chave, ids: vec!["a".into()] };
        assert!(g.admitir(&u("b", "low", &["Read"])).is_ok());
        let e = g.admitir(&u("c", "high", &["Read", "Bash"])).unwrap_err();
        assert_eq!(e.diferencas, ["effort", "tools"]);
        assert_eq!(g.ids, ["a", "b"]);
        assert!(agrupar(&[]).is_empty());
    }
}
