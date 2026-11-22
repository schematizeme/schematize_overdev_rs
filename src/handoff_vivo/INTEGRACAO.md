# Integração do `handoff-vivo` no CLI

O módulo expõe `overdev_casa::handoff_vivo::executar(args: &[String]) -> i32`. Os `args` são os
argumentos DEPOIS de `handoff-vivo` (`["run", "--mecanico-s", "30"]`). Códigos: 0 ok, 1 erro, 2 uso.
Não foi ligado ao clap porque o registro mexe em `cli/args.rs` e `cli/comandos.rs` (donos: integração
de resiliência) e o retorno é `i32`, não `Result<(), String>`.

## 1. `src/cli/args.rs` — dentro de `pub enum Cmd`

```rust
    /// Handoff vivo: mantém o handoff do projeto sempre atualizado (run | agora | verificar | status).
    #[command(name = "handoff-vivo", trailing_var_arg = true, allow_hyphen_values = true)]
    HandoffVivo {
        /// `run|agora|verificar|status` e as flags (`--mecanico-s`, `--claude-s`, `--sem-claude`,
        /// `--modelo`, `--projeto`, `--reparar`).
        args: Vec<String>,
    },
```

## 2. `src/cli/comandos.rs` — dentro de `pub fn executar(sub: Cmd)`

```rust
        Cmd::HandoffVivo { args } => match crate::handoff_vivo::executar(&args) {
            0 => Ok(()),
            c => Err(format!("handoff-vivo terminou com código {c}")),
        },
```

(`executar` já imprime a mensagem de erro; se preferir preservar o código exato, chame
`std::process::exit(c)` no braço em vez de devolver `Err`.)

## 3. Comandos

| Comando | O que faz |
|---|---|
| `handoff-vivo run [--mecanico-s 30] [--claude-s 1800] [--sem-claude] [--modelo haiku]` | loop; SIGTERM/SIGINT gravam um último handoff e saem com 0 |
| `handoff-vivo agora` | um ciclo imediato (mecânico + Claude síncrono, ignora intervalo/backoff) |
| `handoff-vivo verificar [--reparar]` | valida as 3 gerações de cada arquivo; `--reparar` (sob o lock) promove a melhor válida e limpa `.tmp` |
| `handoff-vivo status` | dono do lock, seq, falhas do Claude, data das notas |

`--projeto DIR` escolhe o workspace (padrão: sobe do cwd até achar `<d>/<d>_archive`).

## 4. Arquivos

* `<projeto>_archive/context/ULTIMO-context.md`, `ULTIMO-checklist.md` (+ `.1`, `.2`: 3 gerações) e os
  datados `<YYYY-MM-DD-HH-MM-SS>-context.md`/`-checklist.md` (um carimbo por sessão e por handoff do Claude).
* `.schematize/handoff-vivo/`: `escritor.lock` (pid + starttime), `estado.hv` (3 gerações),
  `erros.log` (rotaciona em 256 KiB), `claude-cwd/` (cwd neutro do `claude -p`).
* Ledger: uma linha por chamada do Claude em `<overdev>/ledger.jsonl` (tokens estimados, custo em micro-USD).

## 5. Unit systemd

`crate::handoff_vivo::unit::unit_systemd()` devolve o texto (nada é instalado). O instalador deve
ajustar `WorkingDirectory=` para a pasta do projeto.

## 6. Hooks do usuário e isolamento do `claude -p`

Validado em uso real (relatório `2026-11-22-handoff-uso-real.md`). O `claude -p` roda num cwd neutro, com
`SCHEMATIZE_HANDOFF_VIVO=1` e `CLAUDE_CODE_DISABLE_CLAUDE_MDS=1`, e flags de isolamento
(`--setting-sources project --disable-slash-commands --tools "" --strict-mcp-config --system-prompt`):
não carrega os hooks do usuário (SessionStart/Stop), CLAUDE.md global, skills nem MCP (~46 mil -> ~640
tokens por chamada). `--bare` NÃO serve (exige `ANTHROPIC_API_KEY`). O `check` do overdev libera a parada
sob a variável, mas só nos binários reinstalados: os de `~/.cargo/bin` antigos ainda bloqueiam se o cwd
tiver overdev ativo (o cwd neutro os mantém inertes).
