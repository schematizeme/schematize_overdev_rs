//! A CARGA: tudo que a janela lê do binário para um projeto, fora do laço da tela.
//!
//! **O quê:** [`carregar`] roda os `--json` pedidos (em paralelo, uma thread por leitura) e devolve
//! a [`Carga`] — um `Result` por documento, SEM aplicar nada. Quem aplica é o `App`.
//!
//! **Onde:** [`super::update`], dentro de uma `Task` (a leitura bloqueia no subprocesso, então
//! nunca no `update`/`view`). No Slint as leituras rodavam NA thread da tela; aqui o relógio de
//! 3 s do «Acompanhar» e o «Reler» não congelam a janela.
//!
//! ## Por que a carga carrega o `dir`
//!
//! Troca de projeto no meio de uma leitura: o resultado do projeto ANTIGO chega depois da troca.
//! O `App` descarta a carga cujo `dir` não é o atual — senão a tela mostraria o checklist de um
//! projeto sob o caminho de outro.
//!
//! ## Por que um escopo
//!
//! O «Acompanhar» relê só progresso+monitor a cada 3 s (baratos); a caixa relê só a caixa; etc.
//! O escopo diz o que ler. Cada documento é lido e PARSEADO isoladamente: a falha de um não apaga
//! os outros (cada erro vai para o lugar dele).

use super::home::{self, DadosHome};
use super::{cli, leituras, telas};
use std::path::{Path, PathBuf};

/// Quais documentos ler. Máscara de bits, com os nomes nas constantes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Escopo(u16);

impl Escopo {
    pub const PROGRESSO: Escopo = Escopo(1);
    pub const CHECKLIST: Escopo = Escopo(1 << 1);
    pub const FILA: Escopo = Escopo(1 << 2);
    pub const HISTORICO: Escopo = Escopo(1 << 3);
    pub const SISTEMA: Escopo = Escopo(1 << 4);
    pub const MONITOR: Escopo = Escopo(1 << 5);
    pub const CAIXA: Escopo = Escopo(1 << 6);
    pub const ARQUIVOS: Escopo = Escopo(1 << 7);
    pub const NOTAS: Escopo = Escopo(1 << 8);
    /// Tudo: a abertura, o «Reler» e a troca de projeto.
    pub const HOME: Escopo = Escopo(1 << 9);
    pub const TUDO: Escopo = Escopo(0x3ff);
    /// O ciclo do «Acompanhar»: o que é barato.
    pub const TICK: Escopo = Escopo(1 | (1 << 5));
    /// Depois de anexar uma nota ou capturar: só a aba Notas / só a Caixa.
    pub const DE_NOTAS: Escopo = Escopo((1 << 7) | (1 << 8));

    /// **O quê:** este escopo inclui `outro`? **Onde:** [`carregar`] e o `App`, ao aplicar.
    pub fn tem(self, outro: Escopo) -> bool {
        self.0 & outro.0 == outro.0
    }
}

/// O resultado de uma leitura: o documento, ou o MOTIVO de não tê-lo (nunca "vazio calado").
pub type Leitura<T> = Option<Result<T, String>>;

/// O que o binário disse de um projeto, num instante.
#[derive(Debug, Clone)]
pub struct Carga {
    pub dir: PathBuf,
    pub escopo: Escopo,
    pub progresso: Leitura<telas::Progresso>,
    pub checklist: Leitura<telas::Checklist>,
    pub fila: Leitura<Vec<telas::Card>>,
    pub historico: Leitura<Vec<telas::Snapshot>>,
    pub sistema: Leitura<leituras::Sistema>,
    pub monitor: Leitura<leituras::Monitor>,
    pub caixa: Leitura<leituras::Caixa>,
    pub arquivos: Leitura<Vec<leituras::Arquivo>>,
    pub notas: Leitura<String>,
    /// A aba Projeto: o que `schematize-skills` e `schematize-git` disseram.
    pub home: Option<DadosHome>,
}

impl Carga {
    /// **O quê:** uma carga SEM nenhuma leitura. **Onde:** o ponto de partida de [`carregar`] e
    /// dos testes, que preenchem só o que querem afirmar.
    pub fn vazia(dir: PathBuf, escopo: Escopo) -> Carga {
        Carga {
            dir,
            escopo,
            progresso: None,
            checklist: None,
            fila: None,
            historico: None,
            sistema: None,
            monitor: None,
            caixa: None,
            arquivos: None,
            notas: None,
            home: None,
        }
    }
}

/// **O quê:** roda `<bin> <args>` em `dir` e passa o stdout pelo `parser`. **Onde:** [`carregar`].
fn ler<T>(dir: &Path, args: &[&str], parser: fn(&str) -> Result<T, String>) -> Result<T, String> {
    cli::rodar(dir, args).and_then(|t| parser(&t))
}

/// **O quê:** `Some(leitura)` se `escopo` pede `parte`; `None` se não. **Onde:** [`carregar`].
fn se<T>(escopo: Escopo, parte: Escopo, f: impl FnOnce() -> Result<T, String>) -> Leitura<T> {
    escopo.tem(parte).then(f)
}

/// **O quê:** lê os documentos de `escopo` para `dir`, em paralelo. `k` é o K do `--split`.
///
/// **Onde:** a `Task` de [`super::update`]. BLOQUEIA até a mais lenta terminar — chamar só fora
/// da thread da tela. Uma thread que entra em pânico vira `Err` daquela leitura (o `join` falho
/// não derruba a janela).
pub fn carregar(dir: &Path, escopo: Escopo, k: i64) -> Carga {
    let k = k.max(2).to_string();
    let mut c = Carga::vazia(dir.to_path_buf(), escopo);
    std::thread::scope(|s| {
        let progresso = s.spawn(|| {
            se(escopo, Escopo::PROGRESSO, || {
                ler(dir, &["progresso", "--json"], telas::ler_progresso)
            })
        });
        let checklist = s.spawn(|| {
            se(escopo, Escopo::CHECKLIST, || {
                ler(dir, &["checklist", "--json"], telas::ler_checklist)
            })
        });
        let fila =
            s.spawn(|| se(escopo, Escopo::FILA, || ler(dir, &["fila", "--json"], telas::ler)));
        let historico = s.spawn(|| {
            se(escopo, Escopo::HISTORICO, || {
                ler(dir, &["history", "--json", "--limit", "50"], telas::ler_historico)
            })
        });
        let sistema = s.spawn(|| {
            se(escopo, Escopo::SISTEMA, || {
                ler(dir, &["agents", "--json", "--split", &k], leituras::ler_sistema)
            })
        });
        let monitor = s.spawn(|| {
            se(escopo, Escopo::MONITOR, || ler(dir, &["monitor", "--json"], leituras::ler_monitor))
        });
        let caixa = s.spawn(|| {
            se(escopo, Escopo::CAIXA, || {
                ler(dir, &["caixa", "list", "--json"], leituras::ler_caixa)
            })
        });
        let arquivos = s.spawn(|| {
            se(escopo, Escopo::ARQUIVOS, || {
                ler(dir, &["arquivos", "--json"], leituras::ler_arquivos)
            })
        });
        let notas = s.spawn(|| {
            se(escopo, Escopo::NOTAS, || ler(dir, &["notas", "--json"], leituras::ler_notas))
        });
        let home = s.spawn(|| escopo.tem(Escopo::HOME).then(|| home::ler_dados(dir)));
        c.home = home.join().unwrap_or_else(|_| {
            let e = || "a leitura falhou por dentro (pânico na thread de leitura)".to_string();
            Some(DadosHome { acoes: Err(e()), aplicadas: Err(e()), commits: Err(e()) })
        });
        c.progresso = juntar(progresso);
        c.checklist = juntar(checklist);
        c.fila = juntar(fila);
        c.historico = juntar(historico);
        c.sistema = juntar(sistema);
        c.monitor = juntar(monitor);
        c.caixa = juntar(caixa);
        c.arquivos = juntar(arquivos);
        c.notas = juntar(notas);
    });
    c
}

/// **O quê:** espera a thread e devolve a leitura; pânico da thread vira `Err` com o motivo.
///
/// **Onde:** [`carregar`]. Sem isto, um pânico num parser derrubaria a Task inteira e a tela
/// ficaria "carregando" para sempre — erro engolido, que a casa veta.
fn juntar<T>(h: std::thread::ScopedJoinHandle<'_, Leitura<T>>) -> Leitura<T> {
    match h.join() {
        Ok(l) => l,
        Err(_) => {
            Some(Err("a leitura falhou por dentro (pânico na thread de leitura)".to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Os escopos compostos contêm o que dizem, e só isso.
    #[test]
    fn o_escopo_diz_o_que_ler() {
        assert!(Escopo::TUDO.tem(Escopo::CAIXA) && Escopo::TUDO.tem(Escopo::NOTAS));
        assert!(Escopo::TICK.tem(Escopo::PROGRESSO) && Escopo::TICK.tem(Escopo::MONITOR));
        assert!(!Escopo::TICK.tem(Escopo::CHECKLIST), "o tick é barato: não relê o checklist");
        assert!(Escopo::DE_NOTAS.tem(Escopo::ARQUIVOS) && !Escopo::DE_NOTAS.tem(Escopo::CAIXA));
    }

    /// **Sem binário instalado a carga devolve ERRO por documento, nunca vazio calado.** Com o
    /// PATH sem o `schematize-overdev` ao lado, cada leitura pedida vira `Some(Err(..))`, e as
    /// não pedidas ficam `None`.
    #[test]
    fn leitura_que_falha_e_erro_e_a_nao_pedida_e_none() {
        let c = carregar(Path::new("/nao/existe/mesmo"), Escopo::PROGRESSO, 2);
        assert!(matches!(c.progresso, Some(Err(_))), "{:?}", c.progresso);
        assert!(c.checklist.is_none() && c.caixa.is_none() && c.monitor.is_none());
    }
}
