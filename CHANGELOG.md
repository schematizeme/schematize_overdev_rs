# Changelog

## 0.5.0

- Checklist particionado (ADR-0027): `schematize-overdev checklist particionar|sync|check|proxima`.
  Partições `checklist/pN-NNN-slug[--completed].md` de 25-50 itens; `CHECKLIST.md` vira índice.
- Leitores de afazeres (itens abertos, prompts, Stop-hook) ignoram `*--completed.md`
  (`paths::multidoc_files_abertos`); progresso e totais seguem contando tudo.
- Stop-hook roda `sync` antes de contar e bloqueia partição `--completed` com item aberto.
- `overdev split` (round-robin) recusa checklist já particionado.
- Prompts do overdev ensinam índice + partição da vez; brief de subagent leva só a partição.

## 0.4.0

Ver o histórico git.
