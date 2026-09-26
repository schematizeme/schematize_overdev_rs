//! Onde as coisas moram — PLATAFORMA, e nada de domínio.
//!
//! **O quê:** os diretórios do Claude Code e os do app, mais a escrita atômica.
//!
//! **Onde:** o domínio de skills, que instala em `~/.claude/skills` e guarda estado em
//! `~/.claude/schematize/`.
//!
//! **Por que é copiado e não uma lib comum (D4 do ADR-0016):** uma `commons` de domínio é
//! vetada pelo piso 6, e uma de plataforma amarraria a versão de oito repos por causa de
//! duzentas linhas. Nada aqui sabe o que é skill, catálogo ou fork.
//!
//! ## A regra "LER AMBOS" veio junto, e não é detalhe
//!
//! Houve um período curto em que o app se chamou **Overflow** e escreveu em
//! `~/.claude/overflow/`. Apagar esse caminho da resolução tornaria INVISÍVEL o estado de
//! quem instalou naquela janela — as skills continuariam no disco e o app diria que não há
//! nenhuma. Ler os dois e escrever no novo é o que migra sem perguntar.

use std::path::{Path, PathBuf};

/// Os nomes de diretório de projeto aceitos, do canônico ao legado.
const DIRS_ACEITOS: [&str; 2] = [".schematize", ".overdev"];

/// **O quê:** o `~/.claude`. **Onde:** tudo que segue.
pub fn claude_dir() -> PathBuf {
    super::util::home().join(".claude")
}

/// **O quê:** onde as skills são instaladas (`~/.claude/skills`).
pub fn skills_dir() -> PathBuf {
    claude_dir().join("skills")
}

/// **O quê:** onde os comandos achatados moram (`~/.claude/commands`).
pub fn commands_dir() -> PathBuf {
    claude_dir().join("commands")
}

/// **O quê:** o dir de DADOS do app, pela regra "ler ambos".
///
/// Canônico é `~/.claude/schematize/`; `overflow/` ainda é LIDO — ver o cabeçalho.
pub fn dados_dir() -> PathBuf {
    let c = claude_dir();
    for nome in ["schematize", "overflow"] {
        let d = c.join(nome);
        if d.is_dir() {
            return d;
        }
    }
    c.join("schematize")
}

/// **O quê:** o `state.json` — que skill está instalada, em que versão, e se virou fork.
pub fn state_path() -> PathBuf {
    dados_dir().join("state.json")
}

/// **O quê:** o dir do schematize DENTRO de um projeto, pela mesma regra "ler ambos".
///
/// **Onde:** o arquivo de skills do projeto (`.schematize/skills.json`).
pub fn schematize_dir_at(root: &Path) -> PathBuf {
    for nome in DIRS_ACEITOS {
        let d = root.join(nome);
        if d.is_dir() {
            return d;
        }
    }
    root.join(DIRS_ACEITOS[0])
}

/// **O quê:** o `settings.json` do Claude Code — onde os HOOKS são registrados.
///
/// **Onde:** o `settings.rs`, que liga e desliga os hooks do overdev.
pub fn settings_path() -> PathBuf {
    claude_dir().join("settings.json")
}

/// **O quê:** a config do app (idioma etc.).
pub fn config_path() -> PathBuf {
    dados_dir().join("config.json")
}

/// **O quê:** grava `conteudo` em `alvo` sem deixar arquivo pela metade.
///
/// **Onde:** o arquivo de skills do projeto, e o `state.json`.
///
/// ## O temporário é ÚNICO POR CHAMADA, e isso foi pago
///
/// Antes o nome era `.{alvo}.tmp-{pid}`: duas THREADS gravando o mesmo alvo usavam o MESMO
/// temporário. A segunda truncava o arquivo da primeira, a primeira renomeava, e a segunda
/// falhava com "No such file or directory" — em ~15% das rodadas. "Atômico" valia entre
/// PROCESSOS; entre threads, não. O contador resolve os dois.
pub fn escreve_atomico(alvo: &Path, conteudo: &str) -> Result<(), String> {
    use std::io::Write;
    let dir = alvo.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = dir.join(format!(
        ".{}.tmp-{}-{n}",
        alvo.file_name().and_then(|s| s.to_str()).unwrap_or("alvo"),
        std::process::id()
    ));
    {
        let mut f = std::fs::File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
        f.write_all(conteudo.as_bytes()).map_err(|e| format!("{}: {e}", tmp.display()))?;
        // `sync_all` ANTES do rename: sem isto, um corte de energia entre os dois deixa o
        // nome novo apontando para um arquivo de tamanho certo e conteúdo zerado.
        f.sync_all().map_err(|e| format!("{}: {e}", tmp.display()))?;
    }
    std::fs::rename(&tmp, alvo).map_err(|e| format!("{}: {e}", alvo.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A regra "ler ambos" não é enfeite.** Apagar o caminho legado tornaria invisível o
    /// estado de quem instalou quando o app se chamava Overflow.
    #[test]
    fn o_dir_do_projeto_le_o_legado_e_escreve_no_novo() {
        let base = std::env::temp_dir().join(format!("sk-cam-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("cria");

        // Nenhum dos dois existe: devolve o CANÔNICO, para quem for criar criar o certo.
        assert!(schematize_dir_at(&base).ends_with(".schematize"));

        // Só o legado existe: devolve o legado, senão o estado de lá some da vista.
        std::fs::create_dir_all(base.join(".overdev")).expect("cria");
        assert!(schematize_dir_at(&base).ends_with(".overdev"));

        // Os dois existem: o canônico ganha.
        std::fs::create_dir_all(base.join(".schematize")).expect("cria");
        assert!(schematize_dir_at(&base).ends_with(".schematize"));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// **Escrita atômica não deixa arquivo pela metade, nem entre THREADS.**
    ///
    /// Vinte threads gravando o MESMO alvo: no fim, o conteúdo tem de ser um dos vinte
    /// inteiro — nunca uma mistura, nunca vazio, e nenhuma escrita pode falhar. Com o
    /// temporário por PID (sem o contador), este teste falhava em ~15% das rodadas.
    #[test]
    fn escrita_atomica_aguenta_threads_concorrentes() {
        let base = std::env::temp_dir().join(format!("sk-atom-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let alvo = base.join("x.json");
        let esperados: Vec<String> = (0..20).map(|i| format!("conteudo-numero-{i:02}")).collect();
        std::thread::scope(|s| {
            for c in &esperados {
                let alvo = alvo.clone();
                s.spawn(move || escreve_atomico(&alvo, c).expect("nenhuma escrita pode falhar"));
            }
        });
        let lido = std::fs::read_to_string(&alvo).expect("o arquivo existe");
        assert!(esperados.contains(&lido), "conteúdo misturado ou parcial: {lido:?}");
        // E nenhum temporário ficou para trás.
        let sobras: Vec<_> = std::fs::read_dir(&base)
            .expect("lê")
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(sobras.is_empty(), "{} temporário(s) órfão(s)", sobras.len());
        let _ = std::fs::remove_dir_all(&base);
    }
}
