//! O subcomando `desktop` — põe o app no menu de aplicativos, ou o tira.
//!
//! **Onde:** [`super::executar`], e o `schematize-market` depois de instalar este app.
//!
//! **Por que a decisão de qual caminho tomar mora aqui e não no núcleo:** o núcleo sabe
//! GRAVAR; esta camada sabe o que a pessoa pediu na linha de comando e o que responder. Um
//! núcleo que lê flag seria um núcleo que não dá para chamar de outro lugar.

use crate::nucleo::{bin, desktop};

/// **O quê:** executa `--install` ou `--remover`, e imprime o que aconteceu.
///
/// **Onde:** o despacho da CLI.
///
/// **Nenhuma das duas flags é o caminho normal, e não é erro:** sem flag, o comando DIZ o que
/// sabe — onde o `.desktop` está (ou estaria) e se ele existe. Um `--help` seco aqui mandaria
/// a pessoa ler a ajuda para descobrir um estado que o comando já tem em mãos (§37.48).
///
/// **As duas juntas é ERRO**, e não "a última ganha": instalar e remover no mesmo comando não
/// tem leitura óbvia, e escolher uma delas em silêncio faria metade das pessoas obter o
/// contrário do que quis.
pub fn executar(install: bool, remover: bool) -> Result<(), String> {
    if install && remover {
        return Err("`--install` e `--remover` juntos não têm leitura óbvia — escolha um".into());
    }
    let home = bin::home().ok_or("não consegui descobrir o seu diretório home")?;

    if remover {
        return match desktop::remover(&home)? {
            true => {
                println!("removido: {}", desktop::arquivo_desktop(&home).display());
                Ok(())
            }
            // Não é erro: o estado desejado é o estado atual. Sair 1 aqui quebraria um
            // desinstalador que roda isto sem saber se o ícone chegou a existir.
            false => {
                println!("nada a remover — este app não está no menu.");
                Ok(())
            }
        };
    }

    if !install {
        let p = desktop::arquivo_desktop(&home);
        let estado = if p.is_file() { "instalado" } else { "NÃO instalado" };
        println!("{estado}: {}", p.display());
        println!("  --install   põe o ícone e a entrada no menu");
        println!("  --remover   tira a entrada do menu");
        return Ok(());
    }

    // O caminho do PRÓPRIO executável, e não um nome adivinhado: gravar um caminho suposto
    // faria o ícone abrir outra coisa (ou nada) na máquina de quem instalou fora do padrão.
    let eu = std::env::current_exe()
        .map_err(|e| format!("não consegui descobrir o caminho deste executável: {e}"))?;
    let p = desktop::instalar(&home, &eu)?;
    println!("✓ {}", p.display());
    match desktop::resolver_gui(&eu) {
        Some(g) => println!("  o ícone abre a JANELA: {}", g.display()),
        // Dizer isto é o ponto: o ícone funciona, mas abre um terminal — e quem esperava
        // janela precisa saber POR QUE, com o comando que resolve, em vez de concluir que o
        // app instalou torto.
        None => println!(
            "  a janela (`{}`) não está instalada — por ora o ícone abre um terminal.\n  \
             Para ter a janela:  cargo install --git https://github.com/schematizeme/\
             schematize_overdev_rs --locked --force",
            desktop::GUI_BIN
        ),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **As duas flags juntas é ERRO, e não "a última ganha".**
    ///
    /// Escolher uma em silêncio faria metade das pessoas obter o contrário do que pediu — e
    /// num desinstalador isso significa deixar no menu um ícone que aponta para um binário
    /// que acabou de ser apagado.
    #[test]
    fn instalar_e_remover_juntos_e_erro() {
        let e = executar(true, true).expect_err("as duas juntas têm de reprovar");
        assert!(e.contains("não têm leitura óbvia"), "{e}");
    }
}
