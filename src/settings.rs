//! Registro dos hooks do overdev no settings.json do Claude Code.
//! O quê: liga/desliga o Stop hook (não parar) e o PreToolUse hook (vetar
//! AskUserQuestion) apontando pro próprio binário. Preserva o resto do settings.
//! Onde: chamado por `schematize overdev enable|disable`.

use crate::nucleo::caminhos::settings_path;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// Um grupo de hooks contém um comando com este trecho?
fn group_has(group: &Value, needle: &str) -> bool {
    group.get("hooks").and_then(|h| h.as_array()).is_some_and(|arr| {
        arr.iter()
            .any(|h| h.get("command").and_then(|c| c.as_str()).is_some_and(|c| c.contains(needle)))
    })
}

/// Lê o settings.json como objeto (ou objeto vazio se não existir/inválido).
fn load() -> Value {
    match fs::read_to_string(settings_path()) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|_| json!({})),
        Err(_) => json!({}),
    }
}

fn save(v: &Value) -> Result<(), String> {
    let p = settings_path();
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let body = serde_json::to_string_pretty(v).map_err(|e| e.to_string())?;
    fs::write(&p, body).map_err(|e| e.to_string())
}

/// Garante que `hooks[event]` (array) tenha o grupo `group` se ainda não houver `needle`.
fn ensure_group(root: &mut Value, event: &str, needle: &str, group: Value) {
    // Nada aqui pode ser `unwrap`: o settings.json é de QUEM USA, e um `"hooks": "x"`
    // (string onde esperamos objeto) fazia o CLI panicar em vez de tratar. Arquivo alheio
    // malformado é entrada hostil como qualquer outra — corrigimos o nó e seguimos.
    let Some(raiz) = root.as_object_mut() else { return };
    let hooks = raiz.entry("hooks").or_insert_with(|| json!({}));
    if !hooks.is_object() {
        *hooks = json!({});
    }
    let Some(hooks) = hooks.as_object_mut() else { return };
    let arr = hooks.entry(event).or_insert_with(|| json!([]));
    if !arr.is_array() {
        *arr = json!([]);
    }
    let Some(arr) = arr.as_array_mut() else { return };
    // REMOVE qualquer versão anterior do hook (mesmo needle) antes de adicionar — assim `enable`
    // ATUALIZA o comando em vez de só adicionar-se-ausente. Sem isto, um hook antigo com caminho
    // absoluto stale (ex.: `/usr/bin/schematize` de um pacote removido) nunca era trocado pelo
    // resolvedor resiliente ao re-rodar `overdev enable`.
    arr.retain(|g| !group_has(g, needle));
    arr.push(group);
}

/// A MARCA que identifica cada grupo de hook do overdev no `settings.json`.
///
/// **Não é o comando, e a diferença é o bug que a E6 expôs.** A mesma string fazia dois
/// trabalhos: identificar o grupo (busca por SUBSTRING) e ser o subcomando passado ao
/// `hook_cmd` (comparação EXATA). Quando o comando mudou de forma — para procurar o
/// `schematize-overdev` antes do hub —, a comparação passou a construir um alvo que ninguém
/// escrevia, e o `hooks_atualizados` respondia sobre uma string inventada.
///
/// A marca continua sendo `overdev check`/`overdev guard` de propósito: o comando NOVO também
/// contém esse texto (na lista de fallback, `exec "$c" overdev check`), então quem tem o hook
/// antigo e quem tem o novo são os dois reconhecidos. Reconhecer só o novo faria o `disable`
/// deixar o grupo antigo para trás — ligado e órfão.
const MARCAS: [(&str, &str, &str); 2] =
    [("Stop", "overdev check", "check"), ("PreToolUse", "overdev guard", "guard")];

/// Os hooks do overdev estão registrados no settings.json?
pub fn overdev_enabled() -> bool {
    load()
        .get("hooks")
        .and_then(|h| h.get("Stop"))
        .and_then(|a| a.as_array())
        .is_some_and(|arr| arr.iter().any(|g| group_has(g, MARCAS[0].1)))
}

/// O settings.json existe e é JSON válido? (None = não existe; Some(false) = inválido.)
pub fn settings_valid() -> Option<bool> {
    match fs::read_to_string(settings_path()) {
        Ok(s) => Some(serde_json::from_str::<Value>(&s).is_ok()),
        Err(_) => None,
    }
}

/// Comando de hook RESILIENTE: resolve o `schematize` em RUNTIME (o `exe` de agora como 1ª pista,
/// depois ~/.cargo/bin, /usr/local/bin, /usr/bin e o PATH) e roda `<sub>`. Se NÃO achar o binário,
/// sai 0 (não bloqueia, não erra) — antes gravava um caminho absoluto que virava stale (ex.: o
/// pacote saía e ficava `/usr/bin/schematize: not found` em todo Stop). "Prever macacos".
fn hook_cmd(_exe: &str, sub: &str) -> String {
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

/// Liga os dois hooks do overdev. O comando resolve o binário em runtime (resiliente a mudança de
/// caminho / pacote removido), com `exe` (o binário atual) como primeira pista.
pub fn enable(exe: &str) -> Result<(), String> {
    let mut root = load();
    aplica_hooks(&mut root, exe);
    save(&root)
}

/// **O quê:** grava (ou substitui) os grupos de hook do overdev em `root`, em memória.
///
/// **Onde:** `enable` (sobre o settings real) e os testes de reconhecimento/idempotência
/// (sobre um JSON de bancada, sem tocar o `~/.claude/settings.json`).
fn aplica_hooks(root: &mut Value, exe: &str) {
    if !root.is_object() {
        *root = json!({});
    }
    let stop_cmd = hook_cmd(exe, "check");
    let guard_cmd = hook_cmd(exe, "guard");
    ensure_group(
        root,
        "Stop",
        "overdev check",
        json!({ "hooks": [ { "type": "command", "command": stop_cmd } ] }),
    );
    ensure_group(
        root,
        "PreToolUse",
        "overdev guard",
        json!({ "matcher": "AskUserQuestion", "hooks": [ { "type": "command", "command": guard_cmd } ] }),
    );
}

/// Liga o hook do gestor de VPS: `PreToolUse` em TODAS as tools, barrando SSH cru e leitura
/// de chave privada (ver `vps::hook`).
///
/// **Onde:** `schematize vps hooks --on`. Separado do [`enable`] do overdev de propósito —
/// são disciplinas independentes: quem não usa overdev pode querer só a barreira de SSH, e
/// desligar uma não pode desligar a outra.
///
/// O matcher é `"*"` porque a checagem de chave privada precisa ver o input de qualquer tool
/// (um `Write` com a chave no conteúdo vaza igual a um `Bash`).
pub fn enable_vps(exe: &str) -> Result<(), String> {
    let mut root = load();
    if !root.is_object() {
        root = json!({});
    }
    let cmd = hook_cmd(exe, "vps guard");
    ensure_group(
        &mut root,
        "PreToolUse",
        "vps guard",
        json!({ "matcher": "*", "hooks": [ { "type": "command", "command": cmd } ] }),
    );
    save(&root)
}

/// Remove o hook do gestor de VPS, sem tocar nos do overdev nem em hook alheio.
///
/// **Onde:** `schematize vps hooks --off`.
pub fn disable_vps() -> Result<(), String> {
    let mut root = load();
    if let Some(arr) =
        root.get_mut("hooks").and_then(|h| h.get_mut("PreToolUse")).and_then(|a| a.as_array_mut())
    {
        arr.retain(|g| !group_has(g, "vps guard"));
    }
    save(&root)
}

/// Acrescenta nomes de tool ao `permissions.allow`, sem duplicar e sem remover nada.
/// Devolve quantos foram acrescentados.
///
/// **Onde:** `schematize mcp install`. Preserva o `allow` do usuário — a lista dele costuma
/// ter dezenas de entradas construídas ao longo de meses, e perder isso seria imperdoável.
pub fn permitir_tools(nomes: &[String]) -> Result<usize, String> {
    let mut root = load();
    if !root.is_object() {
        root = json!({});
    }
    let Some(raiz) = root.as_object_mut() else { return Ok(0) };
    let perms = raiz.entry("permissions").or_insert_with(|| json!({}));
    if !perms.is_object() {
        *perms = json!({});
    }
    let Some(perms) = perms.as_object_mut() else { return Ok(0) };
    let allow = perms.entry("allow").or_insert_with(|| json!([]));
    if !allow.is_array() {
        *allow = json!([]);
    }
    let Some(arr) = allow.as_array_mut() else { return Ok(0) };
    let mut n = 0;
    for nome in nomes {
        if !arr.iter().any(|v| v.as_str() == Some(nome.as_str())) {
            arr.push(json!(nome));
            n += 1;
        }
    }
    save(&root)?;
    Ok(n)
}

/// Remove nomes de tool do `permissions.allow`. Devolve quantos saíram.
pub fn remover_tools(nomes: &[String]) -> Result<usize, String> {
    let mut root = load();
    let Some(arr) =
        root.get_mut("permissions").and_then(|p| p.get_mut("allow")).and_then(|a| a.as_array_mut())
    else {
        return Ok(0);
    };
    let antes = arr.len();
    arr.retain(|v| !v.as_str().is_some_and(|s| nomes.iter().any(|n| n == s)));
    let n = antes - arr.len();
    save(&root)?;
    Ok(n)
}

/// Quantos dos `nomes` já estão em `permissions.allow`.
pub fn tools_permitidas(nomes: &[String]) -> usize {
    let root = load();
    let Some(arr) = root.get("permissions").and_then(|p| p.get("allow")).and_then(|a| a.as_array())
    else {
        return 0;
    };
    nomes.iter().filter(|n| arr.iter().any(|v| v.as_str() == Some(n.as_str()))).count()
}

/// O hook do gestor de VPS está registrado?
///
/// **Onde:** `schematize vps hooks` (sem flag, mostra o estado) e o `doctor`.
pub fn vps_hook_enabled() -> bool {
    load()
        .get("hooks")
        .and_then(|h| h.get("PreToolUse"))
        .and_then(|a| a.as_array())
        .is_some_and(|arr| arr.iter().any(|g| group_has(g, "vps guard")))
}

/// Os hooks registrados batem com o comando que ESTA versão gravaria?
///
/// `false` quando o `settings.json` guarda um comando de uma versão anterior — o caso
/// real: um hook com caminho absoluto (`/usr/bin/schematize`, do pacote .deb) que o
/// install do fonte removeu, deixando `not found` em todo Stop. O `enable` já grava o
/// resolvedor resiliente, mas ninguém re-roda `enable` ao atualizar o app: o comando
/// velho fica lá pra sempre.
pub fn hooks_atualizados(exe: &str) -> bool {
    hooks_atualizados_em(&load(), exe)
}

/// **O quê:** `hooks_atualizados` sobre um JSON dado (comparação EXATA com o comando atual).
///
/// **Onde:** `hooks_atualizados` e os testes.
fn hooks_atualizados_em(root: &Value, exe: &str) -> bool {
    MARCAS.iter().all(|(event, _marca, sub)| {
        let alvo = hook_cmd(exe, sub);
        root.get("hooks").and_then(|h| h.get(event)).and_then(|a| a.as_array()).is_some_and(|arr| {
            arr.iter().any(|g| {
                g.get("hooks").and_then(|h| h.as_array()).is_some_and(|hs| {
                    hs.iter()
                        .any(|h| h.get("command").and_then(|c| c.as_str()) == Some(alvo.as_str()))
                })
            })
        })
    })
}

/// Regrava os hooks com o comando desta versão — SÓ se o overdev já estiver ligado.
///
/// É a auto-cura: quem ligou o overdev numa versão antiga carrega o comando daquela
/// versão no `settings.json`, e atualizar o app não mexia nisso. Chamado de onde roda
/// com frequência (o agente) e do `doctor`. Devolve `true` se regravou.
/// Não liga hook em quem não pediu: overdev desligado → não faz nada.
pub fn refresh_hooks(exe: &str) -> Result<bool, String> {
    if !overdev_enabled() || hooks_atualizados(exe) {
        return Ok(false);
    }
    enable(exe)?;
    Ok(true)
}

// ---------------------------------------------------------------------------
// REPARO de hook quebrado — a classe inteira, não um caso.
//
// Três relatos, três formas diferentes do mesmo estrago:
//   `/usr/bin/schematize: not found`        (caminho do pacote .deb, removido)
//   `.claude/hooks/overdev-stop.sh: não existe` (caminho RELATIVO, do hook shell
//                                                legado, só válido numa pasta)
// O comando do hook mora no settings.json de quem LIGOU o overdev, e nada nunca
// revisitava aquilo. Em vez de tratar caso a caso: se o comando aponta pra um
// caminho que NÃO EXISTE, ele está quebrado — regrava com o resolvedor atual.
//
// O que NÃO é mexido: hook cujo caminho existe. Quem usa de propósito o fallback
// shell da skill (`~/.claude/skills/.../overdev-stop.sh`) continua com o dele.
// ---------------------------------------------------------------------------

/// Os caminhos de arquivo citados num comando de hook (tokens com `/`, `~` expandido).
fn caminhos_citados(cmd: &str) -> Vec<PathBuf> {
    cmd.split_whitespace()
        .map(|t| t.trim_matches(['"', '\'', ';']))
        .filter(|t| t.contains('/') && !t.starts_with('-'))
        .map(|t| match t.strip_prefix("~/") {
            Some(resto) => crate::nucleo::util::home().join(resto),
            None => PathBuf::from(t),
        })
        .collect()
}

/// O comando do hook está QUEBRADO — inexistente OU dependente do cwd?
///
/// Duas formas de quebra, e a segunda é a que enganou a versão anterior desta função:
///
/// 1. **Nenhum caminho citado existe.** O nosso comando resiliente cita quatro de
///    propósito (o ponto dele é tentar todos), então basta UM existir pra passar.
/// 2. **Algum caminho é RELATIVO** (`bash .claude/hooks/overdev-stop.sh`). Isso é
///    quebrado *por construção*, exista o arquivo ou não: quem executa o hook é o
///    Claude Code, e o cwd dele não é garantido ser a raiz do projeto — em qualquer
///    outra pasta vira "arquivo ou diretório inexistente" em toda parada.
///
/// A versão anterior resolvia o relativo com `.exists()`, isto é, **contra o cwd do
/// processo `schematize`** — a mesma fragilidade do hook que ela deveria consertar.
/// Dava "são" exatamente quando o `schematize` rodava na raiz do projeto, que é o
/// único caso em que o hook também teria funcionado; no caso que quebra, ninguém
/// reavaliava. Por isso a regra agora é do FORMATO do caminho, não do disco.
///
/// **Entrada:** `cmd` — o comando do hook como está no settings.json.
/// **Saída:** `true` se precisa ser regravado. Comando sem caminho nenhum (ex.: só
/// `schematize overdev check`, resolvido pelo PATH) não é julgado — não dá pra saber
/// sem executar, e na dúvida não se mexe.
/// **Efeitos:** lê o disco (só `exists`), não escreve.
fn comando_quebrado(cmd: &str) -> bool {
    let caminhos = caminhos_citados(cmd);
    if caminhos.is_empty() {
        return false;
    }
    if caminhos.iter().any(|p| p.is_relative()) {
        return true;
    }
    !caminhos.iter().any(|p| p.exists())
}

/// Repara os hooks do overdev num settings.json específico.
///
/// Regrava o comando quando ele está quebrado (aponta só pra caminho inexistente) OU
/// quando é o comando de uma versão anterior. Devolve `Ok(true)` se mexeu no arquivo.
/// Arquivo que não existe / não é JSON: não faz nada (não inventa settings pra ninguém).
pub fn repara_hooks_em(arquivo: &Path, exe: &str) -> Result<bool, String> {
    let Ok(texto) = fs::read_to_string(arquivo) else {
        return Ok(false);
    };
    let Ok(mut root) = serde_json::from_str::<Value>(&texto) else {
        return Ok(false);
    };
    let mut mexeu = false;
    for (event, _marca, sub) in MARCAS {
        let alvo = hook_cmd(exe, sub);
        let Some(arr) =
            root.get_mut("hooks").and_then(|h| h.get_mut(event)).and_then(|a| a.as_array_mut())
        else {
            continue;
        };
        for grupo in arr.iter_mut() {
            let Some(hs) = grupo.get_mut("hooks").and_then(|h| h.as_array_mut()) else {
                continue;
            };
            for h in hs.iter_mut() {
                let Some(cmd) = h.get("command").and_then(|c| c.as_str()) else {
                    continue;
                };
                // Só mexemos em hook DO OVERDEV: ou cita o subcomando, ou é o script
                // legado da skill. Hook de terceiro no mesmo settings.json não é nosso.
                let nosso = cmd.contains(sub) || cmd.contains("overdev-stop.sh");
                if !nosso || cmd == alvo || !comando_quebrado(cmd) {
                    continue;
                }
                h["command"] = json!(alvo);
                mexeu = true;
            }
        }
    }
    if mexeu {
        let body = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
        fs::write(arquivo, body).map_err(|e| e.to_string())?;
    }
    Ok(mexeu)
}

/// Os settings.json que valem pra uma sessão: o do usuário e, se houver projeto, os
/// dele (`.claude/settings.json` e `settings.local.json`). O hook quebrado do relato
/// estava no do PROJETO — olhar só o do usuário não bastava.
pub fn arquivos_de_settings(projeto: Option<&Path>) -> Vec<PathBuf> {
    let mut v = vec![settings_path()];
    if let Some(p) = projeto {
        v.push(p.join(".claude").join("settings.json"));
        v.push(p.join(".claude").join("settings.local.json"));
    }
    v
}

/// Remove os grupos de hook do overdev (Stop/PreToolUse) sem tocar no resto.
pub fn disable() -> Result<(), String> {
    let mut root = load();
    if let Some(hooks) = root.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        for (event, needle, _sub) in MARCAS {
            if let Some(arr) = hooks.get_mut(event).and_then(|a| a.as_array_mut()) {
                arr.retain(|g| !group_has(g, needle));
            }
        }
    }
    save(&root)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REGRESSÃO: hook gravado por uma versão ANTIGA continua no settings.json depois
    /// do update. Foi o caso real de uma máquina já na versão mais nova recebendo
    /// `/usr/bin/schematize: not found` em todo Stop: o pacote .deb que instalou aquele
    /// caminho tinha sido removido pelo install do fonte, mas o comando ficou.
    #[test]
    fn hook_de_versao_antiga_e_detectado_como_desatualizado() {
        // O que a v-antiga gravava: caminho absoluto, cru, sem resolvedor.
        let antigo = json!({
            "hooks": {
                "Stop": [ { "hooks": [ { "type": "command", "command": "/usr/bin/schematize overdev check" } ] } ],
                "PreToolUse": [ { "matcher": "AskUserQuestion",
                                  "hooks": [ { "type": "command", "command": "/usr/bin/schematize overdev guard" } ] } ]
            }
        });
        // Continua "ligado" (é por isso que ninguém percebia): o needle casa.
        assert!(
            antigo
                .get("hooks")
                .and_then(|h| h.get("Stop"))
                .and_then(|a| a.as_array())
                .is_some_and(|arr| arr.iter().any(|g| group_has(g, MARCAS[0].1))),
            "o hook velho ainda casa como 'ligado' — por isso o enable pulava"
        );
        // Mas NÃO é o comando desta versão: é isso que passa a ser detectado.
        let alvo = hook_cmd("/home/x/.cargo/bin/schematize", "overdev check");
        assert_ne!(alvo, "/usr/bin/schematize overdev check");
        assert!(alvo.contains("command -v schematize"), "o comando atual resolve em runtime");
    }

    /// Os TRÊS formatos de quebra já relatados em campo são reconhecidos, e o hook
    /// legítimo (caminho que existe) NÃO é tocado.
    #[test]
    fn reconhece_hook_quebrado_e_poupa_o_legitimo() {
        // 1) caminho do pacote .deb, removido pelo install do fonte (Mint)
        assert!(comando_quebrado("/usr/bin/schematize overdev check"));
        // 2) caminho RELATIVO do hook shell legado (a máquina do 3º relato):
        //    só resolveria se o Claude Code rodasse exatamente naquela pasta.
        assert!(comando_quebrado("bash .claude/hooks/overdev-stop.sh"));
        // 2b) O MESMO relativo, agora com o arquivo EXISTINDO no cwd do teste. É o
        //     caso real que escapava: o `.exists()` da versão anterior dizia "são"
        //     e o reparo ia embora, enquanto o Claude Code — rodando de outra pasta —
        //     seguia falhando em toda parada. Relativo é quebrado independente do disco.
        let cwd = std::env::current_dir().expect("cwd");
        let dir = cwd.join(".claude/hooks");
        let _ = fs::create_dir_all(&dir);
        let script = dir.join("sz-teste-relativo.sh");
        let _ = fs::write(&script, "#!/usr/bin/env bash\n");
        assert!(
            script.exists(),
            "o teste precisa do arquivo existindo pra provar que existência não salva o relativo"
        );
        assert!(
            comando_quebrado("bash .claude/hooks/sz-teste-relativo.sh"),
            "caminho relativo é quebrado mesmo existindo no cwd do processo"
        );
        let _ = fs::remove_file(&script);
        // 3) caminho absoluto de uma skill que não está instalada
        assert!(comando_quebrado("bash /nao/existe/overdev-stop.sh"));

        // LEGÍTIMO: um caminho que existe → não se mexe (quem usa o fallback shell
        // de propósito continua com o dele).
        let existente = format!("bash {}", std::env::current_exe().unwrap().display());
        assert!(!comando_quebrado(&existente));

        // Sem caminho nenhum (resolvido pelo PATH): não dá pra julgar sem executar,
        // e na dúvida não se mexe.
        assert!(!comando_quebrado("schematize overdev check"));
    }

    /// Repara o settings do PROJETO — era onde estava o hook relativo quebrado, e
    /// olhar só o do usuário não bastava.
    #[test]
    fn repara_settings_do_projeto() {
        let base = std::env::temp_dir().join(format!("hooks-proj-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join(".claude")).unwrap();
        let arq = base.join(".claude").join("settings.json");
        std::fs::write(
            &arq,
            serde_json::to_string_pretty(&json!({
                "hooks": { "Stop": [ { "hooks": [
                    { "type": "command", "command": "bash .claude/hooks/overdev-stop.sh" }
                ] } ] }
            }))
            .unwrap(),
        )
        .unwrap();

        let mexeu = repara_hooks_em(&arq, "/home/x/.cargo/bin/schematize").unwrap();
        assert!(mexeu, "hook quebrado no projeto tinha de ser regravado");
        let novo: Value = serde_json::from_str(&std::fs::read_to_string(&arq).unwrap()).unwrap();
        let cmd = novo["hooks"]["Stop"][0]["hooks"][0]["command"].as_str().unwrap();
        assert!(cmd.contains("command -v schematize"), "virou o resolvedor: {cmd}");
        assert!(cmd.ends_with("exit 0"), "e nunca derruba a parada: {cmd}");

        // Rodar de novo é no-op (não fica reescrevendo arquivo de ninguém à toa).
        assert!(!repara_hooks_em(&arq, "/home/x/.cargo/bin/schematize").unwrap());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Hook de TERCEIRO no mesmo settings.json não é nosso — não se toca.
    #[test]
    fn nao_mexe_em_hook_alheio() {
        let base = std::env::temp_dir().join(format!("hooks-alheio-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let arq = base.join("settings.json");
        let alheio = "bash /caminho/que/nao/existe/formatador.sh";
        std::fs::write(
            &arq,
            serde_json::to_string_pretty(&json!({
                "hooks": { "Stop": [ { "hooks": [ { "type": "command", "command": alheio } ] } ] }
            }))
            .unwrap(),
        )
        .unwrap();

        assert!(!repara_hooks_em(&arq, "/bin/schematize").unwrap(), "não é hook do overdev");
        let depois: Value = serde_json::from_str(&std::fs::read_to_string(&arq).unwrap()).unwrap();
        assert_eq!(depois["hooks"]["Stop"][0]["hooks"][0]["command"].as_str().unwrap(), alheio);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// O comando gravado hoje sobrevive ao binário mudar de lugar: tenta o exe atual,
    /// depois ~/.cargo/bin, /usr/local/bin, /usr/bin e o PATH — e sai 0 se não achar
    /// nenhum (hook que não bloqueia é melhor que hook que quebra toda parada).
    #[test]
    fn comando_do_hook_tenta_todos_os_caminhos_e_nunca_falha() {
        let c = hook_cmd("/opt/schematize", "overdev check");
        for esperado in [
            "$HOME/.cargo/bin/schematize",
            "$HOME/.local/bin/schematize",
            "/usr/local/bin/schematize",
            "/usr/bin/schematize",
        ] {
            assert!(c.contains(esperado), "faltou {esperado} em: {c}");
        }
        assert!(c.ends_with("exit 0"), "sem binário nenhum, o hook sai limpo: {c}");

        // O comando é IGUAL em qualquer máquina: não carrega o caminho de quem o gerou.
        // É isso que impede o hook de ficar pinado num diretório de build ou num binário
        // temporário, e que faz "está atualizado?" ter sempre a mesma resposta.
        assert!(!c.contains("/opt/schematize"), "não embute o caminho de quem gerou: {c}");
        assert_eq!(c, hook_cmd("/home/alguem/.cargo/bin/schematize", "overdev check"));
        assert_eq!(c, hook_cmd("/usr/bin/schematize", "overdev check"));
    }

    /// O comando que a versão ANTERIOR gravava (termina em `exit 0` mudo). Congelado aqui
    /// para provar que quem já o tem é reconhecido como "nosso" e migrado, não duplicado.
    const HOOK_ANTIGO_CHECK: &str = r#"for c in "$HOME/.cargo/bin/schematize-overdev" "$HOME/.local/bin/schematize-overdev" /usr/local/bin/schematize-overdev /usr/bin/schematize-overdev "$(command -v schematize-overdev 2>/dev/null)"; do [ -n "$c" ] && [ -x "$c" ] && exec "$c" check; done; for c in "$HOME/.cargo/bin/schematize" "$HOME/.local/bin/schematize" /usr/local/bin/schematize /usr/bin/schematize "$HOME/.cargo/bin/overflow" "$HOME/.local/bin/overflow" /usr/local/bin/overflow /usr/bin/overflow "$(command -v schematize 2>/dev/null)"; do [ -n "$c" ] && [ -x "$c" ] && exec "$c" overdev check; done; exit 0"#;

    /// Roda `cmd` sob `sh -c` com PATH e HOME controlados; devolve (rc, stdout, stderr).
    fn roda_sh(cmd: &str, path: &Path, home: &Path) -> (i32, String, String) {
        let o = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(cmd)
            .env_clear()
            .env("PATH", path)
            .env("HOME", home)
            .output()
            .expect("sh tem de rodar");
        (
            o.status.code().expect("sem sinal"),
            String::from_utf8_lossy(&o.stdout).into_owned(),
            String::from_utf8_lossy(&o.stderr).into_owned(),
        )
    }

    /// Diretório de bancada dentro do `target/` do repo (nunca `/tmp`).
    fn bancada(nome: &str) -> PathBuf {
        let d = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("hook-tests")
            .join(format!("{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Sem NENHUM binário, o hook avisa UMA linha no stderr (guard desligado + comando de
    /// instalação), sai 0 e não suja o stdout (o stdout do hook é lido pelo Claude Code).
    #[test]
    fn sem_binario_nenhum_avisa_no_stderr_e_sai_zero() {
        let base = bancada("sem-binario");
        let vazio = base.join("vazio");
        let home = base.join("home");
        std::fs::create_dir_all(&vazio).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        for p in [
            "/usr/local/bin/schematize-overdev",
            "/usr/bin/schematize-overdev",
            "/usr/local/bin/schematize",
            "/usr/bin/schematize",
            "/usr/local/bin/overflow",
            "/usr/bin/overflow",
        ] {
            assert!(!Path::new(p).exists(), "{p} existe: o teste não isola o ramo sem-binário");
        }
        for sub in ["check", "guard"] {
            let (rc, out, err) = roda_sh(&hook_cmd("x", sub), &vazio, &home);
            assert_eq!(rc, 0, "nunca bloqueia: {err}");
            assert_eq!(out, "", "stdout limpo");
            assert_eq!(err.lines().count(), 1, "UMA linha: {err:?}");
            assert!(err.contains("DESLIGADO"), "diz que o guard está off: {err}");
            assert!(
                err.contains("cargo install --git https://github.com/schematizeme/schematize_overdev_rs --locked"),
                "traz o comando de instalação: {err}"
            );
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Com o binário presente o hook o executa e NÃO emite o aviso.
    #[test]
    fn com_binario_executa_e_nao_avisa() {
        use std::os::unix::fs::PermissionsExt;
        let base = bancada("com-binario");
        let home = base.join("home");
        let bin = home.join(".cargo").join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let fake = bin.join("schematize-overdev");
        std::fs::write(&fake, "#!/bin/sh\necho \"ran $1\"\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (rc, out, err) = roda_sh(&hook_cmd("x", "check"), &base.join("nada"), &home);
        assert_eq!((rc, out.as_str(), err.as_str()), (0, "ran check\n", ""));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Uma linha, sem quebra de linha, e o JSON round-tripa (seguro para o settings.json).
    #[test]
    fn comando_e_uma_linha_e_json_seguro() {
        let c = hook_cmd("x", "check");
        assert!(!c.contains('\n'), "uma linha só");
        let v = json!({ "command": c });
        let volta: Value = serde_json::from_str(&v.to_string()).unwrap();
        assert_eq!(volta["command"].as_str().unwrap(), c);
    }

    /// O hook ANTIGO e o NOVO são "nossos" (a marca casa nos dois); quem tem o antigo é
    /// detectado como desatualizado, migrado para o novo SEM duplicar, e depois é idempotente.
    #[test]
    fn hook_antigo_e_novo_sao_reconhecidos_sem_duplicar() {
        let novo_check = hook_cmd("x", "check");
        assert_ne!(novo_check, HOOK_ANTIGO_CHECK, "a string mudou");
        for c in [HOOK_ANTIGO_CHECK, novo_check.as_str()] {
            let g = json!({ "hooks": [ { "type": "command", "command": c } ] });
            assert!(group_has(&g, MARCAS[0].1), "reconhecido como nosso: {c}");
        }
        let mut root = json!({ "hooks": {
            "Stop": [
                { "hooks": [ { "type": "command", "command": "bash /alheio.sh" } ] },
                { "hooks": [ { "type": "command", "command": HOOK_ANTIGO_CHECK } ] }
            ]
        } });
        assert!(!hooks_atualizados_em(&root, "x"), "o antigo está desatualizado");
        aplica_hooks(&mut root, "x");
        assert!(hooks_atualizados_em(&root, "x"), "migrou para o novo");
        let stop = root["hooks"]["Stop"].as_array().unwrap();
        let nossos = stop.iter().filter(|g| group_has(g, MARCAS[0].1)).count();
        assert_eq!(nossos, 1, "sem duplicar: {stop:?}");
        assert!(stop.iter().any(|g| group_has(g, "/alheio.sh")), "hook alheio preservado");
        // Idempotente: reaplicar não muda nada.
        let antes = root.clone();
        aplica_hooks(&mut root, "x");
        assert_eq!(root, antes);
        assert!(hooks_atualizados_em(&root, "x"));
    }

    /// O reparo por arquivo (`check`) não reescreve nem duplica o hook novo, e deixa o
    /// antigo (caminhos válidos, não "quebrado") para a migração do `refresh_hooks`.
    #[test]
    fn reparo_por_arquivo_nao_mexe_no_novo() {
        let base = bancada("reparo");
        let arq = base.join("settings.json");
        let novo = hook_cmd("x", "check");
        std::fs::write(
            &arq,
            json!({ "hooks": { "Stop": [ { "hooks": [ { "type": "command", "command": novo } ] } ] } })
                .to_string(),
        )
        .unwrap();
        assert!(!repara_hooks_em(&arq, "x").unwrap(), "o novo é o alvo: no-op");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// O QUE: um `settings.json` malformado (tipo errado em `hooks`) NÃO derruba o CLI.
    ///
    /// POR QUE: o arquivo é de quem usa. `"hooks": "x"` fazia `as_object_mut()` devolver
    /// None e o `unwrap()` panicava — o CLI inteiro morria por causa de um arquivo alheio
    /// mal editado. Entrada hostil é entrada hostil, venha de rede ou de disco: o caminho
    /// certo é corrigir o nó e seguir, nunca panicar.
    #[test]
    fn settings_malformado_nao_derruba() {
        for bruto in [
            r#"{"hooks": "isto deveria ser objeto"}"#,
            r#"{"hooks": {"Stop": "isto deveria ser array"}}"#,
            r#"{"hooks": {}}"#,
            r#"{}"#,
        ] {
            let mut v: Value = serde_json::from_str(bruto).expect("fixture é json válido");
            ensure_group(&mut v, "Stop", "overdev check", json!({"hooks": []}));
            // O contrato é sobreviver e deixar a estrutura utilizável.
            assert!(
                v["hooks"]["Stop"].is_array(),
                "depois do reparo, hooks.Stop tem que ser array — veio {}",
                v["hooks"]["Stop"]
            );
        }
    }

    /// O QUE: uma raiz que nem objeto é (um array no lugar) também não derruba.
    #[test]
    fn raiz_que_nao_e_objeto_nao_derruba() {
        let mut v: Value = serde_json::from_str("[]").unwrap();
        ensure_group(&mut v, "Stop", "overdev check", json!({"hooks": []}));
        assert!(v.is_array(), "sem objeto pra mexer, sai sem tocar em nada");
    }
}
