//! Isolamento de escrita por unidade e merge único (IA-19).
//!
//! **O quê:** detecta, ANTES de lançar, duas unidades que escrevem o mesmo arquivo ou um
//! diretório e algo dentro dele (plano rejeitado); deriva a partição/worktree de cada unidade
//! e o plano de gather (aplica em ordem; unidade falha é descartada sem sujar as demais).
//! **Onde:** spawner (cria worktree com o argv de [`comando_worktree_add`]) e gather do
//! orquestrador. Nada aqui executa `git`: só decide e monta argv.

use std::path::PathBuf;

/// Unidade com seus alvos de escrita (caminhos relativos ao repo; terminar em `/` = diretório).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Escritor {
    /// Id da unidade.
    pub id: String,
    /// Ordem de aplicação no gather (menor primeiro; empate por id).
    pub ordem: u32,
    /// Alvos de escrita.
    pub alvos: Vec<String>,
}

/// Dois escritores com alvos sobrepostos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflito {
    /// Primeira unidade.
    pub a: String,
    /// Segunda unidade.
    pub b: String,
    /// Alvo (normalizado) que colide.
    pub alvo: String,
}

/// Normaliza um alvo: sem `./`, sem `//`, sem `..`, sem absoluto. `Err` = alvo inseguro.
///
/// **O quê:** a barra final (diretório) é preservada. **Onde:** [`validar_plano`].
pub fn normalizar(alvo: &str) -> Result<String, String> {
    let t = alvo.trim();
    if t.is_empty() || t.starts_with('/') || t.contains('\0') || t.contains('\\') {
        return Err(format!("alvo invalido: `{alvo}`"));
    }
    let mut partes: Vec<&str> = Vec::new();
    for p in t.split('/') {
        match p {
            "" | "." => {}
            ".." => return Err(format!("alvo com `..`: `{alvo}`")),
            x => partes.push(x),
        }
    }
    if partes.is_empty() {
        return Err(format!("alvo invalido: `{alvo}`"));
    }
    let dir = if t.ends_with('/') { "/" } else { "" };
    Ok(format!("{}{dir}", partes.join("/")))
}

/// `a` e `b` colidem? Iguais, ou um é diretório (`x/`) que contém o outro.
/// **Onde:** [`detectar_conflitos`].
fn colide(a: &str, b: &str) -> bool {
    let pre = |d: &str, x: &str| d.ends_with('/') && x.starts_with(d);
    a == b || pre(a, b) || pre(b, a)
}

/// Todos os conflitos entre pares de unidades (cada par uma vez por alvo colidente).
///
/// **O quê:** `Err` se algum alvo é inseguro. **Onde:** [`validar_plano`].
pub fn detectar_conflitos(es: &[Escritor]) -> Result<Vec<Conflito>, String> {
    let mut norm: Vec<(&str, Vec<String>)> = Vec::new();
    for e in es {
        let alvos = e.alvos.iter().map(|a| normalizar(a)).collect::<Result<Vec<_>, _>>()?;
        norm.push((&e.id, alvos));
    }
    let mut out = Vec::new();
    for (i, (ia, aa)) in norm.iter().enumerate() {
        for (ib, ab) in norm.iter().skip(i + 1) {
            for x in aa {
                if let Some(y) = ab.iter().find(|y| colide(x, y)) {
                    let alvo = if x.len() >= y.len() { x } else { y };
                    out.push(Conflito { a: (*ia).into(), b: (*ib).into(), alvo: alvo.clone() });
                }
            }
        }
    }
    Ok(out)
}

/// Valida o plano: sem conflito e sem alvo inseguro. **O quê:** `Err` com a lista legível.
/// **Onde:** antes de lançar a onda (plano rejeitado = nada lançado).
pub fn validar_plano(es: &[Escritor]) -> Result<(), Vec<String>> {
    match detectar_conflitos(es) {
        Err(e) => Err(vec![e]),
        Ok(c) if c.is_empty() => Ok(()),
        Ok(c) => {
            Err(c.iter().map(|x| format!("{} e {} escrevem `{}`", x.a, x.b, x.alvo)).collect())
        }
    }
}

/// Diretório da partição da unidade: `<base>/<id>`; `id` fora de `[A-Za-z0-9_-]{1,64}` é recusado.
/// **Onde:** spawner (worktree/partição por unidade).
pub fn particao(base: &std::path::Path, id: &str) -> Result<PathBuf, String> {
    let ok = !id.is_empty()
        && id.len() <= 64
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if ok {
        Ok(base.join(id))
    } else {
        Err(format!("id de unidade invalido para particao: `{id}`"))
    }
}

/// Argv de `git worktree add -b <ramo> <dir> <base>` (sem executar). **Onde:** spawner.
pub fn comando_worktree_add(
    dir: &std::path::Path,
    ramo: &str,
    base: &str,
) -> Result<Vec<String>, String> {
    if [ramo, base].iter().any(|s| s.is_empty() || s.starts_with('-') || s.contains(' ')) {
        return Err(format!("ramo/base invalido: `{ramo}` / `{base}`"));
    }
    Ok(["git", "worktree", "add", "-b", ramo, &dir.display().to_string(), base]
        .iter()
        .map(|s| s.to_string())
        .collect())
}

/// Ação do gather para uma unidade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Acao {
    /// Aplicar a partição da unidade ao tronco.
    Aplicar(String),
    /// Descartar (a unidade falhou; nada dela entra).
    Descartar(String),
}

/// Plano de gather: ordem por (`ordem`, `id`); falhas viram `Descartar` sem afetar as outras.
///
/// **O quê:** `resultados` = (id, ok). Unidade sem resultado é descartada (não presume sucesso).
/// **Onde:** gather do orquestrador, antes da verificação única.
pub fn plano_gather(es: &[Escritor], resultados: &[(String, bool)]) -> Vec<Acao> {
    let mut ord: Vec<&Escritor> = es.iter().collect();
    ord.sort_by(|x, y| (x.ordem, &x.id).cmp(&(y.ordem, &y.id)));
    ord.iter()
        .map(|e| {
            let ok = resultados.iter().any(|(i, ok)| *i == e.id && *ok);
            if ok {
                Acao::Aplicar(e.id.clone())
            } else {
                Acao::Descartar(e.id.clone())
            }
        })
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::path::Path;

    fn e(id: &str, ordem: u32, alvos: &[&str]) -> Escritor {
        Escritor { id: id.into(), ordem, alvos: alvos.iter().map(|s| s.to_string()).collect() }
    }

    #[test]
    fn mesmo_arquivo_ou_diretorio_contendo_e_rejeitado() {
        let p = [
            e("a", 0, &["src/x.rs"]),
            e("b", 1, &["./src//x.rs"]),
            e("c", 2, &["docs/"]),
            e("d", 3, &["docs/a.md"]),
        ];
        let c = detectar_conflitos(&p).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!((c[0].a.as_str(), c[0].b.as_str()), ("a", "b"));
        assert_eq!(c[1].alvo, "docs/a.md");
        let msgs = validar_plano(&p).unwrap_err();
        assert!(msgs[0].contains("a e b"));
        assert!(
            validar_plano(&[e("a", 0, &["src/a.rs"]), e("b", 0, &["src/b.rs", "docs/"])]).is_ok()
        );
        assert!(
            validar_plano(&[e("a", 0, &["src/x"]), e("b", 0, &["src/xy"])]).is_ok(),
            "prefixo de texto nao e diretorio"
        );
    }

    #[test]
    fn alvo_inseguro_e_hostil() {
        for ruim in ["", "/etc/passwd", "../x", "a/../b", "a\0b", "a\\b", "./", " "] {
            assert!(normalizar(ruim).is_err(), "{ruim:?}");
        }
        assert_eq!(normalizar("a/./b/").unwrap(), "a/b/");
        assert!(validar_plano(&[e("a", 0, &["../fora"])]).is_err());
        assert!(particao(Path::new("/w"), "../x").is_err());
        assert_eq!(particao(Path::new("/w"), "u-1").unwrap(), Path::new("/w/u-1"));
    }

    #[test]
    fn gather_em_ordem_e_falha_nao_suja_as_outras() {
        let es = [e("c", 2, &["c"]), e("a", 0, &["a"]), e("b", 0, &["b"])];
        let r = [("a".to_string(), true), ("b".to_string(), false)];
        assert_eq!(
            plano_gather(&es, &r),
            [Acao::Aplicar("a".into()), Acao::Descartar("b".into()), Acao::Descartar("c".into())]
        );
        assert!(plano_gather(&[], &r).is_empty());
    }

    #[test]
    fn worktree_argv_e_flag_injetada() {
        let v = comando_worktree_add(Path::new("/w/u1"), "ovd/u1", "main").unwrap();
        assert_eq!(v[..4], ["git", "worktree", "add", "-b"]);
        assert!(comando_worktree_add(Path::new("/w"), "--force", "main").is_err());
        assert!(comando_worktree_add(Path::new("/w"), "r", "").is_err());
    }
}
