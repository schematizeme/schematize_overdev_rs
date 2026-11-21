//! Registro das unidades jauladas em `.schematize/overdev/jaulas.jsonl` (uma linha JSON por unidade).
//!
//! **O quê:** [`Registro`], [`anexar`] (append de UMA linha, escrita única em `O_APPEND`),
//! [`ler`] e [`geracao_atual`] (PURA: gerações seguidas de morte por OOM/timeout).
//! **Onde:** o lançador grava; o supervisor lê para decidir a próxima geração.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Nome do arquivo no control-plane.
pub const ARQUIVO: &str = "jaulas.jsonl";

/// Uma unidade jaulada que terminou (ou um aviso).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registro {
    /// Nome da unidade (`schematize-<proj>-<seq>`).
    pub unidade: String,
    /// `leve`/`medio`/`pesado`.
    pub classe: String,
    /// Teto de memória (MiB).
    pub teto_mib: u64,
    /// Pico medido (MiB), se houve como medir.
    pub pico_mib: Option<u64>,
    /// `concluiu`/`falhou`/`oom`/`timeout`/`sem_backend`/`parkeado`/`aviso`.
    pub veredito: String,
    /// Geração de re-decomposição da unidade (0 = original).
    pub geracao: u32,
    /// Unix seconds.
    pub timestamp: u64,
    /// Aviso livre (ex.: execução SEM jaula por escolha do usuário).
    pub aviso: Option<String>,
}

/// **O quê:** caminho do `jaulas.jsonl` em `od`. **Onde:** [`anexar`]/[`ler`].
pub fn caminho(od: &Path) -> PathBuf {
    od.join(ARQUIVO)
}

/// **O quê:** acrescenta UMA linha JSON (um único `write` com `\n`, atômico em `O_APPEND`).
/// **Onde:** [`super::executar_jaulado`] e [`super::apurar`]. **Efeitos:** cria/escreve o arquivo.
pub fn anexar(od: &Path, r: &Registro) -> std::io::Result<()> {
    std::fs::create_dir_all(od)?;
    let mut linha = serde_json::to_string(r)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    linha.push('\n');
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(caminho(od))?;
    f.write_all(linha.as_bytes())
}

/// **O quê:** lê o jsonl; devolve `(registros, linhas_invalidas)` — linha corrompida é CONTADA,
/// nunca derruba a leitura. Arquivo ausente = vazio. **Onde:** supervisor, testes.
pub fn ler(od: &Path) -> (Vec<Registro>, usize) {
    let texto = std::fs::read_to_string(caminho(od)).unwrap_or_default();
    let mut ok = Vec::new();
    let mut ruins = 0;
    for l in texto.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Registro>(l) {
            Ok(r) => ok.push(r),
            Err(_) => ruins += 1,
        }
    }
    (ok, ruins)
}

/// **O quê:** geração corrente: quantas mortes por OOM/timeout seguidas desde o último
/// `concluiu`/`parkeado` (falhas comuns e avisos são neutros). Máx. sem teto aqui; o limite é de
/// [`super::reagir`]. **Onde:** supervisor, ao montar o prompt do relançamento.
pub fn geracao_atual(registros: &[Registro]) -> u32 {
    let mut n = 0;
    for r in registros.iter().rev() {
        match r.veredito.as_str() {
            "oom" | "timeout" => n += 1,
            "concluiu" | "parkeado" => break,
            _ => {}
        }
    }
    n
}
