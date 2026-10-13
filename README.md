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
