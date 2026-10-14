//! **ambiente** — a `Plataforma` e os `Hooks` do processo, instalados pelo `cli::run`/`gui::run`.
//!
//! **O quê:** guarda, pelo tempo de uma execução, as referências recebidas pelas portas de
//! entrada da lib (`cli::run`, `gui::run`) para que o código fundo (trava, supervisor, lançador,
//! GUI em threads) as alcance sem carregar `&Plataforma` por dezenas de assinaturas. As
//! funções que decidem algo recebem o seam por PARÂMETRO (`*_com`) para o teste injetar um
//! falso; só o fio fino de produção passa por aqui.
//!
//! **Onde:** [`instalar`] roda no começo da execução; [`plat`] e [`hooks`] são lidos pelos módulos de
//! domínio. Sem instalação (testes unitários da lib em Unix) cai na plataforma Unix real.
//!

use std::sync::OnceLock;

use schematize_plataforma::parasita::{self, Config, HOOKS_OVERDEV};
use schematize_plataforma::seams::Plataforma;

use crate::hooks::Hooks;

/// A instalação do processo: feita UMA vez por `cli::run`/`gui::run`, que cedem a posse.
static ATUAL: OnceLock<(Plataforma, Box<dyn Hooks>)> = OnceLock::new();

/// A configuração do parasita do market para este app (prefixo das mensagens e hooks a pular).
const PARASITA: Config<'static> = Config::new("schematize").com_hooks(HOOKS_OVERDEV);

/// **O quê:** instala `plat` e `hooks` como o ambiente do processo (posse cedida, uma vez só).
///
/// **Onde:** `cli::run` e `gui::run`, no começo da execução.
///
/// **Erro:** uma segunda instalação devolve `Err` (nunca panica): o ambiente já está em uso por
/// código que guardou `&'static` dele e não pode mudar no meio.
pub fn instalar(plat: Plataforma, hooks: Box<dyn Hooks>) -> Result<(), String> {
    ATUAL
        .set((plat, hooks))
        .map_err(|_| "a Plataforma já foi instalada neste processo (run chamado duas vezes)".into())
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
/// **Panica** só em plataforma não-Unix sem `cli::run`/`gui::run` ter instalado: ali o host DEVE injetar
/// a dele (violação de contrato do host, não condição de execução).
pub fn plat() -> &'static Plataforma {
    if let Some((p, _)) = ATUAL.get() {
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
    if let Some((_, h)) = ATUAL.get() {
        return h.as_ref();
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
