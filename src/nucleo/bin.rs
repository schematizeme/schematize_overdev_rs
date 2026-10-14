//! Resolver de executável — PLATAFORMA, e nada de domínio.
//!
//! **O quê:** acha um binário irmão no disco, em caminho ABSOLUTO.
//!
//! **Onde:** [`super::desktop`], para descobrir se a janela está instalada.
//!
//! **Por que absoluto importa:** o lançador do ambiente gráfico dá **PATH mínimo** — não lê
//! `~/.bashrc` nem `~/.profile` —, então um `Exec=schematize-overdev-gui` não acharia o
//! binário em `~/.cargo/bin`. É a armadilha que já quebrou o lançador do hub.

/// **O quê:** o caminho absoluto de `nome`, procurando nos lugares em que a casa instala
/// (`~/.cargo/bin` primeiro, depois o `$PATH`; a extensão do SO fica por conta da lib).
///
/// **Onde:** [`super::desktop::resolver_gui`] e o `rerun`.
///
/// **A ordem é a da confiança:** `~/.cargo/bin` primeiro, porque é onde o `install.sh` e o
/// `schematize-market` põem tudo, e é o par que se atualiza junto. Só depois o `$PATH`, que
/// pode ter uma cópia velha de outra instalação.
pub use schematize_plataforma::bin::resolve_bin;

/// **O quê:** o diretório home, na variável que cada plataforma usa (`HOME`, depois
/// `USERPROFILE`; vazio ou relativo é descartado).
///
/// **Onde:** os chamadores de [`super::desktop`].
pub use schematize_plataforma::bin::home;

#[cfg(test)]
mod tests {
    use super::*;

    /// Nome que não existe em lugar nenhum devolve `None` — e não um caminho inventado que o
    /// `.desktop` gravaria como `Exec=`.
    #[test]
    fn binario_inexistente_e_none() {
        assert!(resolve_bin("schematize-nao-existe-em-lugar-nenhum-xyz").is_none());
    }

    /// O que é achado vem ABSOLUTO — relativo dependeria do diretório de quem clicou.
    #[test]
    fn o_que_e_achado_vem_absoluto() {
        // `sh` existe em toda máquina POSIX; no Windows o teste ainda vale por vacuidade.
        if let Some(p) = resolve_bin("sh") {
            assert!(p.is_absolute(), "{}", p.display());
        }
    }
}
