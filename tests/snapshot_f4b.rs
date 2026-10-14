//! Snapshots de comportamento da ADR-0026 F4b: os comandos de hook e o `.desktop` (com os
//! ícones) saem BYTE A BYTE como antes da extração para os seams.
//!
//! **Onde:** capturados do binário 0.3.1 ANTES de qualquer mudança (`tests/golden/`). Se um
//! destes testes quebra, o comportamento no Linux/FreeBSD mudou.

use std::path::{Path, PathBuf};
use std::process::Command;

use overdev_casa::hooks::{Hooks, HooksUnix};

const BIN: &str = env!("CARGO_BIN_EXE_schematize-overdev");

/// **O quê:** os quatro comandos de hook gerados por [`HooksUnix`] são idênticos aos do 0.3.1.
///
/// **Onde:** teste de contrato do `settings.json` (um hook diferente reescreve o settings de todos).
#[test]
fn hooks_unix_byte_identico_ao_031() {
    let h = HooksUnix;
    assert_eq!(h.comando("check"), include_str!("golden/hook_check.txt"));
    assert_eq!(h.comando("guard"), include_str!("golden/hook_guard.txt"));
    assert_eq!(h.comando("vps guard"), include_str!("golden/hook_vps_guard.txt"));
    assert_eq!(h.comando("overdev check"), include_str!("golden/hook_od_check.txt"));
}

/// **O quê:** a pasta de trabalho do teste, limpa e única.
///
/// **Onde:** os testes de `.desktop`.
fn pasta(nome: &str) -> PathBuf {
    let d =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("f4b-{}-{nome}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("criar a pasta do teste");
    d
}

/// **O quê:** roda `desktop --install` com HOME isolado e devolve o HOME.
///
/// **Onde:** os testes de `.desktop`.
fn instalar(bin: &Path, home: &Path) {
    let o = Command::new(bin)
        .args(["desktop", "--install"])
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env("SCHEMATIZE_SEM_MARKET", "1")
        .output()
        .expect("rodar desktop --install");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}

/// **O quê:** compara o `.desktop` instalado com o molde (`{ROOT}` = pasta do binário/home).
///
/// **Onde:** os dois testes de `.desktop`.
fn confere_desktop(home: &Path, bin_dir: &Path, molde: &str) {
    let t =
        std::fs::read_to_string(home.join(".local/share/applications/schematize-overdev.desktop"))
            .expect("ler o .desktop");
    let esperado = molde
        .replace("{BINDIR}", &bin_dir.display().to_string())
        .replace("{HOME}", &home.display().to_string());
    assert_eq!(t, esperado);
    for s in [16, 24, 32, 48, 64, 128, 256, 512] {
        let png = std::fs::read(home.join(format!(
            ".local/share/icons/hicolor/{s0}x{s0}/apps/schematize-overdev.png",
            s0 = s
        )))
        .expect("ler o ícone");
        let ouro = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/golden/icons/{s}.png")),
        )
        .expect("ler o ícone de ouro");
        assert_eq!(png, ouro, "ícone {s}px mudou");
    }
    for extra in [".icons", ".local/share/icons", ".local/share/pixmaps"] {
        assert!(
            home.join(extra).join("schematize-overdev.png").is_file(),
            "faltou o ícone em {extra}"
        );
    }
}

/// **O quê:** sem a janela ao lado, o `.desktop` é a forma de terminal (`status`).
///
/// **Onde:** snapshot do `.desktop` do 0.3.1.
#[test]
fn desktop_sem_janela_byte_identico() {
    let d = pasta("sem");
    let bin_dir = d.join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let eu = bin_dir.join("schematize-overdev");
    std::fs::copy(BIN, &eu).unwrap();
    let home = d.join("home");
    instalar(&eu, &home);
    confere_desktop(&home, &bin_dir, include_str!("golden/desktop_without.desktop.tpl"));
}

/// **O quê:** com a janela ao lado, o `.desktop` aponta para ela (`Terminal=false`). Este
/// teste só roda se a janela foi compilada junto (`gui`), copiando-a para o par.
///
/// **Onde:** snapshot do `.desktop` do 0.3.1.
#[cfg(feature = "gui")]
#[test]
fn desktop_com_janela_byte_identico() {
    let d = pasta("com");
    let bin_dir = d.join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let eu = bin_dir.join("schematize-overdev");
    std::fs::copy(BIN, &eu).unwrap();
    std::fs::write(bin_dir.join("schematize-overdev-gui"), "x").unwrap();
    let home = d.join("home");
    instalar(&eu, &home);
    confere_desktop(&home, &bin_dir, include_str!("golden/desktop_with.desktop.tpl"));
}
