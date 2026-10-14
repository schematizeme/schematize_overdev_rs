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
