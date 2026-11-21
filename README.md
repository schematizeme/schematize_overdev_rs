# schematize-overdev

O motor de um overdev: fila, trava, supervisor e gate.

Extraído do `schematize` na **E6** da extradição (ADR-0018, ADR-0012 F5) — a última fase, e a
mais arriscada: **este é o motor que executou a própria extração**.

```
schematize-overdev start "o objetivo"   # começa um run neste diretório
schematize-overdev status               # em que pé está
schematize-overdev progresso --json     # o mesmo, para outro programa ler
schematize-overdev supervise            # relança o agente se ele morrer com item aberto
schematize-overdev split 4 --dispatch   # divide o checklist em 4 e lança 4 claudes
schematize-overdev guard                # (hook PreToolUse) veta AskUserQuestion durante o run
schematize-overdev check                # (hook Stop) recusa parar com item de máquina aberto
schematize-overdev hooks --repair       # regrava os hooks apontando para este binário
```

`schematize overdev …` continua funcionando: o hub encaminha.

## Resiliência absoluta (0.6)

Se o kernel/OOM/SIGKILL matar o overdev, ele retoma exatamente de onde parou, sem perder nem duplicar
trabalho. O estado mora em `.overdev/estado/` (gitignored; segue a regra "ler ambos" do control-plane):
journal WAL com `fsync` a cada evento + snapshot atômico (plataforma v0.4.0, features `persistencia`,
`memoria`, `sistema`). **Toda** transição entra no journal ANTES do efeito (write-ahead).

```
schematize-overdev executar --plano plano.json   # despacha as unidades sob governador de memória
schematize-overdev retomar                       # (ou `schematize-overdev --resume`) continua do journal
schematize-overdev sistema diag [--json] [--lang pt|en]   # perfil OOM/systemd/zram/PSI + achados (só leitura)
schematize-overdev memoria status [--json]       # leitura do host, decisão do governador, tetos recomendados
schematize-overdev vigia unit                    # imprime a unit systemd do vigia (nada é instalado)
schematize-overdev vigia run [-- <args>]         # delega ao `schematize-vigia` (precisa estar no PATH)
```

- **Plano** (`--plano`): JSON `{ "prefixo", "modelo", "paralelo", "margem_mib", "itens": [{ "id", "brief",
  "tipo", "teto_mib" (ADR), "marca", "argv" }] }`. `argv` aceita `{prompt} {brief} {prefixo} {modelo} {id}`;
  vazio = `claude -p`. O prefixo estável vai igual em todo despacho (cache de prompt).
- **Portão**: cada unidade nasce presa num invólucro `sh` que só executa depois de `Lancada` (pid +
  `starttime`) estar no journal; ao terminar grava `<código> <oom_kill>` num `resfile` atômico. Por isso
  uma unidade que sobrevive ao SIGKILL do orquestrador é **reanexada** (e, se já terminou, o resultado é
  lido sem re-executar); uma que nunca foi liberada simplesmente não roda.
- **Reconciliação**: PID reciclado é detectado por `starttime` e nunca é morto; `--matar-orfas` mata só
  as unidades registradas. `Entregue` nunca regride. Nunca pergunta: esgotou a escada, parkeia (`- [~]`).
- **Governador**: admissão por reserva vs margem do host (`sistema::margem_segura`), estrangula sob PSI,
  `PedirEncolher`/`PedirParar` antes do teto duro; picos por tipo em `picos.hist` recomendam o teto
  (p95 x 1,25, clamp 500-2048 MiB; o `teto_mib` por ADR do plano manda). Eventos do monitor viram linhas
  `aviso` em `jaulas.jsonl` e observações no journal. O orquestrador tenta `oom_score_adj` baixo (sem erro
  se não for permitido).
- **Estado corrompido** (bit-flip/truncado): cai no `.prev`/replay e reporta; sem saída, vai para
  `estado/corrompido-<ts>/` e recomeça. Nunca panic.
- **Testes**: `PERSIST_SEED=<n> cargo test --test resiliencia_kill -- --nocapture` (30 iterações de SIGKILL
  aleatório sobre 20 unidades; `OD_KILL_ITER` muda o nº). `comando_que_falha_traz_o_stderr` é ambiental:
  rode com `TMPDIR=/tmp`.

## Os hooks, e por que a ordem da migração importou

O Claude Code chama `guard` a **cada chamada de ferramenta** e `check` a **cada parada**. Uma
janela em que o binário instalado não soubesse responder isso bloquearia a sessão inteira — por
isso o app novo foi instalado e provado ANTES de o hub perder o domínio, nunca o contrário.

O comando gravado no `settings.json` procura o `schematize-overdev` e, se não achar, cai no
`schematize overdev …`. Os dois formatos funcionam, e quem tem o antigo não precisa fazer nada.

## A janela (`schematize-overdev-gui`)

Desde a **0.2.0** a janela é **iced 0.14 com o renderer `tiny-skia`** (CPU, sem `wgpu`), no lugar
do Slint — paridade com a tela de Overdev do hub. O iced carrega x11/wayland/xkbcommon por
`dlopen`: **não há libs `-dev` de sistema** para compilar nem para o CI. O release publica os mesmos
assets de antes (`schematize-overdev-*` e `schematize-overdev-gui-*`). A 0.2.1 corrige o build da janela em macOS e Windows (o release 0.2.0 só saiu para Linux).

## Lib com costuras de plataforma (0.4.0, ADR-0026)

Este repo é **Unix-only** (Linux, FreeBSD, macOS). O corpo do CLI e da janela mora na lib
`overdev_casa`, que recebe a plataforma por injeção:

- `overdev_casa::cli::run(args, &Plataforma, &dyn Hooks) -> i32` — o `schematize-overdev`;
- `overdev_casa::gui::run(args, &Plataforma, &dyn Hooks) -> iced::Result` — a janela, atrás da
  feature `gui` (padrão; `--no-default-features` deixa o iced de fora);
- `overdev_casa::hooks::{Hooks, HooksUnix}` — como o `settings.json` do Claude Code guarda o
  comando de `check`/`guard`; `overdev_casa::agentrun::{montar_path, extras_unix}` — o `PATH` do
  filho com separador e diretórios como dado.

`Plataforma` vem de `schematize_plataforma` (repo `schematize_plataforma_rs`): `shell`, `terminal`,
`abridor`, `atalhos`, `processos`… Os dois binários são só `main` de poucas linhas com
`Plataforma::unix()` + `HooksUnix`. O `.exe` do Windows é montado no repo `schematize_windows_rs`
com a `Plataforma` e os `Hooks` do Windows; **este repo não publica mais asset Windows**.

## Agentes (`agentes.toml`, ADR-0024)

Quem faz o quê num overdev é configuração, não código: o **principal** (Claude Opus) só planeja,
decompõe, despacha e revisa; cada papel tem **motor** (`claude`, `codex` ou `cursor`), **modelo**,
um interruptor `ativo` e uma descrição.

- **Onde mora:** global em `~/.claude/schematize/agentes.toml` (ao lado do `config.json`); override
  por projeto em `<projeto>/.schematize/overdev/agentes.toml`. Ordem de merge: padrões embutidos <
  global < projeto — o projeto sobrescreve por papel, e `escada`/`roteamento` inteiros. Arquivo
  ausente = padrões; arquivo inválido = erro tipado (a janela mostra em vermelho, nunca quebra).
- **Aba «Agentes» da janela** (`schematize-overdev-gui --aba agentes`): escolhe Global ou «Este
  projeto», edita papéis, escada e roteamento, mostra se claude/codex/cursor estão instalados e
  logados, e só grava ao clicar «Salvar» (valida antes; «Restaurar padrão» e «Descartar» disponíveis).
- **`schematize-overdev delegar <papel> --brief-stdin --json`:** executa uma tarefa no motor do papel
  (`--listar` mostra papéis e disponibilidade). Saída 0 ok · 2 uso/config inválida · 3 motor
  indisponível · 4 motor falhou.
- **Escada padrão** (tarefa de código sobe de cima para baixo; esgotou → park + `- [~]`):
  `trivial` (haiku) → `executor` (sonnet) → `fallback-1` (codex gpt-5.6-terra) → `fallback-cursor`
  (cursor auto) → `fallback-2` (opus). Roteamento: `visual` e `imagem` vão direto ao Codex.
- **Codex sem login?** rode `codex login`. Cursor ausente: instale pelo `schematize-market`.
  Motor indisponível é pulado com aviso.
- **Imagem:** o papel `imagem` PEDE a imagem ao Codex Terra (cota da assinatura). É proibido usar
  endpoint/API de imagem paga.

## FreeBSD

O release publica também `schematize-overdev-freebsd-x86_64` (CLI cross-compilado com `cross` 0.2.5 e
executado de verdade numa VM FreeBSD 14.2 antes de anexar) e, em regime de melhor esforço,
`schematize-overdev-gui-freebsd-x86_64` (compilada nativamente na VM; só entra se compilou). O
parasita do market baixa `schematize-market-freebsd-x86_64` / `schematize-market-gui-freebsd-x86_64`.
