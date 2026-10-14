//! **Hooks** — o comando que o `settings.json` do Claude Code guarda para `check`/`guard`.
//!
//! **O quê:** o trait [`Hooks`] (como a plataforma ESCREVE o comando de um hook e como
//! RECONHECE um comando que já está no arquivo) e a implementação Unix [`HooksUnix`], que é o
//! one-liner POSIX de sempre, byte a byte (travado por `tests/snapshot_f4b.rs`).
//!
//! **Onde:** `settings` (enable/refresh/reparo/diagnóstico) recebe um `&dyn Hooks`; o repo
//! `schematize_windows_rs` injeta o dele (caminho absoluto do `.exe` entre aspas) via
//! [`crate::cli::run`] e [`crate::gui::run`] (ADR-0026).

/// Como uma plataforma escreve e reconhece o comando de um hook do Claude Code.
///
/// **Onde:** `settings::{enable, enable_vps, hooks_atualizados, refresh_hooks, repara_hooks_em}`.
pub trait Hooks: Send + Sync {
    /// **O quê:** o comando COMPLETO do hook para o subcomando `sub` (`check`, `guard`,
    /// `vps guard`). Tem de ser de UMA linha, seguro para JSON e **nunca** sair não-zero
    /// quando o binário não existe (hook `PreToolUse` que falha bloqueia a sessão inteira).
    ///
    /// **Onde:** gravação no `settings.json` e comparação "está atualizado?".
    fn comando(&self, sub: &str) -> String;

    /// **O quê:** `cmd` (como está no `settings.json`) está num formato que ESTA plataforma sabe
    /// avaliar (caminhos citados, existência)? `false` = comando alheio a esta plataforma: não é
    /// julgado quebrado nem reescrito.
    ///
    /// **Onde:** `settings::comando_quebrado`.
    fn reconhece(&self, cmd: &str) -> bool;
}

/// Hooks Unix (linux/freebsd/macos): o one-liner POSIX que procura o binário em runtime.
///
/// **Onde:** composição do `main` do host em Unix.
#[derive(Debug, Clone, Copy, Default)]
pub struct HooksUnix;

impl Hooks for HooksUnix {
    /// **O quê:** [`comando_unix`]. **Onde:** `settings`.
    fn comando(&self, sub: &str) -> String {
        comando_unix(sub)
    }

    /// **O quê:** formato Unix? Um caminho de unidade do Windows (`C:\...`) é alheio: o
    /// resolvedor Unix não sabe julgá-lo. **Onde:** `settings::comando_quebrado`.
    fn reconhece(&self, cmd: &str) -> bool {
        !cmd.contains(":\\")
    }
}

/// Comando de hook RESILIENTE: resolve o `schematize` em RUNTIME (o `exe` de agora como 1ª pista,
/// depois ~/.cargo/bin, /usr/local/bin, /usr/bin e o PATH) e roda `<sub>`. Se NÃO achar o binário,
/// sai 0 (não bloqueia, não erra) — antes gravava um caminho absoluto que virava stale (ex.: o
/// pacote saía e ficava `/usr/bin/schematize: not found` em todo Stop). "Prever macacos".
pub fn comando_unix(sub: &str) -> String {
    // NÃO embute o caminho de quem gerou o comando — nem o binário em execução.
    //
    // Embutir tornava o comando instável: gerado a partir de um `target/release/`, o hook
    // ficava pinado no diretório de build; gerado de um binário temporário, ficava pinado nele
    // para sempre. E a checagem de "está atualizado?" trocava de resposta conforme de onde o
    // comando foi chamado, reescrevendo o settings à toa.
    //
    // ## Procura DOIS binários, e a ordem é a do dono
    //
    // Depois da E6, quem responde `check`/`guard` é o `schematize-overdev`. Mas o `schematize`
    // continua respondendo `overdev check` — ele delega. O hook tenta o dono primeiro e o hub
    // como rede, e assim ele funciona:
    //
    // - com os dois instalados (o normal);
    // - só com o hub (quem ainda não instalou o app novo);
    // - só com o app novo (quem instalou só ele).
    //
    // **Sem nenhum dos dois, AVISA e sai 0.** Antes saía 0 mudo, e o guard ficava desligado
    // sem ninguém saber. Agora vai UMA linha no stderr (POSIX `printf`, aspas simples, sem
    // quebra de linha — segura para JSON e para `sh -c`) com o comando de instalação, que deve
    // ficar igual ao do hub (`schematize_cli_rs/src/instalardica.rs`). O repo ainda não é
    // público, então o `cargo install --git` só funciona para quem tem acesso. Um hook `PreToolUse` que sai não-zero bloqueia TODA
    // chamada de ferramenta da sessão — e bloquear a sessão de alguém porque um binário não
    // está instalado é o pior desfecho possível deste arquivo.
    //
    // `overflow` está na lista porque houve um interregno curto em que o app se chamou assim;
    // quem gravou hook naquela janela ficou com o nome antigo, e a auto-cura regrava o
    // canônico na primeira oportunidade.
    format!(
        r#"for c in "$HOME/.cargo/bin/schematize-overdev" "$HOME/.local/bin/schematize-overdev" /usr/local/bin/schematize-overdev /usr/bin/schematize-overdev "$(command -v schematize-overdev 2>/dev/null)"; do [ -n "$c" ] && [ -x "$c" ] && exec "$c" {sub}; done; for c in "$HOME/.cargo/bin/schematize" "$HOME/.local/bin/schematize" /usr/local/bin/schematize /usr/bin/schematize "$HOME/.cargo/bin/overflow" "$HOME/.local/bin/overflow" /usr/local/bin/overflow /usr/bin/overflow "$(command -v schematize 2>/dev/null)"; do [ -n "$c" ] && [ -x "$c" ] && exec "$c" overdev {sub}; done; printf '%s\n' 'schematize-overdev: o hook do overdev esta instalado mas nenhum binario overdev foi encontrado - o guard esta DESLIGADO. Instale: cargo install --git https://github.com/schematizeme/schematize_overdev_rs --locked' >&2; exit 0"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O comando é igual em qualquer máquina e sai 0 sem binário.
    #[test]
    fn comando_unix_nao_carrega_caminho_de_quem_gerou() {
        let c = HooksUnix.comando("check");
        assert!(c.ends_with("exit 0"), "{c}");
        assert!(!c.contains('\n'), "uma linha só: {c}");
    }

    /// Caminho de unidade do Windows não é formato Unix; o resto é.
    #[test]
    fn reconhece_so_o_formato_unix() {
        assert!(HooksUnix.reconhece("/usr/bin/schematize overdev check"));
        assert!(HooksUnix.reconhece("bash .claude/hooks/overdev-stop.sh"));
        assert!(!HooksUnix.reconhece("\"C:\\Users\\x\\schematize-overdev.exe\" check"));
    }
}
