//! A casca de PLATAFORMA — caminhos do HOME, leitura segura e execução de comando.
//!
//! **Onde:** os módulos de domínio deste app.
//!
//! ## Por que é copiada, e o que isso significa (D4 do ADR-0016)
//!
//! Estas quatro funções eram `schematize::util::{home, dados_dir, ler_para_modificar, run}` no
//! app principal. Vieram junto com o domínio, porque **eram a única coisa que ele importava de
//! fora**. Depender do crate do hub por causa delas faria este app precisar do hub para existir
//! — o inverso do piso 10, que manda cada serviço subir sozinho.
//!
//! O duplicado é **plataforma**: nenhuma destas funções sabe o que é uma conta de git. Se um dia
//! algo de domínio migrar para este arquivo, o corte foi feito errado.

use std::path::PathBuf;
use std::process::Command;

/// **O quê:** o HOME do usuário, com a cadeia de fallback do Windows.
///
/// **Onde:** [`dados_dir`] e os módulos que leem `~/.ssh` e `~/.gitconfig`.
///
/// **A cadeia existe e é testada** porque `USERPROFILE`/`HOMEDRIVE`+`HOMEPATH` são o que o
/// Windows dá. Em Linux o primeiro ramo sempre vence, e por isso a lógica está numa função
/// PURA ([`resolver_home`]): testar a versão que lê o ambiente exigiria mexer no `HOME` do
/// processo, que é estado global — e testes de Rust rodam em paralelo, roubando-o uns dos
/// outros.
pub fn home() -> PathBuf {
    let ler = |k: &str| {
        std::env::var_os(k).filter(|v| !v.is_empty()).map(|v| v.to_string_lossy().into_owned())
    };
    resolver_home(ler("HOME"), ler("USERPROFILE"), ler("HOMEDRIVE"), ler("HOMEPATH"))
}

/// **O quê:** a cadeia de resolução do HOME, sobre valores em vez de variáveis de ambiente.
///
/// **Onde:** [`home`] e os testes.
pub fn resolver_home(
    home: Option<String>,
    userprofile: Option<String>,
    homedrive: Option<String>,
    homepath: Option<String>,
) -> PathBuf {
    if let Some(h) = home {
        return PathBuf::from(h);
    }
    if let Some(u) = userprofile {
        return PathBuf::from(u);
    }
    if let (Some(d), Some(p)) = (homedrive, homepath) {
        return PathBuf::from(format!("{d}{p}"));
    }
    PathBuf::from(".")
}

/// **O quê:** `~/.claude/schematize/` — onde os dados do ecossistema moram.
///
/// **Onde:** [`crate::contas`], para achar o cadastro.
///
/// **O `overflow` é o nome de um interregno** e continua sendo lido: uma máquina que instalou o
/// app naquela janela tem os dados lá, e ignorá-los faria o cadastro sumir sem erro nenhum.
pub fn dados_dir() -> PathBuf {
    let claude = home().join(".claude");
    let canonico = claude.join("schematize");
    if canonico.is_dir() {
        return canonico;
    }
    let interregno = claude.join("overflow");
    if interregno.is_dir() {
        return interregno;
    }
    canonico
}

/// **O quê:** lê um arquivo que vai ser REESCRITO, tratando "não existe" como vazio.
///
/// **Onde:** a escrita do `~/.ssh/config`.
///
/// **A distinção é o ponto:** não existir é estado normal (quem escreve o arquivo o cria). Um
/// erro de leitura de verdade — permissão, disco — **não** pode virar string vazia, porque
/// reescrever a partir de vazio APAGARIA o que está lá.
pub fn ler_para_modificar(p: &std::path::Path) -> Result<String, String> {
    match std::fs::read_to_string(p) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!(
            "{}: não deu pra ler ({e}). Não vou reescrever este arquivo às cegas — \
             reescrever a partir de um conteúdo vazio apagaria o que está lá.",
            p.display()
        )),
    }
}

/// **O quê:** roda um comando externo capturando o stdout; o erro traz o stderr.
///
/// **Onde:** toda chamada a `git` e a `gh` deste app.
///
/// **O stderr entra no erro de propósito:** é onde o `git` e o `gh` explicam o que houve, e um
/// erro que diz só "falhou" manda a pessoa adivinhar.
pub fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("falha ao executar {cmd}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "{cmd} falhou ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// **O quê:** o instante atual em segundos desde a época Unix.
///
/// **Onde:** o `installed_at` de cada skill instalada.
///
/// **Devolve `0` se o relógio estiver antes de 1970**, em vez de propagar erro. É um carimbo
/// informativo: um `0` diz "não sei quando", e uma máquina com relógio quebrado não deveria
/// impedir a instalação de uma skill.
pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// **O quê:** o dir do app no HOME, pela regra "ler ambos".
///
/// **Onde:** o stash do fork — a cópia oficial guardada quando uma skill vira editável.
///
/// Canônico é `~/.schematize`; `~/.overflow` ainda é LIDO, porque houve um período em que o
/// app se chamou assim. Ver o cabeçalho de [`super::caminhos`].
pub fn home_app_dir() -> PathBuf {
    for nome in [".schematize", ".overflow"] {
        let d = home().join(nome);
        if d.is_dir() {
            return d;
        }
    }
    home().join(".schematize")
}

/// **O quê:** abre uma URL no navegador padrão, sem bloquear. Best-effort.
///
/// **Onde:** o painel HTML do overdev, quando se pede "abrir no navegador".
///
/// **Não devolve erro de propósito.** O abridor falha em ambiente sem sessão gráfica, e um
/// `Result` aqui obrigaria cada chamador a decidir o que fazer com uma falha sobre a qual não
/// há o que fazer — o painel já está no disco, e o caminho dele aparece na tela.
pub fn open_url(url: &str) {
    // Pelo seam `Abridor` da plataforma (xdg-open/open); o erro é descartado de propósito.
    let _ = crate::ambiente::plat().abridor.abrir(url);
}

/// **O quê:** o caminho ABSOLUTO do próprio binário.
///
/// **Onde:** o registro dos hooks no `settings.json` do Claude Code.
///
/// **Absoluto, e é o ponto:** o hook roda num ambiente cujo PATH não é o do terminal de quem
/// instalou. Gravar o nome puro faz o hook falhar em silêncio — e um hook de `PreToolUse` que
/// falha bloqueia TODA chamada de ferramenta da sessão.
pub fn self_exe() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "schematize-overdev".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cadeia do HOME, ramo a ramo — inclusive os que Linux nunca alcança.
    #[test]
    fn a_cadeia_do_home_cobre_os_quatro_ramos() {
        assert_eq!(resolver_home(Some("/h".into()), None, None, None), PathBuf::from("/h"));
        assert_eq!(
            resolver_home(None, Some("C:\\u".into()), None, None),
            PathBuf::from("C:\\u"),
            "sem HOME, o USERPROFILE do Windows vale"
        );
        assert_eq!(
            resolver_home(None, None, Some("C:".into()), Some("\\u".into())),
            PathBuf::from("C:\\u"),
            "HOMEDRIVE + HOMEPATH é o último recurso do Windows"
        );
        assert_eq!(
            resolver_home(None, None, Some("C:".into()), None),
            PathBuf::from("."),
            "HOMEDRIVE sozinho não forma caminho"
        );
        assert_eq!(resolver_home(None, None, None, None), PathBuf::from("."));
    }

    /// **Arquivo ausente é vazio; erro de leitura NÃO é.**
    ///
    /// A diferença existe porque quem chama vai REESCREVER o arquivo — e tratar um erro de
    /// permissão como conteúdo vazio apagaria o que está lá.
    #[test]
    fn ausente_e_vazio_mas_ilegivel_e_erro() {
        let p = std::env::temp_dir().join(format!("git-rs-nao-existe-{}", std::process::id()));
        let _ = std::fs::remove_file(&p);
        assert_eq!(ler_para_modificar(&p).expect("ausente é Ok"), "");

        // Um DIRETÓRIO no lugar do arquivo: `read_to_string` falha com algo que não é NotFound.
        let d = std::env::temp_dir().join(format!("git-rs-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("criar dir");
        let e = ler_para_modificar(&d).expect_err("diretório tem de ser Err, não string vazia");
        assert!(e.contains("às cegas"), "a mensagem tem de dizer POR QUE recusou: {e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Comando que não existe vira erro com o nome dele, não panic.
    #[test]
    fn comando_inexistente_da_erro_nomeando_o_comando() {
        let e = run("comando-que-nao-existe-9999", &["--x"]).expect_err("tinha de falhar");
        assert!(e.contains("comando-que-nao-existe-9999"), "{e}");
    }

    /// Comando que falha traz o stderr — é onde o `git` explica o que houve.
    #[test]
    fn comando_que_falha_traz_o_stderr() {
        // `git` num diretório que não é repo: falha e escreve no stderr.
        let d = std::env::temp_dir().join(format!("git-rs-norepo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("dir");
        let out = Command::new("git")
            .args(["-C", d.to_str().unwrap(), "status"])
            .output()
            .expect("git existe em qualquer máquina de dev");
        assert!(!out.status.success(), "git status fora de repo tem de falhar");
        assert!(!out.stderr.is_empty(), "e ele escreve no stderr — que é o que `run` propaga");
        let _ = std::fs::remove_dir_all(&d);
    }
}
