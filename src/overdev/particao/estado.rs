//! Estado de uma partição (abertos/feitos/humanos/on-hold) e a listagem do diretório.

use super::nome::{self, Nome};
use crate::overdev::progresso::count_str;
use std::path::{Path, PathBuf};

/// Pasta das partições, relativa ao dir do overdev.
pub const PASTA: &str = "checklist";
/// Arquivo do índice, relativo ao dir do overdev.
pub const INDICE: &str = "CHECKLIST.md";

/// Contagem de itens de uma partição, por categoria.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Estado {
    /// Máquina abertos `- [ ]`.
    pub abertos: usize,
    /// Feitos `- [x]` / `- [H x]` / respondidos.
    pub feitos: usize,
    /// Humanos abertos `- [H ]`.
    pub humanos: usize,
    /// On-hold `- [~]`.
    pub hold: usize,
    /// Recusados/cancelados (resolvidos, não feitos).
    pub recusados: usize,
}

impl Estado {
    /// **O quê:** conta os itens de `texto`. **Onde:** [`Particao::ler`] e o `particionar`.
    pub fn de(texto: &str) -> Estado {
        let c = count_str(texto);
        Estado {
            abertos: c.open,
            feitos: c.done(),
            humanos: c.human,
            hold: c.hold,
            recusados: c.recusado,
        }
    }

    /// **O quê:** total de itens (toda categoria). **Onde:** limite 25–50 e tabela do índice.
    pub fn total(&self) -> usize {
        self.abertos + self.feitos + self.humanos + self.hold + self.recusados
    }

    /// **O quê:** itens que impedem `--completed` (`- [ ]`, `- [H ]`, `- [~]`).
    /// **Onde:** `sync`, `check` e o Stop-hook.
    pub fn pendentes(&self) -> usize {
        self.abertos + self.humanos + self.hold
    }

    /// **O quê:** a partição está toda fechada (e tem ao menos 1 item). **Onde:** `sync`.
    pub fn fechada(&self) -> bool {
        self.total() > 0 && self.pendentes() == 0
    }
}

/// Uma partição lida do disco.
#[derive(Debug, Clone)]
pub struct Particao {
    /// Nome decomposto.
    pub nome: Nome,
    /// Caminho do arquivo.
    pub caminho: PathBuf,
    /// Contagem dos itens.
    pub estado: Estado,
    /// Ids do `depende:` do cabeçalho.
    pub depende: Vec<u16>,
}

impl Particao {
    /// **O quê:** lê e analisa o arquivo `caminho` de nome já válido. **Onde:** [`listar`].
    fn ler(nome: Nome, caminho: PathBuf) -> Result<Particao, String> {
        let txt =
            std::fs::read_to_string(&caminho).map_err(|e| format!("{}: {e}", caminho.display()))?;
        Ok(Particao { nome, estado: Estado::de(&txt), depende: parse_depende(&txt), caminho })
    }
}

/// **O quê:** extrai os ids de `depende: 000, 001` do cabeçalho `> fase: … · depende: … · …`.
/// **Onde:** [`Particao::ler`]. `-`, vazio ou lixo viram lista vazia; nunca panica.
pub fn parse_depende(texto: &str) -> Vec<u16> {
    let Some(linha) = texto.lines().take(8).find(|l| l.starts_with('>') && l.contains("depende:"))
    else {
        return Vec::new();
    };
    let Some((_, resto)) = linha.split_once("depende:") else { return Vec::new() };
    let campo = resto.split('·').next().unwrap_or("");
    campo
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter_map(|t| if t.len() == 3 { t.parse::<u16>().ok() } else { None })
        .collect()
}

/// Resultado de [`listar`]: partições válidas (ordenadas por `(pN, NNN)`) e nomes rejeitados.
#[derive(Debug, Default)]
pub struct Listagem {
    /// Partições com nome válido, em ordem de trabalho.
    pub validas: Vec<Particao>,
    /// Arquivos `.md` da pasta cujo nome NÃO casa com o contrato.
    pub invalidos: Vec<String>,
}

/// **O quê:** lista a pasta `checklist/` de `od`. Dotfiles e não-`.md` são ignorados.
/// **Onde:** `sync`, `check`, `proxima`, `particionar` e o Stop-hook.
pub fn listar(od: &Path) -> Result<Listagem, String> {
    let dir = od.join(PASTA);
    let mut out = Listagem::default();
    if !dir.is_dir() {
        return Ok(out);
    }
    for e in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let e = e.map_err(|e| e.to_string())?;
        let p = e.path();
        let Some(n) = p.file_name().and_then(|n| n.to_str()).map(String::from) else { continue };
        if !p.is_file() || n.starts_with('.') || !n.ends_with(".md") {
            continue;
        }
        match nome::parse(&n) {
            Some(nm) => out.validas.push(Particao::ler(nm, p)?),
            None => out.invalidos.push(n),
        }
    }
    out.validas.sort_by(|a, b| a.nome.cmp(&b.nome).then(a.caminho.cmp(&b.caminho)));
    out.invalidos.sort();
    Ok(out)
}

/// **O quê:** o checklist está no layout particionado (há ≥1 partição válida)?
/// **Onde:** `split` (round-robin não se aplica) e `particionar` (idempotência).
pub fn tem_particoes(od: &Path) -> bool {
    listar(od).map(|l| !l.validas.is_empty()).unwrap_or(false)
}

/// **O quê:** o próximo id `NNN` livre (máx. existente + 1; nunca reusa). `Err` se passar de 999.
/// **Onde:** `particionar`.
pub fn proximo_id(l: &Listagem) -> Result<u16, String> {
    let max = l.validas.iter().map(|p| p.nome.id).max();
    let n = max.map_or(1, |m| m + 1);
    if n > 999 {
        return Err("ids de partição esgotados (NNN > 999)".into());
    }
    Ok(n)
}
