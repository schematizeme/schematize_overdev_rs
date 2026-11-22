//! Lançamento escalonado da onda: aquecer o cache antes dos demais (IA-06).
//!
//! **O quê:** por grupo de cache (`grupos`), lança 1 processo (o pioneiro), espera o primeiro
//! evento do stream e só então lança os outros — evita pagar N escritas de cache iguais em
//! paralelo. Timeout na espera NÃO trava a onda: registra aviso e segue (cache pode não estar quente).
//! **Onde:** scheduler de ondas. Não lança processo sozinho: o scheduler injeta um [`Lancador`]
//! (o gancho; o supervisor de `resiliencia` pode implementá-lo sem este módulo conhecê-lo).

use super::grupos::Grupo;
use std::time::Duration;

/// Quem lança e observa unidades. **Onde:** implementado pelo supervisor (e por falsos nos testes).
pub trait Lancador {
    /// Handle de um filho em execução.
    type Filho;
    /// Lança a unidade `id`.
    fn lancar(&mut self, id: &str) -> Result<Self::Filho, String>;
    /// Espera o primeiro evento do stream do filho por até `limite`. `Ok(false)` = não veio.
    fn primeiro_evento(&mut self, f: &mut Self::Filho, limite: Duration) -> Result<bool, String>;
}

/// Passo registrado (ordem real de execução).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Passo {
    /// Lançou a unidade (`true` = pioneiro do grupo).
    Lancou(String, bool),
    /// Esperou o primeiro evento do pioneiro (`true` = chegou).
    Esperou(String, bool),
    /// Falha ao lançar ou observar (não aborta a onda).
    Falhou(String, String),
}

/// Resultado do escalonamento: filhos vivos e trilha.
#[derive(Debug)]
pub struct Escalonado<F> {
    /// Filhos lançados, na ordem de lançamento.
    pub filhos: Vec<(String, F)>,
    /// Trilha de passos.
    pub passos: Vec<Passo>,
}

/// Lança o pioneiro do grupo; se falhar, promove o próximo. **Onde:** [`lancar_escalonado`].
fn pioneiro<L: Lancador>(l: &mut L, g: &Grupo, out: &mut Escalonado<L::Filho>) -> Option<usize> {
    for (i, id) in g.ids.iter().enumerate() {
        match l.lancar(id) {
            Ok(f) => {
                out.passos.push(Passo::Lancou(id.clone(), true));
                out.filhos.push((id.clone(), f));
                return Some(i);
            }
            Err(e) => out.passos.push(Passo::Falhou(id.clone(), e)),
        }
    }
    None
}

/// Espera o primeiro evento do pioneiro (último filho lançado). **Onde:** [`lancar_escalonado`].
fn aquecer<L: Lancador>(l: &mut L, id: &str, lim: Duration, out: &mut Escalonado<L::Filho>) {
    let Some((_, f)) = out.filhos.iter_mut().rfind(|(i, _)| i == id) else { return };
    match l.primeiro_evento(f, lim) {
        Ok(v) => out.passos.push(Passo::Esperou(id.to_string(), v)),
        Err(e) => {
            out.passos.push(Passo::Esperou(id.to_string(), false));
            out.passos.push(Passo::Falhou(id.to_string(), e));
        }
    }
}

/// Lança a onda: fase 1 = pioneiro de cada grupo (grupos não compartilham cache, então vão
/// juntos); fase 2 = por grupo, espera o evento e lança os seguidores.
///
/// **O quê:** garante que nenhum seguidor inicia antes do primeiro evento (ou do timeout) do
/// pioneiro do seu grupo. **Onde:** scheduler de ondas.
pub fn lancar_escalonado<L: Lancador>(
    l: &mut L,
    grupos: &[Grupo],
    lim: Duration,
) -> Escalonado<L::Filho> {
    let mut out = Escalonado { filhos: Vec::new(), passos: Vec::new() };
    let pios: Vec<Option<usize>> = grupos.iter().map(|g| pioneiro(l, g, &mut out)).collect();
    for (g, p) in grupos.iter().zip(pios) {
        let Some(p) = p else { continue };
        aquecer(l, &g.ids[p], lim, &mut out);
        for id in g.ids.iter().skip(p + 1) {
            match l.lancar(id) {
                Ok(f) => {
                    out.passos.push(Passo::Lancou(id.clone(), false));
                    out.filhos.push((id.clone(), f));
                }
                Err(e) => out.passos.push(Passo::Falhou(id.clone(), e)),
            }
        }
    }
    out
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::otimiza::grupos::{agrupar, Chave, Unidade};

    /// Filho simulado: registra a ordem das chamadas.
    #[derive(Default)]
    struct Falso {
        log: Vec<String>,
        falha: Vec<&'static str>,
        evento: bool,
    }

    impl Lancador for Falso {
        type Filho = String;
        fn lancar(&mut self, id: &str) -> Result<String, String> {
            if self.falha.contains(&id) {
                return Err(format!("{id} nao subiu"));
            }
            self.log.push(format!("lancar {id}"));
            Ok(id.to_string())
        }
        fn primeiro_evento(&mut self, f: &mut String, _l: Duration) -> Result<bool, String> {
            self.log.push(format!("evento {f}"));
            Ok(self.evento)
        }
    }

    fn grupos(n: usize, effort: &str) -> Vec<Grupo> {
        let us: Vec<Unidade> = (0..n)
            .map(|i| Unidade {
                id: format!("{effort}{i}"),
                chave: Chave::nova("m", effort, &[], &[]),
            })
            .collect();
        agrupar(&us)
    }

    #[test]
    fn seguidores_so_depois_do_primeiro_evento() {
        let mut f = Falso { evento: true, ..Falso::default() };
        let r = lancar_escalonado(&mut f, &grupos(3, "a"), Duration::from_secs(1));
        assert_eq!(f.log, ["lancar a0", "evento a0", "lancar a1", "lancar a2"]);
        assert_eq!(r.filhos.len(), 3);
        assert_eq!(r.passos[1], Passo::Esperou("a0".into(), true));
    }

    #[test]
    fn dois_grupos_aquecem_em_paralelo_e_timeout_nao_trava() {
        let mut gs = grupos(2, "a");
        gs.extend(grupos(2, "b"));
        let mut f = Falso::default();
        let r = lancar_escalonado(&mut f, &gs, Duration::from_millis(1));
        assert_eq!(&f.log[..2], ["lancar a0", "lancar b0"], "pioneiros primeiro");
        assert_eq!(r.filhos.len(), 4, "timeout nao impede os seguidores");
        assert!(r.passos.contains(&Passo::Esperou("a0".into(), false)));
    }

    #[test]
    fn pioneiro_que_falha_promove_o_proximo_e_grupo_vazio_nao_quebra() {
        let mut f = Falso { falha: vec!["a0"], evento: true, ..Falso::default() };
        let r = lancar_escalonado(&mut f, &grupos(3, "a"), Duration::from_secs(1));
        assert_eq!(f.log, ["lancar a1", "evento a1", "lancar a2"]);
        assert!(r.passos.iter().any(|p| matches!(p, Passo::Falhou(i, _) if i == "a0")));
        let mut todos = Falso { falha: vec!["a0", "a1"], ..Falso::default() };
        let r = lancar_escalonado(&mut todos, &grupos(2, "a"), Duration::from_secs(1));
        assert!(r.filhos.is_empty());
        assert!(lancar_escalonado(&mut Falso::default(), &[], Duration::ZERO).passos.is_empty());
    }
}
