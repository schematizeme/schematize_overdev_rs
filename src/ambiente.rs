//! **ambiente** — a `Plataforma` e os `Hooks` do processo, instalados pelo `cli::run`/`gui::run`.
//!
//! **O quê:** guarda, pelo tempo de uma execução, as referências recebidas pelas portas de
//! entrada da lib (`cli::run`, `gui::run`) para que o código fundo (trava, supervisor, lançador,
//! GUI em threads) as alcance sem carregar `&Plataforma` por dezenas de assinaturas. As
//! funções que decidem algo recebem o seam por PARÂMETRO (`*_com`) para o teste injetar um
//! falso; só o fio fino de produção passa por aqui.
//!
//! **Onde:** [`com`] envolve a execução inteira; [`plat`] e [`hooks`] são lidos pelos módulos de
//! domínio. Sem instalação (testes unitários da lib em Unix) cai na plataforma Unix real.
//!
//! **Por que um escopo e não uma `static` permanente:** o host entrega `&Plataforma` com tempo de
//! vida curto (vive na `main`); [`com`] estende o empréstimo SÓ enquanto a execução roda e o
//! restaura ao sair (ver o `SAFETY` em [`com`]).

use std::sync::RwLock;

use schematize_plataforma::parasita::{self, Config, HOOKS_OVERDEV};
use schematize_plataforma::seams::Plataforma;

use crate::hooks::Hooks;

/// Referências instaladas (estendidas a `'static` apenas durante [`com`]).
type Instalado = (&'static Plataforma, &'static dyn Hooks);

/// A instalação corrente, ou `None` fora de [`com`].
static ATUAL: RwLock<Option<Instalado>> = RwLock::new(None);

/// A configuração do parasita do market para este app (prefixo das mensagens e hooks a pular).
const PARASITA: Config<'static> = Config::new("schematize").com_hooks(HOOKS_OVERDEV);

/// **O quê:** lê a instalação corrente (um `RwLock` envenenado ainda guarda um valor válido).
///
/// **Onde:** [`plat`] e [`hooks`].
fn atual() -> Option<Instalado> {
    *ATUAL.read().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// **O quê:** roda `f` com `plat` e `hooks` instalados como o ambiente do processo e restaura o
/// anterior ao sair (aninhável).
///
/// **Onde:** `cli::run` e `gui::run`, envolvendo a execução inteira; os testes de integração em
/// processo também.
///
/// **Efeitos:** muda o estado global durante `f`.
pub fn com<R>(plat: &Plataforma, hooks: &dyn Hooks, f: impl FnOnce() -> R) -> R {
    /// Restaura o ambiente anterior ao sair de [`com`], inclusive em `panic`.
    struct Restaura(Option<Instalado>);
    impl Drop for Restaura {
        /// **O quê:** devolve o ambiente que existia antes. **Onde:** fim de [`com`].
        fn drop(&mut self) {
            *ATUAL.write().unwrap_or_else(std::sync::PoisonError::into_inner) = self.0;
        }
    }
    // SAFETY: as referências só são lidas enquanto `f` roda e [`Restaura`] limpa a instalação ao
    // sair de `com` (mesmo com `panic`); `plat`/`hooks` vivem, no mínimo, até `com` retornar. O
    // contrato do host é não deixar thread alguma usar o ambiente depois de `run` retornar (a
    // GUI bloqueia no loop de eventos; o CLI é síncrono).
    let (p, h) = unsafe {
        (
            std::mem::transmute::<&Plataforma, &'static Plataforma>(plat),
            std::mem::transmute::<&dyn Hooks, &'static dyn Hooks>(hooks),
        )
    };
    let anterior = ATUAL.write().unwrap_or_else(std::sync::PoisonError::into_inner).replace((p, h));
    let _restaura = Restaura(anterior);
    f()
}

/// Plataforma Unix real usada quando nada foi instalado (testes unitários da lib).
#[cfg(unix)]
fn padrao() -> &'static (Plataforma, crate::hooks::HooksUnix) {
    static P: std::sync::OnceLock<(Plataforma, crate::hooks::HooksUnix)> =
        std::sync::OnceLock::new();
    P.get_or_init(|| (Plataforma::unix(), crate::hooks::HooksUnix))
}

/// **O quê:** a `Plataforma` do processo.
///
/// **Onde:** trava, supervisor, agentes, lançador, desktop e a GUI.
///
/// **Panica** só em plataforma não-Unix sem `cli::run`/`gui::run` ativos: ali o host DEVE injetar
/// a dele (violação de contrato do host, não condição de execução).
pub fn plat() -> &'static Plataforma {
    if let Some((p, _)) = atual() {
        return p;
    }
    #[cfg(unix)]
    {
        &padrao().0
    }
    #[cfg(not(unix))]
    {
        panic!("overdev_casa: Plataforma não instalada — chame cli::run/gui::run")
    }
}

/// **O quê:** os `Hooks` do processo.
///
/// **Onde:** `settings` (enable/refresh/diagnóstico) e o gate do Stop hook.
///
/// **Panica** nas mesmas condições de [`plat`].
pub fn hooks() -> &'static dyn Hooks {
    if let Some((_, h)) = atual() {
        return h;
    }
    #[cfg(unix)]
    {
        &padrao().1
    }
    #[cfg(not(unix))]
    {
        panic!("overdev_casa: Hooks não instalados — chame cli::run/gui::run")
    }
}

/// **O quê:** garante o gestor de atualizações (market) em `~/.cargo/bin` — o "parasita", agora
/// da lib compartilhada.
///
/// **Onde:** `cli::run` (depois do parse) e `gui::run`. Nunca derruba o app; os hooks
/// `guard`/`check` ficam de fora (regra do `PARASITA`).
pub fn garantir_market() {
    parasita::garantir(&PARASITA);
}
