//! Coleta DETERMINÍSTICA do estado do projeto (sem chamar o Claude, só leitura).
//!
//! **O quê:** [`coletar`] junta o checklist do overdev (feito vs aberto vs `[~]`), as unidades em
//! voo (`estado/` do overdev), `git status`/HEAD/branch de cada repo do workspace, o último
//! relatório do archive, os arquivos tocados na janela e os vereditos recentes de `jaulas.jsonl`.
//! **Onde:** [`super::ciclo::Motor::tick_mecanico`]; [`super::render`] transforma em Markdown.
//! Falha de uma fonte vira AVISO no resultado (nunca engolida, nunca derruba o tick).

use super::config::Config;
use super::proc::{rodar, Pedido};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

const MAX_ITENS: usize = 40;
const MAX_SUJOS: usize = 15;

/// Estado do checklist do overdev.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Checklist {
    /// Existe algum `CHECKLIST` no overdev.
    pub existe: bool,
    /// Itens `- [x]`.
    pub feitos: usize,
    /// Últimos itens feitos (para contexto).
    pub ultimos_feitos: Vec<String>,
    /// Total de abertos (`- [ ]` e `- [H ]`).
    pub n_abertos: usize,
    /// Primeiros abertos.
    pub abertos: Vec<String>,
    /// Total de parkeados (`- [~]`).
    pub n_parkeados: usize,
    /// Primeiros parkeados.
    pub parkeados: Vec<String>,
}

/// Um repo git do workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    /// Nome da pasta (`.` = o próprio workspace).
    pub nome: String,
    /// Linha `## branch...origin/branch [ahead N]`.
    pub branch: String,
    /// `git rev-parse --short HEAD` (vazio se repo sem commit).
    pub head: String,
    /// Total de entradas sujas.
    pub n_sujos: usize,
    /// Primeiras entradas sujas.
    pub sujos: Vec<String>,
}

/// Tudo que o handoff mecânico mostra.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Coleta {
    /// Nome do projeto.
    pub projeto: String,
    /// Checklist.
    pub checklist: Checklist,
    /// Unidades em voo (`nome (bytes) primeira-linha`).
    pub em_voo: Vec<String>,
    /// Repos do workspace.
    pub repos: Vec<Repo>,
    /// Último relatório do archive (`arquivo — título`).
    pub relatorio: Option<String>,
    /// Arquivos sujos tocados na janela (`repo/arquivo`).
    pub tocados: Vec<String>,
    /// Vereditos recentes do `jaulas.jsonl`.
    pub vereditos: Vec<String>,
    /// Fontes que falharam.
    pub avisos: Vec<String>,
}

/// **O quê:** corta `s` em `n` chars. **Onde:** linhas de itens. PURA.
fn cortar(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// **O quê:** classifica as linhas de um checklist. **Onde:** [`coletar`]. PURA.
pub fn parse_checklist(texto: &str) -> Checklist {
    let mut c = Checklist { existe: !texto.is_empty(), ..Checklist::default() };
    for l in texto.lines() {
        let t = l.trim_start();
        let item = |pre: usize| cortar(t[pre..].trim(), 200);
        if t.starts_with("- [x]") || t.starts_with("- [X]") {
            c.feitos += 1;
            c.ultimos_feitos.push(item(5));
            if c.ultimos_feitos.len() > 10 {
                c.ultimos_feitos.remove(0);
            }
        } else if t.starts_with("- [~]") {
            c.n_parkeados += 1;
            if c.parkeados.len() < MAX_ITENS {
                c.parkeados.push(item(5));
            }
        } else if t.starts_with("- [ ]") || t.starts_with("- [H ]") {
            c.n_abertos += 1;
            if c.abertos.len() < MAX_ITENS {
                c.abertos.push(item(t.find(']').map_or(5, |i| i + 1)));
            }
        }
    }
    c
}

/// **O quê:** `git` em `dir` com timeout; `Err(motivo)` se falhar. **Onde:** [`repo_de`].
fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut argv: Vec<String> = vec!["git".into(), "-C".into(), dir.to_string_lossy().into_owned()];
    argv.extend(args.iter().map(|s| (*s).to_string()));
    let env = [
        ("GIT_OPTIONAL_LOCKS".to_string(), "0".to_string()),
        ("LC_ALL".to_string(), "C".to_string()),
    ];
    let p = Pedido {
        argv: &argv,
        env: &env,
        cwd: dir,
        stdin: None,
        timeout: Duration::from_secs(15),
        teto_stdout: 512 * 1024,
        limite_as_mib: None,
    };
    let s = rodar(&p)?;
    if s.timeout || s.codigo != Some(0) {
        return Err(format!(
            "git {} saiu {:?}{}: {}",
            args.join(" "),
            s.codigo,
            if s.timeout { " (timeout)" } else { "" },
            s.stderr_cauda
        ));
    }
    Ok(String::from_utf8_lossy(&s.stdout).into_owned())
}

/// **O quê:** estado git de um repo (`status -b` + HEAD). **Onde:** [`coletar`].
fn repo_de(
    dir: &Path,
    nome: &str,
    agora: u64,
    janela: u64,
    tocados: &mut Vec<String>,
) -> Result<Repo, String> {
    let st = git(dir, &["status", "--porcelain=v1", "-b"])?;
    let head = git(dir, &["rev-parse", "--short", "HEAD"]).unwrap_or_default().trim().to_string();
    let mut it = st.lines();
    let branch = it.next().unwrap_or("").trim_start_matches("## ").to_string();
    let sujos: Vec<&str> = it.collect();
    for l in sujos.iter().take(5000) {
        let caminho =
            l.get(3..).unwrap_or("").rsplit(" -> ").next().unwrap_or("").trim_matches('"');
        let m = std::fs::metadata(dir.join(caminho)).and_then(|m| m.modified()).ok();
        let secs = m.and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs());
        if secs.is_some_and(|s| agora.saturating_sub(s) <= janela) {
            tocados.push(format!("{}/{caminho}", nome));
        }
    }
    Ok(Repo {
        nome: nome.to_string(),
        branch,
        head,
        n_sujos: sujos.len(),
        sujos: sujos.iter().take(MAX_SUJOS).map(|s| cortar(s, 160)).collect(),
    })
}

/// **O quê:** pastas do workspace com `.git` (+ o próprio workspace), ordenadas. **Onde:** [`coletar`].
fn repos_do_workspace(raiz: &Path) -> Vec<(String, PathBuf)> {
    let mut v: Vec<(String, PathBuf)> = Vec::new();
    if raiz.join(".git").exists() {
        v.push((".".to_string(), raiz.to_path_buf()));
    }
    if let Ok(rd) = std::fs::read_dir(raiz) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && p.join(".git").exists() {
                v.push((e.file_name().to_string_lossy().into_owned(), p));
            }
        }
    }
    v.sort();
    v
}

/// **O quê:** lista `estado/` do overdev (só leitura): `nome (N B) primeira linha`.
/// **Onde:** [`coletar`].
fn em_voo(dir: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir.join("estado")) else { return Vec::new() };
    let mut v: Vec<String> = rd
        .flatten()
        .filter_map(|e| {
            let md = e.metadata().ok()?;
            let nome = e.file_name().to_string_lossy().into_owned();
            let l1 = if md.is_file() && md.len() <= 4096 {
                std::fs::read_to_string(e.path())
                    .ok()
                    .and_then(|s| s.lines().next().map(|l| cortar(l, 100)))
            } else {
                None
            };
            Some(format!(
                "{nome} ({} B){}",
                md.len(),
                l1.map(|l| format!(" {l}")).unwrap_or_default()
            ))
        })
        .collect();
    v.sort();
    v.truncate(30);
    v
}

/// **O quê:** o relatório mais novo (por nome, que começa com a data). **Onde:** [`coletar`].
fn ultimo_relatorio(archive: &Path) -> Option<String> {
    let mut nomes: Vec<PathBuf> = std::fs::read_dir(archive.join("relatorios"))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("md"))
        .collect();
    nomes.sort();
    let p = nomes.pop()?;
    let titulo = std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| !l.trim().is_empty())
                .map(|l| cortar(l.trim_start_matches('#').trim(), 120))
        })
        .unwrap_or_default();
    Some(format!("{} — {titulo}", p.file_name()?.to_string_lossy()))
}

/// **O quê:** monta a [`Coleta`] completa. **Onde:** cada tick mecânico. **Efeitos:** só leitura
/// de arquivos + `git status`/`rev-parse` dos repos.
pub fn coletar(cfg: &Config, agora: u64) -> Coleta {
    let od = cfg.dir_overdev();
    let mut c = Coleta { projeto: cfg.projeto.clone(), ..Coleta::default() };
    c.checklist = parse_checklist(&crate::paths::read_multidoc(&od, "CHECKLIST.md", "checklist"));
    c.em_voo = em_voo(&od);
    c.relatorio = ultimo_relatorio(&cfg.archive);
    let (regs, ruins) = crate::agentrun::jaulado::ler(&od);
    c.vereditos = regs
        .iter()
        .rev()
        .take(8)
        .map(|r| format!("{} {} ger={} teto={}MiB", r.unidade, r.veredito, r.geracao, r.teto_mib))
        .collect();
    if ruins > 0 {
        c.avisos.push(format!("jaulas.jsonl: {ruins} linha(s) inválida(s)"));
    }
    for (nome, dir) in repos_do_workspace(&cfg.workspace) {
        match repo_de(&dir, &nome, agora, cfg.janela_tocados_s, &mut c.tocados) {
            Ok(r) => c.repos.push(r),
            Err(e) => c.avisos.push(format!("repo {nome}: {e}")),
        }
    }
    c.tocados.sort();
    c.tocados.truncate(60);
    c
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn classifica_checklist() {
        let c = parse_checklist(
            "# t\n- [x] a\n  - [x] b\n- [ ] c\n- [H ] humano\n- [~] parkeado\ntexto\n",
        );
        assert_eq!((c.feitos, c.n_abertos, c.n_parkeados), (2, 2, 1));
        assert_eq!(c.abertos, vec!["c".to_string(), "humano".to_string()]);
        assert!(c.existe && !parse_checklist("").existe);
    }
}
