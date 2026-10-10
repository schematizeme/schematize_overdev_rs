//! Ícone do **Overdev** desenhado EM CÓDIGO.
//!
//! **O quê:** gera o PNG do app em qualquer tamanho, sem asset externo e sem rasterizador.
//!
//! **Onde:** [`super::desktop::instalar`], ao gravar a entrada no menu.
//!
//! ## Por que é cópia do gerador dos irmãos, com outra paleta E outra forma
//!
//! Os apps da casa precisam parecer **da mesma família** no menu (mesmo squircle, mesma
//! construção) e ao mesmo tempo ser **distinguíveis à distância**, que é como se escolhe ícone
//! numa grade. Compartilhar o desenho e trocar cor e disposição dá as duas coisas.
//!
//! - schematize: **azul**, grafo de 3 nós em triângulo.
//! - deployer: **verde-água**, 3 nós em cadeia diagonal.
//! - database: **âmbar**, faixas empilhadas com divisória de coluna (uma tabela).
//! - git: **violeta**, tronco vertical com ramo em ângulo reto (um branch).
//! - skills: **verde**, três blocos em ESCADA — peças que se encaixam.
//! - overdev: **laranja**, a MESMA escada com outra cor — o run é uma sequência de degraus.
//!
//! **A forma não é enfeite.** A escada distingue-se da tabela do database (que é simétrica e
//! tem divisória) e dos dois grafos, sem depender da cor — que é o que se perde primeiro em
//! 16px, em tema claro e para quem não distingue matiz. Uma skill é uma peça que se ENCAIXA,
//! e o degrau é a forma mais curta de dizer isso.
//!
//! ## Por que EM CÓDIGO e não um `.svg` no repo
//!
//! Asset externo some, não entra no `cargo install`, e rasterizá-lo depende de ImageMagick ou
//! rsvg, que falham em SVG com frequência. Do código, o ícone sai em qualquer tamanho, nunca
//! falta, e não quebra o build. Antialiasing por supersampling — nítido de 16px a 1024px.

// Paleta: o MESMO fundo da casa, acento próprio.
const BG_TL: [f32; 3] = [0x14 as f32, 0x16 as f32, 0x1c as f32]; // #14161c (canto sup-esq)
const BG_BR: [f32; 3] = [0x1b as f32, 0x1e as f32, 0x27 as f32]; // #1b1e27 (canto inf-dir)
const ACCENT: [f32; 3] = [0xf0 as f32, 0x8a as f32, 0x3c as f32]; // #f08a3c laranja (Overdev)
const ACCENT_HI: [f32; 3] = [0xff as f32, 0xc2 as f32, 0x8a as f32]; // #ffc28a (o bloco de cima)

/// Amostras por eixo no supersampling (SS×SS por pixel) → bordas suaves sem lib de imagem.
const SS: u32 = 4;

/// Uma faixa da "tabela": centro em Y, meia-altura, e a cor. Em FRAÇÃO do lado.
///
/// **Onde:** [`rgba`] e [`sample`].
///
/// **A de cima é mais clara e mais fina** porque é o CABEÇALHO — é o que faz a pilha ler como
/// tabela e não como três traços iguais. Três iguais seriam um menu hambúrguer, que é o ícone
/// de "abrir menu" em metade dos aplicativos do mundo.
/// Os três degraus: `(centro Y, meia-altura, X inicial, X final, cor)`, em FRAÇÃO do lado.
///
/// **Em ESCADA, e não empilhados**: empilhados seriam a tabela do database com outra cor. O
/// deslocamento lateral é o que se enxerga em 16px depois que a cor vira cinza.
const DEGRAUS: [(f32, f32, f32, f32, [f32; 3]); 3] = [
    (0.310, 0.062, 0.220, 0.560, ACCENT_HI), // o de cima, mais claro e recuado à esquerda
    (0.500, 0.062, 0.330, 0.670, ACCENT),    // o do meio
    (0.690, 0.062, 0.440, 0.780, ACCENT),    // o de baixo, avançado à direita
];

/// Raio das pontas arredondadas dos degraus, em fração do lado.
const DEGRAU_R: f32 = 0.030;

/// **O quê:** o ícone em RGBA no tamanho `n` (px). Devolve `(bytes, w, h)`. PURO e determinístico.
///
/// **Onde:** [`write_png`], e os testes — que podem afirmar pixels sem tocar no disco.
pub fn rgba(n: u32) -> (Vec<u8>, u32, u32) {
    let nf = n as f32;
    let radius = nf * 0.227; // rx=232/1024, o mesmo squircle da casa
    let mut buf = vec![0u8; (n * n * 4) as usize];
    let inv_ss2 = 1.0 / (SS * SS) as f32;

    for y in 0..n {
        for x in 0..n {
            let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let px = x as f32 + (sx as f32 + 0.5) / SS as f32;
                    let py = y as f32 + (sy as f32 + 0.5) / SS as f32;
                    if let Some(c) = sample(px, py, nf, radius) {
                        r += c[0];
                        g += c[1];
                        b += c[2];
                        a += 255.0;
                    }
                }
            }
            let idx = ((y * n + x) * 4) as usize;
            let cover = a * inv_ss2; // 0..255 — o alpha final é a cobertura
            if cover > 0.0 {
                // Média só sobre os subpixels COBERTOS: dividir pelo total criaria um halo
                // escuro na borda, porque os vazios entrariam como preto.
                let covered = a / 255.0;
                buf[idx] = (r / covered).round().clamp(0.0, 255.0) as u8;
                buf[idx + 1] = (g / covered).round().clamp(0.0, 255.0) as u8;
                buf[idx + 2] = (b / covered).round().clamp(0.0, 255.0) as u8;
                buf[idx + 3] = cover.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    (buf, n, n)
}

/// **O quê:** a cor OPACA de um subpixel, ou `None` fora do squircle (transparente).
///
/// **Onde:** [`rgba`]. Ordem de pintura: fundo (gradiente) → faixas.
fn sample(px: f32, py: f32, nf: f32, radius: f32) -> Option<[f32; 3]> {
    if !inside_rounded(px, py, nf, radius) {
        return None;
    }
    let t = ((px + py) / (2.0 * nf)).clamp(0.0, 1.0);
    let mut color = [
        BG_TL[0] + (BG_BR[0] - BG_TL[0]) * t,
        BG_TL[1] + (BG_BR[1] - BG_TL[1]) * t,
        BG_TL[2] + (BG_BR[2] - BG_TL[2]) * t,
    ];
    for (cy, meia_h, x0, x1, c) in DEGRAUS {
        if dentro_do_degrau(px, py, nf, cy, meia_h, x0, x1) {
            color = c;
            break;
        }
    }
    Some(color)
}

/// **O quê:** o ponto cai num degrau de cantos arredondados, entre `x0` e `x1`?
///
/// **Onde:** [`sample`]. Separada porque é a geometria da forma — e é o que um teste afirma
/// sem ter de ler pixel.
fn dentro_do_degrau(px: f32, py: f32, nf: f32, cy: f32, meia_h: f32, x0: f32, x1: f32) -> bool {
    let (fx, fy) = (px / nf, py / nf);
    let (cx, hw) = ((x0 + x1) / 2.0, (x1 - x0) / 2.0);
    let (x, y) = (fx - cx, fy - cy);
    let r = DEGRAU_R.min(meia_h).min(hw);
    // Retângulo arredondado: encolhe pelo raio, mede a distância ao retângulo interno.
    let dx = (x.abs() - (hw - r)).max(0.0);
    let dy = (y.abs() - (meia_h - r)).max(0.0);
    x.abs() <= hw && y.abs() <= meia_h && dx * dx + dy * dy <= r * r
}

/// **O quê:** ponto dentro de um quadrado `0..n` com cantos de raio `r`.
///
/// **Onde:** [`sample`], como recorte do squircle.
fn inside_rounded(px: f32, py: f32, n: f32, r: f32) -> bool {
    if px < 0.0 || py < 0.0 || px > n || py > n {
        return false;
    }
    let cx = px.clamp(r, n - r);
    let cy = py.clamp(r, n - r);
    let dx = px - cx;
    let dy = py - cy;
    dx * dx + dy * dy <= r * r
}

/// **O quê:** escreve o ícone (tamanho `n`) como PNG em `path`, criando os diretórios-pai.
///
/// **Onde:** [`write_hicolor`] e [`install_all`].
pub fn write_png(path: &std::path::Path, n: u32) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let (data, w, h) = rgba(n);
    let file = std::fs::File::create(path)?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(std::io::Error::other)?;
    writer.write_image_data(&data).map_err(std::io::Error::other)?;
    Ok(())
}

/// Nome do arquivo de ícone. Um lugar só: se divergir do `Icon=` do `.desktop`, o menu mostra
/// um quadrado cinza e ninguém liga a causa ao nome.
pub const NOME: &str = "schematize-overdev";

/// Tamanhos hicolor padrão (freedesktop).
pub const HICOLOR_SIZES: [u32; 8] = [16, 24, 32, 48, 64, 128, 256, 512];

/// **O quê:** gera a árvore hicolor completa em `base`. Devolve os caminhos escritos.
///
/// **Onde:** [`install_all`].
pub fn write_hicolor(base: &std::path::Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut out = Vec::new();
    for n in HICOLOR_SIZES {
        let p = base.join(format!("{n}x{n}")).join("apps").join(format!("{NOME}.png"));
        write_png(&p, n)?;
        out.push(p);
    }
    Ok(out)
}

/// **O quê:** instala o ícone em todos os locais que os ambientes consultam, e devolve o
/// caminho ABSOLUTO do 256px — o que vai no `Icon=`.
///
/// **Onde:** [`super::desktop::instalar_com_gui`].
///
/// **Absoluto no `Icon=` de propósito:** no Wayland o dock casa a janela ao `.desktop`, e um
/// nome de tema depende de cache de ícones que pode estar velho ou quebrado. Os extras são
/// best-effort — o hicolor é o que importa.
pub fn install_all(home: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    let icons = home.join(".local/share/icons/hicolor");
    write_hicolor(&icons)?;
    let p256 = icons.join("256x256").join("apps").join(format!("{NOME}.png"));
    for extra in [
        home.join(format!(".local/share/pixmaps/{NOME}.png")),
        home.join(format!(".icons/{NOME}.png")),
        home.join(format!(".local/share/icons/{NOME}.png")),
    ] {
        let _ = write_png(&extra, 256);
    }
    Ok(p256)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cor(buf: &[u8], n: u32, fx: f32, fy: f32) -> [u8; 3] {
        let (x, y) = ((fx * n as f32) as u32, (fy * n as f32) as u32);
        let i = ((y * n + x) * 4) as usize;
        [buf[i], buf[i + 1], buf[i + 2]]
    }

    /// O canto é TRANSPARENTE e o centro é OPACO — é o squircle existindo.
    #[test]
    fn o_squircle_recorta_os_cantos() {
        let (buf, n, _) = rgba(64);
        let alpha = |x: u32, y: u32| buf[((y * n + x) * 4 + 3) as usize];
        assert_eq!(alpha(0, 0), 0, "o canto tem de ser transparente");
        assert_eq!(alpha(63, 63), 0);
        assert_eq!(alpha(32, 32), 255, "o centro tem de ser opaco");
    }

    /// **É uma ESCADA, e não uma pilha.** Empilhados, os três degraus seriam a tabela do
    /// database com outra cor — e a distinção passaria a depender só do matiz, que é o que se
    /// perde primeiro em 16px, em tema claro e para quem não distingue cor.
    #[test]
    fn os_degraus_sao_deslocados_e_nao_empilhados() {
        // Cada degrau começa e termina mais à direita que o anterior.
        for par in DEGRAUS.windows(2) {
            let (a, b) = (par[0], par[1]);
            assert!(b.2 > a.2, "o início não avançou: {} -> {}", a.2, b.2);
            assert!(b.3 > a.3, "o fim não avançou: {} -> {}", a.3, b.3);
        }
        let n = 512u32;
        let (buf, _, _) = rgba(n);
        let acento = [ACCENT[0] as u8, ACCENT[1] as u8, ACCENT[2] as u8];
        let claro = [ACCENT_HI[0] as u8, ACCENT_HI[1] as u8, ACCENT_HI[2] as u8];

        // O canto superior-ESQUERDO do primeiro degrau é pintado; a mesma altura no lado
        // direito, não — é isso que faz a escada.
        assert_eq!(cor(&buf, n, 0.26, DEGRAUS[0].0), claro);
        assert_ne!(cor(&buf, n, 0.74, DEGRAUS[0].0), claro, "o topo não pode chegar à direita");
        // E o inverso embaixo.
        assert_eq!(cor(&buf, n, 0.74, DEGRAUS[2].0), acento);
        assert_ne!(cor(&buf, n, 0.26, DEGRAUS[2].0), acento, "a base não pode chegar à esquerda");
    }

    /// **O degrau de cima é o mais CLARO.** É o que dá profundidade à escada; três iguais
    /// leriam como três traços soltos.
    #[test]
    fn o_degrau_de_cima_e_o_destaque() {
        let n = 512u32;
        let (buf, _, _) = rgba(n);
        let cima = cor(&buf, n, 0.39, DEGRAUS[0].0);
        let meio = cor(&buf, n, 0.50, DEGRAUS[1].0);
        assert_ne!(cima, meio);
        assert!(cima[0] > meio[0] && cima[1] > meio[1], "{cima:?} vs {meio:?}");
    }

    /// **Entre os degraus há FUNDO.** Sem o vão, a escada vira um paralelogramo sólido.
    #[test]
    fn ha_vao_entre_os_degraus() {
        let n = 512u32;
        let (buf, _, _) = rgba(n);
        let acento = [ACCENT[0] as u8, ACCENT[1] as u8, ACCENT[2] as u8];
        let claro = [ACCENT_HI[0] as u8, ACCENT_HI[1] as u8, ACCENT_HI[2] as u8];
        let meio_y = (DEGRAUS[0].0 + DEGRAUS[1].0) / 2.0;
        let p = cor(&buf, n, 0.50, meio_y);
        assert_ne!(p, acento);
        assert_ne!(p, claro);
    }

    /// Todo tamanho hicolor sai, e sai com o número de bytes certo.
    #[test]
    fn sai_em_todo_tamanho_do_hicolor() {
        for n in HICOLOR_SIZES {
            let (buf, w, h) = rgba(n);
            assert_eq!((w, h), (n, n));
            assert_eq!(buf.len(), (n * n * 4) as usize, "tamanho {n}");
        }
    }
}
