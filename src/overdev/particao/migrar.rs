//! `checklist particionar`: migra o monolito para partições por coesão (ADR-0027).

use super::estado::{self, Estado, INDICE, PASTA};
use super::indice;
use super::nome::{self, Nome, SLUG_MAX};
use super::secoes::{self, Bloco};
use crate::overdev::trava::{com_trava, escreve_atomico};
use std::path::{Path, PathBuf};

/// Parâmetros de tamanho do `particionar`.
#[derive(Debug, Clone, Copy)]
pub struct Opcoes {
    /// Teto de itens por partição (default 50).
    pub max: usize,
    /// Piso desejado (default 25): seções menores são fundidas com a vizinha de mesma prioridade.
    pub min: usize,
}

impl Default for Opcoes {
    fn default() -> Self {
        Opcoes { max: 50, min: 25 }
    }
}

/// Resultado do `particionar`.
#[derive(Debug, Default)]
pub struct Relatorio {
    /// Partições criadas nesta execução.
    pub criadas: Vec<PathBuf>,
    /// Itens migrados.
    pub itens: usize,
    /// Backup do `CHECKLIST.md` original, se houve migração.
    pub backup: Option<PathBuf>,
}

/// Uma partição em construção (antes de ganhar id).
struct Pendente {
    titulo: String,
    /// Títulos de todas as seções fundidas (cabeçalho legível).
    titulos: Vec<String>,
    prio: u8,
    n: usize,
    corpo: Vec<String>,
    /// `Some(i)`: continua a partição pendente `i` (fatia 2+ da mesma seção).
    continua: Option<usize>,
    secao: usize,
}

/// **O quê:** a fase (`F2`) a partir do título, ou `-`. **Onde:** cabeçalho da partição.
fn fase_de(titulo: &str) -> String {
    let t = titulo.split_whitespace().next().unwrap_or("");
    if !t.is_empty() && t.len() <= 5 && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '.') {
        t.to_string()
    } else {
        "-".into()
    }
}

/// **O quê:** o texto completo de uma partição (cabeçalho do ADR + corpo). **Onde:** [`migrar`] e `editar::anexar_itens`.
pub(super) fn montar(
    nome: &Nome,
    titulo: &str,
    cabecalho: &str,
    depende: &[u16],
    corpo: &[String],
) -> String {
    let fase = fase_de(titulo);
    let plano = if fase == "-" {
        "PLAN.md".to_string()
    } else {
        format!("PLAN.md#{}", fase.to_lowercase())
    };
    let dep = if depende.is_empty() {
        "-".to_string()
    } else {
        depende.iter().map(|d| format!("{d:03}")).collect::<Vec<_>>().join(", ")
    };
    format!(
        "# p{}-{} · {cabecalho}\n> fase: {fase} · depende: {dep} · plano: {plano}\n\
         `- [ ]`/`- [x]` máquina · `- [H ]`/`- [H x]` humano · `- [~]` on-hold\n\n{}\n",
        nome.prio,
        nome.id3(),
        corpo.join("\n")
    )
}

/// **O quê:** transforma as seções em partições pendentes: fatia (>max), funde (<min).
/// **Onde:** [`particionar`].
fn planejar(secoes: &[secoes::Secao], op: &Opcoes) -> Vec<Pendente> {
    let mut v: Vec<Pendente> = Vec::new();
    for (si, s) in secoes.iter().enumerate() {
        let prio = s.prio.unwrap_or(2);
        let fatias = secoes::fatiar(&s.blocos, op.max);
        let nf = fatias.len();
        for (fi, f) in fatias.into_iter().enumerate() {
            let n: usize = f.iter().map(|b: &Bloco| b.n).sum();
            let mut corpo = Vec::new();
            if !s.cabecalho.is_empty() {
                corpo.push(if fi == 0 {
                    s.cabecalho.clone()
                } else {
                    format!("## {} (cont. {}/{nf})", s.titulo, fi + 1)
                });
            }
            for b in &f {
                corpo.extend(b.linhas.iter().cloned());
            }
            if fi + 1 == nf {
                corpo.extend(s.sobra.iter().cloned());
            }
            let continua = if fi > 0 { Some(v.len() - 1) } else { None };
            // Fusão de seção pequena com a anterior (mesma prio, outra seção).
            if fi == 0 {
                if let Some(p) = v.last_mut() {
                    if p.secao != si && p.prio == prio && p.n < op.min && p.n + n <= op.max {
                        p.n += n;
                        p.titulos.push(s.titulo.clone());
                        p.corpo.extend(corpo);
                        p.secao = si;
                        continue;
                    }
                }
            }
            v.push(Pendente {
                titulo: s.titulo.clone(),
                titulos: vec![s.titulo.clone()],
                prio,
                n,
                corpo,
                continua,
                secao: si,
            });
        }
    }
    v
}

/// **O quê:** cópia de segurança sem sobrescrever backup anterior (`.bak`, `.bak.1`, …).
/// **Onde:** [`particionar`].
fn backup(od: &Path) -> Result<PathBuf, String> {
    let origem = od.join(INDICE);
    let mut alvo = od.join("CHECKLIST.md.bak");
    let mut n = 0;
    while alvo.exists() {
        n += 1;
        alvo = od.join(format!("CHECKLIST.md.bak.{n}"));
    }
    std::fs::copy(&origem, &alvo).map_err(|e| format!("backup {}: {e}", alvo.display()))?;
    Ok(alvo)
}

/// **O quê:** migra o `CHECKLIST.md` monolítico (e arquivos legados do `split`) para partições
/// `checklist/pN-NNN-slug.md`, deixando o `CHECKLIST.md` como índice sem nenhum item.
/// **Onde:** `schematize-overdev checklist particionar`.
///
/// Idempotente: sem itens no monolito só regenera o índice. NUNCA renumera ids existentes
/// (novas partições usam o próximo NNN livre). Falha no meio deixa o backup e erra alto.
pub fn particionar(od: &Path, op: &Opcoes) -> Result<Relatorio, String> {
    if op.max < 1 || op.min > op.max {
        return Err(format!("--min ({}) deve ser <= --max ({}) e --max >= 1", op.min, op.max));
    }
    std::fs::create_dir_all(od.join(PASTA)).map_err(|e| e.to_string())?;
    com_trava(&od.join(INDICE), || migrar(od, op))
}

/// **O quê:** o corpo do `particionar`, já sob trava. **Onde:** [`particionar`].
fn migrar(od: &Path, op: &Opcoes) -> Result<Relatorio, String> {
    let mut rel = Relatorio::default();
    let caminho = od.join(INDICE);
    let texto = match std::fs::read_to_string(&caminho) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("{}: {e}", caminho.display())),
    };
    let listagem = estado::listar(od)?;
    let legados: Vec<String> = listagem.invalidos.clone();
    let mut analise = secoes::analisar(&texto, "geral");
    for nm in &legados {
        let p = od.join(PASTA).join(nm);
        let t = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        let stem = nm.trim_end_matches(".md");
        let blocos = secoes::blocos_de(&secoes::linhas_de(&t)).0;
        if !blocos.is_empty() {
            analise.secoes.push(secoes::Secao {
                titulo: nome::slugar(stem, SLUG_MAX),
                blocos,
                ..Default::default()
            });
        }
    }
    let tem_itens = analise.secoes.iter().any(|s| s.total() > 0);
    if !tem_itens {
        indice::sincronizar(od)?;
        return Ok(rel);
    }
    if texto.lines().any(secoes::eh_item) {
        rel.backup = Some(backup(od)?);
    }
    let pend = planejar(&analise.secoes, op);
    let primeiro = estado::proximo_id(&listagem)?;
    if usize::from(primeiro) + pend.len() > 1000 {
        return Err("ids de partição esgotados (NNN > 999)".into());
    }
    let mut ids: Vec<u16> = Vec::new();
    for (i, p) in pend.iter().enumerate() {
        let id = primeiro + i as u16;
        let depende: Vec<u16> = p.continua.map(|c| vec![ids[c]]).unwrap_or_default();
        let mut slug = nome::slugar(&p.titulo, SLUG_MAX - 3);
        if p.continua.is_some() || pend.iter().filter(|q| q.secao == p.secao).count() > 1 {
            slug = format!("{slug}-{}", i + 1);
            slug.truncate(SLUG_MAX);
        }
        let mut nm = Nome { prio: p.prio, id, slug, concluida: false };
        let corpo = montar(&nm, &p.titulo, &p.titulos.join(" + "), &depende, &p.corpo);
        nm.concluida = Estado::de(&corpo).fechada();
        let alvo = od.join(PASTA).join(nm.formatar());
        escreve_atomico(&alvo, &corpo)?;
        rel.itens += p.n;
        rel.criadas.push(alvo);
        ids.push(id);
    }
    let mut prosa = analise.prosa.join("\n");
    if !prosa.ends_with('\n') {
        prosa.push('\n');
    }
    escreve_atomico(&caminho, &prosa)?;
    mover_legados(od, &legados)?;
    indice::sincronizar(od)?;
    Ok(rel)
}

/// **O quê:** move os arquivos legados do `split` para `checklist/legado/` (fora da leitura).
/// **Onde:** [`migrar`], depois de gravar as partições.
fn mover_legados(od: &Path, legados: &[String]) -> Result<(), String> {
    if legados.is_empty() {
        return Ok(());
    }
    let dest = od.join(PASTA).join("legado");
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    for n in legados {
        std::fs::rename(od.join(PASTA).join(n), dest.join(n))
            .map_err(|e| format!("mover legado {n}: {e}"))?;
    }
    Ok(())
}
