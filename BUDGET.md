# BUDGET — orçamento de memória (RSS de pico)

Piso de eficiência de runtime da casa (`eficiencia-runtime.md`): cada operação declara um teto de RSS de pico, medido e imposto por teste. Teto da casa: 256 MiB para CLI/serviço, 400 MiB para GUI.

| Operação | RSS pico alvo (release) | Medido release (antes -> depois) | Medido debug (teste) | Teto no teste | Como medir |
|---|---|---|---|---|---|
| `schematize-overdev --version` | <= 8 MiB | 4,9 -> 4,9 MiB | 7,3 MiB | 10 MiB | `cargo test --no-default-features --test orcamento -- --nocapture` |
| `agents` | <= 8 MiB | 5,4 -> 5,4 MiB | 15,5 MiB | 21 MiB | idem |
| `hooks` | <= 8 MiB | 5,3 -> 5,3 MiB | 8,6 MiB | 12 MiB | idem |
| `status` | <= 8 MiB | 5,1 -> 5,1 MiB | 15,6 MiB | 21 MiB | idem |
| `checklist` | <= 8 MiB | 5,4 -> 5,4 MiB | 15,6 MiB | 21 MiB | idem |
| Janela (`schematize-overdev-gui`) | <= 40 MiB | pico 37 MiB (`VmHWM` de instância real em uso) | não medido (precisa de display no teste) | n/a | `grep VmHWM /proc/$(pgrep -f schematize-overdev-gui)/status` |
| Canal do PTY (agente acoplado) | <= 1 MiB retido | ilimitado -> 256 pedaços x 4 KiB | teste `canal_do_pty_e_limitado` | 256 pedaços | `cargo test --lib agentrun` |

Teto = medido (debug, o pior caso) + ~30%. O medidor usa `ru_maxrss` do `wait4` (igual ao `VmHWM`; o polling de `/proc` perde o pico de um processo de milissegundos). Debug é maior que release porque os testes usam o build de debug; o release instalado fica em ~5 MiB.

## O que mudou

`ClaudeRunner::spawn` usava `mpsc::channel` ilimitado entre a thread que lê o PTY e a `Session`. Com a janela lenta (ou fechada sem drenar) o output do agente acumulava sem teto. Agora é `sync_channel(256)`: cheio, o leitor bloqueia (backpressure) e nada se perde.

## O que ficou e por quê

- 245 pacotes, 28 duplicatas (quase todas do stack do iced/winit/wayland, não controláveis aqui) e binário de 4,7 MiB (CLI) / 9,0 MiB (janela). `iced` já está em `default-features = false` com `tiny-skia`, sem `wgpu`; `rusqlite` é `bundled` de propósito (ADR). Trocar dependência não reduz o RSS medido (CLI em ~5 MiB), então nada foi trocado.
- `default = ["gui"]` mantido: o `install.sh` compartilhado depende dele.
