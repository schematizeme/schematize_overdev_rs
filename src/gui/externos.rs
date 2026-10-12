//! A CASCA FINA para os OUTROS binários da casa: `schematize-skills`, `schematize-git` e `schematize`.
//!
//! **O quê:** acha o binário irmão, roda um subcomando COM o projeto como diretório corrente e
//! devolve o texto ou o MOTIVO da falha — e traz os leitores PUROS dos três documentos que a tela
//! de projeto do hub desenha: as ações das skills, as skills atrasadas e os commits.
//!
//! **Onde:** a aba Projeto ([`super::home`]), o sino ([`super::sino`]) e a versão do hub no topo.
//! É o MESMO contrato que o hub usa (`schematize-skills acoes --json`, `schematize-skills applied
//! --json`, `schematize-git log --json`): a janela é uma casca e não copia domínio nenhum — só lê
//! o documento que o app dono emite.
//!
//! ## Ausência de app nunca é silêncio
//!
//! O hub devolve lista vazia quando o app não está instalado. Aqui o erro SOBE como texto
//! acionável e a tela o mostra: «sem ações de skills» e «o app não está instalado» são frases
//! diferentes, e a pessoa precisa saber qual é a verdadeira. Documento ilegível também é `Err`.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// **O quê:** o caminho do binário irmão `nome`: ao lado deste executável, depois
/// `~/.cargo/bin`, e por fim o nome puro (o `PATH` decide). **Onde:** [`rodar`].
pub fn bin_irmao(nome: &str) -> PathBuf {
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf))
    {
        let c = dir.join(nome);
        if c.is_file() {
            return c;
        }
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        let c = home.join(".cargo").join("bin").join(nome);
        if c.is_file() {
            return c;
        }
    }
    PathBuf::from(nome)
}

/// **O quê:** roda `nome args…` em `dir` e devolve o stdout, ou o motivo (stderr, ou «não está
/// instalado»). **Onde:** as leituras da aba Projeto, o sino e a versão do hub.
///
/// Sem `NO_COLOR` o `schematize notifications` imprime ANSI; ele é ligado aqui para todo filho.
pub fn rodar(nome: &str, dir: &Path, args: &[&str]) -> Result<String, String> {
    let bin = bin_irmao(nome);
    let out = Command::new(&bin)
        .args(args)
        .current_dir(dir)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                format!("o app `{nome}` não está instalado nesta máquina")
            } else {
                format!("não consegui executar {}: {e}", bin.display())
            }
        })?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = err.trim();
        return Err(if err.is_empty() {
            format!("`{nome} {}` falhou ({})", args.join(" "), out.status)
        } else {
            err.to_string()
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn doc(texto: &str, quem: &str) -> Result<Value, String> {
    serde_json::from_str(texto).map_err(|e| format!("resposta ilegível do {quem}: {e}"))
}

fn campo(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or_default().trim().to_string()
}

// ---------------------------------------------------------------------------------------------
// Ações das skills
// ---------------------------------------------------------------------------------------------

/// Uma ação declarada por uma skill (um botão). `comando` vem CRU de terceiro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Acao {
    pub rotulo: String,
    pub comando: String,
    pub precisa_projeto: bool,
}

/// **O quê:** `schematize-skills acoes --json` → as ações. **Onde:** a carga da aba Projeto.
///
/// Ação sem rótulo ou sem comando NÃO vira botão (um botão mudo é pior que nenhum). `precisa_projeto`
/// duvidoso conta como `false`. Documento ilegível é `Err`; lista vazia é `Ok(vec![])`.
pub fn ler_acoes(texto: &str) -> Result<Vec<Acao>, String> {
    let v = doc(texto, "schematize-skills")?;
    let itens = v
        .get("acoes")
        .and_then(Value::as_array)
        .ok_or_else(|| "resposta ilegível do schematize-skills: faltou `acoes`".to_string())?;
    Ok(itens
        .iter()
        .filter_map(|a| {
            let (rotulo, comando) = (campo(a, "rotulo"), campo(a, "comando"));
            (!rotulo.is_empty() && !comando.is_empty()).then(|| Acao {
                rotulo,
                comando,
                precisa_projeto: a.get("precisa_projeto").and_then(Value::as_bool) == Some(true),
            })
        })
        .collect())
}

/// Skills que ficaram para trás neste projeto, e o prompt que as reaplica.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Aplicadas {
    /// `(slug, versão aplicada, versão instalada)`.
    pub atrasadas: Vec<(String, String, String)>,
    pub prompt: String,
}

/// **O quê:** `schematize-skills applied --json` → as atrasadas + o prompt. **Onde:** a carga da
/// aba Projeto.
///
/// Prompt sem atrasadas (ou o inverso) zera tudo: um botão sem tarefa, ou uma contagem sem
/// prompt para agir, promete o que não faz.
pub fn ler_aplicadas(texto: &str) -> Result<Aplicadas, String> {
    let v = doc(texto, "schematize-skills")?;
    let itens = v
        .get("atrasadas")
        .and_then(Value::as_array)
        .ok_or_else(|| "resposta ilegível do schematize-skills: faltou `atrasadas`".to_string())?;
    let atrasadas: Vec<_> = itens
        .iter()
        .filter_map(|a| {
            let slug = campo(a, "slug");
            (!slug.is_empty()).then(|| (slug, campo(a, "aplicada"), campo(a, "instalada")))
        })
        .collect();
    let prompt = campo(&v, "prompt");
    if atrasadas.is_empty() || prompt.is_empty() {
        return Ok(Aplicadas::default());
    }
    Ok(Aplicadas { atrasadas, prompt })
}

// ---------------------------------------------------------------------------------------------
// Commits
// ---------------------------------------------------------------------------------------------

/// Um commit, no mínimo que a tela desenha. `pushed` duvidoso conta como NÃO enviado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub short: String,
    pub author: String,
    pub date: String,
    pub subject: String,
    pub pushed: bool,
}

/// O ramo e a distância dele para o remoto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upstream {
    pub branch: String,
    pub remote: Option<String>,
    pub ahead: u64,
    pub behind: u64,
}

impl Upstream {
    /// **O quê:** a linha «ramo → remoto · ↑a ↓b» do hub. **Onde:** a seção Commits.
    pub fn linha(&self) -> String {
        let remoto = self.remote.clone().unwrap_or_else(|| "—".to_string());
        format!("{} → {} · ↑{} ↓{}", self.branch, remoto, self.ahead, self.behind)
    }
}

/// **O quê:** `schematize-git log --json` → `(upstream, commits)`. **Onde:** a carga da aba Projeto.
///
/// `branch` ausente derruba o upstream inteiro (uma linha «→ origin» sem ramo afirma o que
/// ninguém mediu). `pushed` ausente/de outro tipo é «não enviado» — o lado seguro.
pub fn ler_commits(texto: &str) -> Result<(Option<Upstream>, Vec<Commit>), String> {
    let v = doc(texto, "schematize-git")?;
    let up = v.get("upstream").and_then(|u| {
        Some(Upstream {
            branch: u.get("branch")?.as_str()?.to_string(),
            remote: u.get("remote").and_then(Value::as_str).map(str::to_string),
            ahead: u.get("ahead").and_then(Value::as_u64).unwrap_or(0),
            behind: u.get("behind").and_then(Value::as_u64).unwrap_or(0),
        })
    });
    let commits = v
        .get("commits")
        .and_then(Value::as_array)
        .ok_or_else(|| "resposta ilegível do schematize-git: faltou `commits`".to_string())?
        .iter()
        .map(|c| Commit {
            short: campo(c, "short"),
            author: campo(c, "author"),
            date: campo(c, "date"),
            subject: campo(c, "subject"),
            pushed: c.get("pushed").and_then(Value::as_bool) == Some(true),
        })
        .collect();
    Ok((up, commits))
}

// ---------------------------------------------------------------------------------------------
// Terminal e versão
// ---------------------------------------------------------------------------------------------

/// **O quê:** `terminal --json` → o nome do terminal aberto. **Onde:** as ações que abrem o
/// `claude` num terminal (skills, afazeres do archive, rerun, «abrir no terminal»).
pub fn ler_terminal(texto: &str) -> Result<String, String> {
    let nome = campo(&doc(texto, "schematize-overdev")?, "terminal");
    (!nome.is_empty())
        .then_some(nome)
        .ok_or_else(|| "resposta ilegível do schematize-overdev: faltou `terminal`".to_string())
}

/// **O quê:** «schematize 0.66.1 (726ed18aef2e)» → «schematize v0.66.1». **Onde:** o topo.
pub fn ler_versao_do_hub(texto: &str) -> Result<String, String> {
    let mut it = texto.split_whitespace();
    match (it.next(), it.next()) {
        (Some(nome), Some(v)) if v.chars().next().is_some_and(|c| c.is_ascii_digit()) => {
            Ok(format!("{nome} v{v}"))
        }
        _ => Err(format!("versão ilegível: {}", texto.trim())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACOES: &str = r#"{"acoes":[
      {"skill":"qa","rotulo":"Q.A.","comando":"/qa-plan","precisa_projeto":true},
      {"skill":"archive","rotulo":"Archive","comando":"/archive-init"}]}"#;

    #[test]
    fn le_as_acoes_e_precisa_projeto_duvidoso_e_falso() {
        let a = ler_acoes(ACOES).unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!((a[0].rotulo.as_str(), a[0].comando.as_str()), ("Q.A.", "/qa-plan"));
        assert!(a[0].precisa_projeto && !a[1].precisa_projeto);
        let b = ler_acoes(r#"{"acoes":[{"rotulo":"A","comando":"/a","precisa_projeto":"sim"}]}"#);
        assert!(!b.unwrap()[0].precisa_projeto);
    }

    #[test]
    fn acao_incompleta_nao_vira_botao_e_o_comando_sai_cru() {
        let a = ler_acoes(
            r#"{"acoes":[{"rotulo":"","comando":"/a"},{"rotulo":"B","comando":"  "},
            {"rotulo":"C"},{"comando":"/d"},{"rotulo":"X","comando":"/c --f \"q\" && echo"}]}"#,
        )
        .unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].comando, "/c --f \"q\" && echo");
    }

    #[test]
    fn documento_ruim_e_erro_e_nunca_lista_vazia_calada() {
        let fundo = format!("{}{}", "[".repeat(3000), "]".repeat(3000));
        for lixo in ["", "null", "[]", "0", "{ nao e json", "\u{0}", &fundo, r#"{"acoes":7}"#] {
            assert!(ler_acoes(lixo).is_err(), "{lixo:?}");
            assert!(ler_aplicadas(lixo).is_err(), "{lixo:?}");
            assert!(ler_commits(lixo).is_err(), "{lixo:?}");
        }
        assert_eq!(ler_acoes(r#"{"acoes":[]}"#).unwrap(), vec![], "vazio legítimo é Ok");
    }

    #[test]
    fn aplicadas_so_vale_com_atrasada_e_prompt() {
        let ok =
            r#"{"atrasadas":[{"slug":"qa","aplicada":"1","instalada":"2"}],"prompt":"reaplique"}"#;
        let a = ler_aplicadas(ok).unwrap();
        assert_eq!(a.atrasadas, [("qa".into(), "1".into(), "2".into())]);
        assert_eq!(a.prompt, "reaplique");
        for meio in [
            r#"{"atrasadas":[{"slug":"qa"}],"prompt":""}"#,
            r#"{"atrasadas":[],"prompt":"x"}"#,
            r#"{"atrasadas":[{"aplicada":"1"}],"prompt":"x"}"#,
        ] {
            assert_eq!(ler_aplicadas(meio).unwrap(), Aplicadas::default(), "{meio}");
        }
    }

    #[test]
    fn commits_leem_upstream_e_pushed_duvidoso_e_nao_enviado() {
        let (up, cs) = ler_commits(
            r#"{"upstream":{"branch":"main","remote":"origin/main","ahead":2,"behind":1},
            "commits":[{"short":"a","pushed":true},{"short":"b","pushed":"sim"},{"short":"c"}]}"#,
        )
        .unwrap();
        assert_eq!(up.unwrap().linha(), "main → origin/main · ↑2 ↓1");
        assert_eq!(cs.iter().map(|c| c.pushed).collect::<Vec<_>>(), [true, false, false]);
        let (up, _) = ler_commits(r#"{"upstream":{"remote":"o","ahead":9},"commits":[]}"#).unwrap();
        assert!(up.is_none(), "sem ramo não há upstream");
        let sem_remoto = Upstream { branch: "m".into(), remote: None, ahead: 0, behind: 0 };
        assert_eq!(sem_remoto.linha(), "m → — · ↑0 ↓0");
    }

    #[test]
    fn terminal_e_versao() {
        assert_eq!(ler_terminal(r#"{"terminal":"kitty"}"#).unwrap(), "kitty");
        assert!(ler_terminal(r#"{"terminal":""}"#).is_err());
        assert_eq!(
            ler_versao_do_hub("schematize 0.66.1 (726ed18aef2e)\n").unwrap(),
            "schematize v0.66.1"
        );
        assert!(ler_versao_do_hub("").is_err() && ler_versao_do_hub("oi mundo").is_err());
    }

    #[test]
    fn binario_ausente_diz_que_nao_esta_instalado() {
        let e = rodar("schematize-que-nao-existe-42", Path::new("."), &["x"]).unwrap_err();
        assert!(e.contains("não está instalado"), "{e}");
    }

    #[test]
    fn a_casca_nao_sabe_nada_do_dominio_do_overdev() {
        let prod = include_str!("externos.rs").split("#[cfg(test)]").next().unwrap().to_lowercase();
        for palavra in ["checklist", "snapshot", "quiz"] {
            let n = prod
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .any(|l| l.contains(palavra));
            assert!(!n, "`{palavra}` é domínio do overdev");
        }
    }
}
