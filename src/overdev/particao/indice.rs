//! `checklist sync`: sufixo `--completed` nas duas direções + regeneração do índice.

use super::estado::{self, Particao, INDICE, PASTA};
use super::secoes::eh_item;
use crate::overdev::trava::{com_trava, escreve_atomico};
use std::path::Path;

/// Marca de início da região gerada do índice.
pub const MARCA_INI: &str = "<!-- particoes:inicio (gerado por `checklist sync` — não edite) -->";
/// Marca de fim da região gerada do índice.
pub const MARCA_FIM: &str = "<!-- particoes:fim -->";

/// O que o `sync` fez.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Mudancas {
    /// Renomeios `(de, para)` (nomes de arquivo).
    pub renomeados: Vec<(String, String)>,
    /// O índice foi regravado com conteúdo diferente.
    pub indice_atualizado: bool,
    /// `CHECKLIST.md` ainda é monolito com itens: índice não tocado.
    pub monolito: bool,
}

/// **O quê:** uma linha da tabela do índice. **Onde:** [`tabela`].
fn linha(p: &Particao) -> String {
    let arq = p.nome.formatar();
    let dep = if p.depende.is_empty() {
        "-".to_string()
    } else {
        p.depende.iter().map(|d| format!("{d:03}")).collect::<Vec<_>>().join(", ")
    };
    let estado = if p.nome.concluida { "concluída" } else { "aberta" };
    format!(
        "| [{}]({PASTA}/{arq}) | p{} | {} | {} | {dep} | {estado} |",
        arq.trim_end_matches(".md"),
        p.nome.prio,
        p.estado.pendentes(),
        p.estado.total(),
    )
}

/// **O quê:** a região gerada (marcas + tabela partição|prio|abertos|total|depende|estado).
/// **Onde:** [`sync`]. Nenhuma linha começa com `- [`.
pub fn tabela(ps: &[Particao]) -> String {
    let aberto: usize = ps.iter().map(|p| p.estado.pendentes()).sum();
    let total: usize = ps.iter().map(|p| p.estado.total()).sum();
    let mut s = format!(
        "{MARCA_INI}\nPartições em `checklist/`: leia só a próxima não-concluída \
         (`schematize-overdev checklist proxima`). Total: {total} itens, {aberto} abertos.\n\n\
         | partição | prio | abertos | total | depende | estado |\n|---|---|---|---|---|---|\n"
    );
    for p in ps {
        s.push_str(&linha(p));
        s.push('\n');
    }
    s.push_str(MARCA_FIM);
    s.push('\n');
    s
}

/// **O quê:** renomeia `--completed` ↔ aberto conforme o conteúdo e regenera o índice.
/// **Onde:** CLI `checklist sync`, fim do `particionar` e o Stop-hook (antes de contar).
/// Idempotente. Se o `CHECKLIST.md` ainda tem itens (monolito) o índice não é tocado.
pub fn sync(od: &Path) -> Result<Mudancas, String> {
    if !od.join(PASTA).is_dir() {
        return Ok(Mudancas::default());
    }
    com_trava(&od.join(INDICE), || sincronizar(od))
}

/// **O quê:** o corpo do `sync`, sob trava. **Onde:** [`sync`].
pub(super) fn sincronizar(od: &Path) -> Result<Mudancas, String> {
    let mut m = Mudancas::default();
    let mut ps = estado::listar(od)?.validas;
    for p in ps.iter_mut() {
        let deve = p.estado.fechada();
        if deve == p.nome.concluida {
            continue;
        }
        let novo = p.nome.com_conclusao(deve);
        let alvo = p.caminho.with_file_name(novo.formatar());
        if alvo.exists() {
            return Err(format!("{} já existe; resolva a colisão à mão", alvo.display()));
        }
        std::fs::rename(&p.caminho, &alvo)
            .map_err(|e| format!("renomear {}: {e}", p.caminho.display()))?;
        m.renomeados.push((p.nome.formatar(), novo.formatar()));
        p.nome = novo;
        p.caminho = alvo;
    }
    if ps.is_empty() {
        return Ok(m);
    }
    let caminho = od.join(INDICE);
    let atual = match std::fs::read_to_string(&caminho) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("{}: {e}", caminho.display())),
    };
    if atual.lines().any(eh_item) {
        m.monolito = true;
        return Ok(m);
    }
    let antes = match (atual.find(MARCA_INI), atual.find(MARCA_FIM)) {
        (Some(i), _) => atual[..i].to_string(),
        _ if atual.trim().is_empty() => "# CHECKLIST — índice\n\n".to_string(),
        _ => format!("{}\n", atual.trim_end()),
    };
    let novo = format!("{antes}{}", tabela(&ps));
    if novo != atual {
        escreve_atomico(&caminho, &novo)?;
        m.indice_atualizado = true;
    }
    Ok(m)
}
