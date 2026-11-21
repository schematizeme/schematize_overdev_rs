//! Envelope dos arquivos do handoff vivo: cabeçalho versionado + corpo + rodapé de fechamento.
//!
//! **O quê:** [`codificar`] embrulha um corpo com versão de esquema, `seq`, tamanho e checksum
//! FNV-1a 64; [`decodificar`] só aceita bytes que sejam EXATAMENTE o que [`codificar`] produziria
//! (comparação byte a byte), então truncamento, bit-flip em qualquer byte, mistura de duas
//! gravações ou lixo no fim são sempre rejeitados.
//! **Onde:** [`super::atomico`] grava e lê todo arquivo do handoff vivo por aqui.
//!
//! Layout (texto, legível em Markdown):
//! ```text
//! <!-- schematize-handoff-vivo esquema=1 seq=7 len=123 fnv1a64=00ff... -->
//! <corpo, len bytes>
//! <!-- fim seq=7 fnv1a64=00ff... -->
//! ```

/// Versão do esquema do envelope.
pub const ESQUEMA: u32 = 1;

const PREFIXO: &str = "<!-- schematize-handoff-vivo esquema=1 seq=";

/// Corpo decodificado e validado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decodificado {
    /// Número de sequência monotônico (independe do relógio).
    pub seq: u64,
    /// Corpo íntegro.
    pub corpo: Vec<u8>,
}

/// **O quê:** FNV-1a 64 do corpo. **Onde:** [`codificar`]/[`decodificar`]. PURA.
pub fn checksum(corpo: &[u8]) -> u64 {
    crate::otimiza::fnv64(corpo)
}

/// **O quê:** monta cabeçalho + corpo + rodapé. **Onde:** toda gravação do handoff vivo. PURA.
pub fn codificar(seq: u64, corpo: &[u8]) -> Vec<u8> {
    let h = checksum(corpo);
    let cab = format!("{PREFIXO}{seq} len={} fnv1a64={h:016x} -->\n", corpo.len());
    let rod = format!("\n<!-- fim seq={seq} fnv1a64={h:016x} -->\n");
    let mut v = Vec::with_capacity(cab.len() + corpo.len() + rod.len());
    v.extend_from_slice(cab.as_bytes());
    v.extend_from_slice(corpo);
    v.extend_from_slice(rod.as_bytes());
    v
}

/// **O quê:** lê `seq` e `len` do cabeçalho (primeira linha). **Onde:** [`decodificar`]. PURA.
fn ler_cabecalho(bytes: &[u8]) -> Result<(u64, usize, usize), String> {
    let fim = bytes.iter().position(|b| *b == b'\n').ok_or("sem fim de cabeçalho")?;
    let linha = std::str::from_utf8(&bytes[..fim]).map_err(|_| "cabeçalho não é UTF-8")?;
    let resto = linha.strip_prefix(PREFIXO).ok_or("prefixo do cabeçalho inválido")?;
    let (seq, resto) = resto.split_once(" len=").ok_or("cabeçalho sem len")?;
    let (len, _) = resto.split_once(" fnv1a64=").ok_or("cabeçalho sem checksum")?;
    let seq = seq.parse::<u64>().map_err(|_| "seq inválido")?;
    let len = len.parse::<usize>().map_err(|_| "len inválido")?;
    Ok((seq, len, fim + 1))
}

/// **O quê:** valida `bytes` e devolve o corpo; qualquer divergência vira `Err(motivo)`.
/// **Onde:** leitura de gerações, releitura pós-gravação, `verificar`. PURA.
pub fn decodificar(bytes: &[u8]) -> Result<Decodificado, String> {
    let (seq, len, ini) = ler_cabecalho(bytes)?;
    let fim =
        ini.checked_add(len).filter(|f| *f <= bytes.len()).ok_or("truncado: corpo incompleto")?;
    let corpo = &bytes[ini..fim];
    if codificar(seq, corpo) != bytes {
        return Err("checksum, rodapé ou bytes extras divergem (corrompido/truncado)".into());
    }
    Ok(Decodificado { seq, corpo: corpo.to_vec() })
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ida_e_volta() {
        let b = codificar(9, "olá\nmundo".as_bytes());
        let d = decodificar(&b).unwrap();
        assert_eq!((d.seq, d.corpo.as_slice()), (9, "olá\nmundo".as_bytes()));
    }

    #[test]
    fn vazio_e_truncamentos_e_bitflips_sao_rejeitados() {
        let b = codificar(3, b"corpo de teste\ncom linhas");
        assert!(decodificar(b"").is_err());
        for n in 0..b.len() {
            assert!(decodificar(&b[..n]).is_err(), "truncado em {n}");
        }
        for i in 0..b.len() {
            for bit in 0..8 {
                let mut c = b.clone();
                c[i] ^= 1 << bit;
                assert!(decodificar(&c).is_err(), "flip byte {i} bit {bit}");
            }
        }
        let mut extra = b.clone();
        extra.push(b'x');
        assert!(decodificar(&extra).is_err());
    }
}
