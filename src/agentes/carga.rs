//! Leitura, merge e escrita do `agentes.toml` (global e por projeto).
//!
//! **O quê:** parse tolerante a campos ausentes mas ESTRITO nos valores (erro tipado), merge
//! `padrões < global < projeto` por papel, e gravação atômica validada.
//! **Onde:** `ClaudeRunner`, prompt, `delegar` e a janela.

use super::erro::ErroAgentes;
use super::schema::{nome_valido, Agentes, Motor, Papel};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// O arquivo como o humano escreve: tudo opcional, validado depois.
#[derive(Debug, Deserialize, Default)]
struct Bruto {
    versao: Option<i64>,
    escada: Option<Vec<String>>,
    roteamento: Option<BTreeMap<String, String>>,
    papeis: Option<BTreeMap<String, PapelBruto>>,
}

/// Um papel como escrito no arquivo (campos obrigatórios checados em [`converte_papel`]).
#[derive(Debug, Deserialize, Default)]
struct PapelBruto {
    motor: Option<String>,
    modelo: Option<String>,
    ativo: Option<bool>,
    descricao: Option<String>,
}

/// **O quê:** onde mora o `agentes.toml` global (ao lado do `config.json`).
/// **Onde:** `carregar` e `salvar_global`.
pub fn caminho_global() -> PathBuf {
    crate::nucleo::caminhos::dados_dir().join("agentes.toml")
}

/// **O quê:** onde mora o override do projeto (`<proj>/.schematize/overdev/agentes.toml`).
/// **Onde:** `carregar` e `salvar_projeto`.
pub fn caminho_projeto(projeto: &Path) -> PathBuf {
    crate::paths::overdev_dir_at(projeto).join("agentes.toml")
}

/// **O quê:** converte um papel bruto em [`Papel`], com erro tipado por campo.
/// **Onde:** [`aplica`].
fn converte_papel(nome: &str, b: PapelBruto) -> Result<Papel, ErroAgentes> {
    let falta = |campo: &str| ErroAgentes::CampoAusente {
        papel: nome.to_string(),
        campo: campo.to_string(),
    };
    let motor_txt = b.motor.ok_or_else(|| falta("motor"))?;
    let motor = Motor::tentar(&motor_txt).ok_or_else(|| ErroAgentes::MotorDesconhecido {
        papel: nome.to_string(),
        motor: motor_txt.clone(),
    })?;
    let modelo = b.modelo.ok_or_else(|| falta("modelo"))?;
    if modelo.trim().is_empty() {
        return Err(ErroAgentes::ModeloVazio(nome.to_string()));
    }
    Ok(Papel {
        motor,
        modelo,
        ativo: b.ativo.unwrap_or(true),
        descricao: b.descricao.unwrap_or_default(),
    })
}

/// **O quê:** parseia o texto de UM arquivo e SOBREPÕE o resultado em `base` (por papel;
/// `escada`/`roteamento` inteiros se presentes). **Onde:** `carregar` e `salvar_*`.
fn aplica(base: &mut Agentes, texto: &str, origem: &str) -> Result<(), ErroAgentes> {
    let b: Bruto = toml::from_str(texto)
        .map_err(|e| ErroAgentes::Toml { origem: origem.to_string(), msg: e.to_string() })?;
    if let Some(v) = b.versao {
        if v != 1 {
            return Err(ErroAgentes::Versao(v));
        }
    }
    for (nome, pb) in b.papeis.unwrap_or_default() {
        if !nome_valido(&nome) {
            return Err(ErroAgentes::NomeInvalido(nome));
        }
        let p = converte_papel(&nome, pb)?;
        base.papeis.insert(nome, p);
    }
    if let Some(e) = b.escada {
        base.escada = e;
    }
    if let Some(r) = b.roteamento {
        base.roteamento = r;
    }
    Ok(())
}

/// **O quê:** lê um arquivo opcional: `Ok(None)` se não existe, `Err` se existe e não lê.
/// **Onde:** [`carregar`].
fn le_opcional(caminho: &Path) -> Result<Option<String>, ErroAgentes> {
    match std::fs::read_to_string(caminho) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ErroAgentes::Io { caminho: caminho.to_path_buf(), msg: e.to_string() }),
    }
}

/// **O quê:** a config EFETIVA: padrões, sobrepostos pelo global, sobrepostos pelo projeto.
/// Arquivo ausente nunca é erro; arquivo inválido é. **Onde:** runner, prompt, `delegar`, janela.
pub fn carregar(projeto: Option<&Path>) -> Result<Agentes, ErroAgentes> {
    carregar_de(&caminho_global(), projeto.map(caminho_projeto).as_deref())
}

/// **O quê:** o miolo de [`carregar`] com os caminhos explícitos. **Onde:** `carregar`.
fn carregar_de(global: &Path, projeto: Option<&Path>) -> Result<Agentes, ErroAgentes> {
    let mut ag = Agentes::padrao();
    for caminho in [Some(global), projeto].into_iter().flatten() {
        if let Some(t) = le_opcional(caminho)? {
            aplica(&mut ag, &t, &caminho.display().to_string())?;
        }
    }
    ag.validar()?;
    Ok(ag)
}

/// **O quê:** como [`carregar`], mas NUNCA falha: em erro, LOGA no stderr e devolve os padrões.
/// **Onde:** `ClaudeRunner::spawn`, terminal externo e prompt — o overdev não pode deixar de
/// subir por causa de um TOML ruim, mas também não pode engolir o erro em silêncio.
pub fn carregar_ou_padrao(projeto: Option<&Path>) -> Agentes {
    match carregar(projeto) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("[schematize] agentes.toml inválido, usando os padrões embutidos: {e}");
            Agentes::padrao()
        }
    }
}

/// **O quê:** valida `ag` e grava em `alvo` de forma atômica (tmp + rename).
/// **Onde:** `salvar_global` / `salvar_projeto`.
fn salvar_em(alvo: &Path, ag: &Agentes) -> Result<(), ErroAgentes> {
    ag.validar()?;
    let texto = ag.para_toml()?;
    crate::nucleo::caminhos::escreve_atomico(alvo, &texto)
        .map_err(|msg| ErroAgentes::Io { caminho: alvo.to_path_buf(), msg })
}

/// **O quê:** grava `ag` como o `agentes.toml` global. **Onde:** a janela (aba Agentes).
pub fn salvar_global(ag: &Agentes) -> Result<(), ErroAgentes> {
    salvar_em(&caminho_global(), ag)
}

/// **O quê:** grava `ag` como o override do `projeto`. **Onde:** a janela (aba Agentes).
pub fn salvar_projeto(projeto: &Path, ag: &Agentes) -> Result<(), ErroAgentes> {
    salvar_em(&caminho_projeto(projeto), ag)
}

/// **O quê:** parse+merge de texto sobre os padrões (sem disco). **Onde:** testes.
#[cfg(test)]
pub(crate) fn de_texto(texto: &str) -> Result<Agentes, ErroAgentes> {
    let mut ag = Agentes::padrao();
    aplica(&mut ag, texto, "<texto>")?;
    ag.validar()?;
    Ok(ag)
}

/// **O quê:** carga com caminhos explícitos. **Onde:** testes (sem tocar o HOME real).
#[cfg(test)]
pub(crate) fn carregar_caminhos(
    global: &Path,
    projeto: Option<&Path>,
) -> Result<Agentes, ErroAgentes> {
    carregar_de(global, projeto)
}
