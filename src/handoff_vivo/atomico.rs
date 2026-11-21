//! Escrita atômica com 3 gerações e leitura da geração válida mais nova.
//!
//! **O quê:** [`escrever_atomico`] = tmp no MESMO diretório + `write_all` + `fsync(arquivo)` +
//! `rename` + `fsync(diretório)`. [`Armazem`] mantém `nome`, `nome.1`, `nome.2` (gerações 0..2):
//! grava a nova geração só depois de reler e validar o tmp, empurra a anterior (somente se
//! válida) para `.1`, relê o resultado e, se a releitura falhar, restaura a geração anterior.
//! A leitura ([`Armazem::melhor`]) escolhe a geração VÁLIDA de maior `seq`.
//! **Onde:** [`super::ciclo`] (context, checklist e estado), [`super::verificar`].
//! Implementação local (não depende da plataforma): técnica de `persistencia/fsutil.rs`.

use super::formato::{codificar, decodificar, Decodificado};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Quantas gerações o armazém guarda.
pub const GERACOES: usize = 3;

/// **O quê:** nome de temporário único (`.<alvo>.tmp-<pid>-<n>`) ao lado de `alvo`.
/// **Onde:** [`escrever_atomico`]. Efeito: incrementa um contador.
pub fn nome_tmp(alvo: &Path) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let nome = alvo.file_name().and_then(|s| s.to_str()).unwrap_or("alvo");
    alvo.with_file_name(format!(".{nome}.tmp-{}-{n}", std::process::id()))
}

/// **O quê:** `fsync` do diretório (torna durável o `rename`). **Onde:** fim de toda gravação.
pub fn sincronizar_dir(dir: &Path) -> Result<(), String> {
    let dir = if dir.as_os_str().is_empty() { Path::new(".") } else { dir };
    let f = File::open(dir).map_err(|e| format!("abrir dir {}: {e}", dir.display()))?;
    f.sync_all().map_err(|e| format!("fsync do dir {}: {e}", dir.display()))
}

/// **O quê:** cria `tmp`, `write_all`, `sync_all`. **Onde:** [`escrever_atomico`].
fn gravar_tmp(tmp: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut f = File::create(tmp).map_err(|e| format!("criar {}: {e}", tmp.display()))?;
    f.write_all(bytes).map_err(|e| format!("gravar {}: {e}", tmp.display()))?;
    f.sync_all().map_err(|e| format!("fsync {}: {e}", tmp.display()))
}

/// **O quê:** remove `tmp` sem engolir o motivo: se a remoção falhar, o erro original ganha o
/// aviso. **Onde:** caminhos de falha de [`escrever_atomico`] e [`Armazem::gravar_seq`].
fn remover_tmp(tmp: &Path, erro: String) -> String {
    match std::fs::remove_file(tmp) {
        Ok(()) => erro,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => erro,
        Err(e) => format!("{erro}; e não removi {}: {e}", tmp.display()),
    }
}

/// **O quê:** escrita atômica de `bytes` em `alvo` (tmp + fsync + rename + fsync do dir).
/// Falha remove o tmp e deixa `alvo` como estava. **Onde:** [`Armazem`] e arquivos datados.
pub fn escrever_atomico(alvo: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = nome_tmp(alvo);
    let r = gravar_tmp(&tmp, bytes).and_then(|()| {
        std::fs::rename(&tmp, alvo).map_err(|e| format!("renomear para {}: {e}", alvo.display()))
    });
    if let Err(e) = r {
        return Err(remover_tmp(&tmp, e));
    }
    sincronizar_dir(alvo.parent().unwrap_or(Path::new(".")))
}

/// **O quê:** lê `caminho` e valida o envelope. **Onde:** [`Armazem::geracoes`], releituras.
pub fn ler_validado(caminho: &Path) -> Result<Decodificado, String> {
    let b = std::fs::read(caminho).map_err(|e| format!("ler {}: {e}", caminho.display()))?;
    decodificar(&b)
}

/// **O quê:** temporários órfãos (`.x.tmp-<pid>-<n>`) de OUTROS pids em `dir`.
/// **Onde:** [`limpar_tmp`] e o relatório de `verificar`.
pub fn tmps_orfaos(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let meu = std::process::id().to_string();
    rd.flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            let Some((_, cauda)) = n.rsplit_once(".tmp-") else { return false };
            let pid = cauda.split('-').next().unwrap_or("");
            n.starts_with('.')
                && !pid.is_empty()
                && pid.bytes().all(|b| b.is_ascii_digit())
                && pid != meu
        })
        .map(|e| e.path())
        .collect()
}

/// **O quê:** apaga os temporários órfãos de [`tmps_orfaos`]; devolve quantos removeu.
/// **Onde:** abertura do motor (já com o lock), `verificar --reparar`.
pub fn limpar_tmp(dir: &Path) -> Result<usize, String> {
    let mut n = 0;
    for p in tmps_orfaos(dir) {
        std::fs::remove_file(&p).map_err(|e| format!("remover {}: {e}", p.display()))?;
        n += 1;
    }
    Ok(n)
}

/// Uma geração lida do disco.
#[derive(Debug)]
pub struct Geracao {
    /// 0 = mais nova (nome base), 1 e 2 = anteriores.
    pub indice: usize,
    /// Caminho do arquivo.
    pub caminho: PathBuf,
    /// Conteúdo validado, ou o motivo da invalidez/ausência.
    pub lida: Result<Decodificado, String>,
}

/// Armazém de um arquivo lógico com 3 gerações.
#[derive(Debug, Clone)]
pub struct Armazem {
    dir: PathBuf,
    nome: String,
}

impl Armazem {
    /// **O quê:** armazém de `nome` em `dir`. **Onde:** [`super::ciclo`], [`super::verificar`].
    pub fn novo(dir: &Path, nome: &str) -> Armazem {
        Armazem { dir: dir.to_path_buf(), nome: nome.to_string() }
    }

    /// **O quê:** caminho da geração `i` (`nome`, `nome.1`, `nome.2`). **Onde:** todo método.
    pub fn caminho(&self, i: usize) -> PathBuf {
        if i == 0 {
            self.dir.join(&self.nome)
        } else {
            self.dir.join(format!("{}.{i}", self.nome))
        }
    }

    /// **O quê:** lê e valida as 3 gerações. **Onde:** [`Self::melhor`], `verificar`, testes.
    pub fn geracoes(&self) -> Vec<Geracao> {
        (0..GERACOES)
            .map(|i| Geracao {
                indice: i,
                caminho: self.caminho(i),
                lida: ler_validado(&self.caminho(i)),
            })
            .collect()
    }

    /// **O quê:** a geração VÁLIDA de maior `seq` (empate: a de menor índice). **Onde:** leitura.
    pub fn melhor(&self) -> Option<(usize, Decodificado)> {
        let mut m: Option<(usize, Decodificado)> = None;
        for g in self.geracoes() {
            if let Ok(d) = g.lida {
                if m.as_ref().is_none_or(|(_, x)| d.seq > x.seq) {
                    m = Some((g.indice, d));
                }
            }
        }
        m
    }

    /// **O quê:** `seq` da próxima gravação (maior válido + 1). **Onde:** [`Self::gravar`].
    pub fn proxima_seq(&self) -> u64 {
        self.melhor().map_or(1, |(_, d)| d.seq.saturating_add(1))
    }

    /// **O quê:** grava `corpo` como nova geração com a próxima `seq`; devolve a `seq`.
    /// **Onde:** cada tick que mudou o conteúdo.
    pub fn gravar(&self, corpo: &[u8]) -> Result<u64, String> {
        let seq = self.proxima_seq();
        self.gravar_seq(seq, corpo)?;
        Ok(seq)
    }

    /// **O quê:** a gravação completa (ver doc do módulo). Em qualquer falha a geração anterior
    /// permanece como a mais nova válida. **Onde:** [`Self::gravar`].
    pub fn gravar_seq(&self, seq: u64, corpo: &[u8]) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| format!("criar {}: {e}", self.dir.display()))?;
        let bytes = codificar(seq, corpo);
        let alvo = self.caminho(0);
        let tmp = nome_tmp(&alvo);
        gravar_tmp(&tmp, &bytes).map_err(|e| remover_tmp(&tmp, e))?;
        let pronto = conferir(&tmp, seq).and_then(|()| self.rotacionar());
        if let Err(e) = pronto {
            return Err(remover_tmp(&tmp, e));
        }
        std::fs::rename(&tmp, &alvo).map_err(|e| remover_tmp(&tmp, format!("renomear: {e}")))?;
        sincronizar_dir(&self.dir)?;
        conferir(&alvo, seq).map_err(|e| self.restaurar(&e))
    }

    /// **O quê:** empurra a geração 0 (se válida) para `.1` e a `.1` (se válida) para `.2`, sempre
    /// por escrita atômica; a 0 segue no lugar até o `rename` da nova. **Onde:** [`Self::gravar_seq`].
    fn rotacionar(&self) -> Result<(), String> {
        let (c0, c1, c2) = (self.caminho(0), self.caminho(1), self.caminho(2));
        if ler_validado(&c0).is_err() {
            return Ok(());
        }
        if ler_validado(&c1).is_ok() {
            let b1 = std::fs::read(&c1).map_err(|e| format!("ler {}: {e}", c1.display()))?;
            escrever_atomico(&c2, &b1)?;
        }
        let b0 = std::fs::read(&c0).map_err(|e| format!("ler {}: {e}", c0.display()))?;
        escrever_atomico(&c1, &b0)
    }

    /// **O quê:** a releitura pós-`rename` falhou: devolve a geração 1 válida ao lugar da 0.
    /// Retorna a mensagem de erro (sempre `Err` para quem chama). **Onde:** [`Self::gravar_seq`].
    fn restaurar(&self, motivo: &str) -> String {
        let c1 = self.caminho(1);
        let r = ler_validado(&c1)
            .and_then(|_| std::fs::read(&c1).map_err(|e| e.to_string()))
            .and_then(|b| escrever_atomico(&self.caminho(0), &b));
        match r {
            Ok(()) => format!("releitura falhou ({motivo}); geração anterior restaurada"),
            Err(e) => format!("releitura falhou ({motivo}); restauração também falhou: {e}"),
        }
    }

    /// **O quê:** promove a melhor geração válida ao índice 0 (se a 0 estiver inválida/ausente).
    /// `Ok(true)` se reparou. **Onde:** `verificar --reparar` (com o lock).
    pub fn reparar(&self) -> Result<bool, String> {
        let Some((i, d)) = self.melhor() else { return Ok(false) };
        if i == 0 {
            return Ok(false);
        }
        let b = codificar(d.seq, &d.corpo);
        escrever_atomico(&self.caminho(0), &b)?;
        Ok(true)
    }
}

/// **O quê:** relê `caminho` e exige envelope válido com `seq` esperada. **Onde:** [`Armazem`].
fn conferir(caminho: &Path, seq: u64) -> Result<(), String> {
    let d = ler_validado(caminho)?;
    if d.seq == seq {
        Ok(())
    } else {
        Err(format!("seq lida {} != gravada {seq}", d.seq))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn dir(n: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hv-atomico-{n}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn tres_geracoes_rotacionam_e_a_mais_nova_vence() {
        let d = dir("rot");
        let a = Armazem::novo(&d, "U.md");
        for i in 1..=5u8 {
            assert_eq!(a.gravar(&[b'a' + i]).unwrap(), u64::from(i));
        }
        let g = a.geracoes();
        let seqs: Vec<u64> = g.iter().map(|x| x.lida.as_ref().unwrap().seq).collect();
        assert_eq!(seqs, vec![5, 4, 3]);
        assert_eq!(a.melhor().unwrap().1.seq, 5);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn geracao0_corrompida_cai_na_anterior_e_nao_e_empurrada() {
        let d = dir("corr");
        let a = Armazem::novo(&d, "U.md");
        a.gravar(b"um").unwrap();
        a.gravar(b"dois").unwrap();
        std::fs::write(a.caminho(0), b"lixo").unwrap();
        assert_eq!(a.melhor().unwrap().1.corpo, b"um");
        assert!(a.reparar().unwrap());
        assert!(ler_validado(&a.caminho(0)).is_ok());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn restaurar_devolve_a_anterior() {
        let d = dir("rest");
        let a = Armazem::novo(&d, "U.md");
        a.gravar(b"um").unwrap();
        a.gravar(b"dois").unwrap();
        std::fs::write(a.caminho(0), b"ruim").unwrap();
        let msg = a.restaurar("teste");
        assert!(msg.contains("restaurada"), "{msg}");
        assert_eq!(ler_validado(&a.caminho(0)).unwrap().corpo, b"um");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn tmp_orfao_de_outro_pid_e_limpo() {
        let d = dir("tmp");
        std::fs::write(d.join(".U.md.tmp-999999-0"), b"x").unwrap();
        std::fs::write(d.join(".U.md.tmp-1234abc-0"), b"x").unwrap();
        assert_eq!(limpar_tmp(&d).unwrap(), 1);
        assert!(d.join(".U.md.tmp-1234abc-0").exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
