# Redimensionamento da Fase 1 — Handoff

```text
Agente:  Claude
Data:    2026-08-31
Branch:  claude/NH-fase1-escopo
Status:  DONE
```

## O que foi feito

A Fase 1 foi redimensionada contra o código, antes de qualquer implementação. As tarefas
NH-010 a NH-013 foram reescritas; a fila mudou de ordem.

## Descoberta importante

**O plano supunha que a Fase 1 começava do zero. Não começa.** O que já existe:

- `src-tauri/fixtures/schema10_representative.sql`, com 18 tabelas povoadas — universos,
  histórias, livros, capítulos, entidades, atributos, relações, menções, tags, timeline,
  planning, attachments, devices, sync_events e colaboração.
- `representative_schema10_fixture_upgrades_without_data_loss`, que testa o upgrade dessa
  fixture sem perda de dados.
- Testes por migration de v7 a v15, vários com `pragma_foreign_key_check`.
- `full_migration_chain_creates_a_reopenable_file_database`, que roda a cadeia 1→15 num
  arquivo real, reabre e confere `integrity_check`.
- Em `backup.rs`: backup online com WAL, rejeição por hash divergente, manifesto tentando
  escapar do diretório, staging interrompido, e retenção que preserva backups pré-restore.

Tudo isso já roda no CI via `cargo test`.

**Esta é a quarta vez que uma premissa do plano cai no primeiro contato com o código, e a
terceira em que ela cai para melhor.** Antes foram: a `main` que estava mais atrás do que
se pensava, o validador de versão que já existia, e a proteção de branch que já estava
ligada. O padrão é claro — este repositório está em estado melhor do que a memória do
projeto sugere, e planejar sem verificar produz trabalho duplicado.

## O que realmente falta na Fase 1

Menor e mais difícil do que o plano dizia:

1. **NH-011 — rollback de restore que falha no meio.** O caminho feliz e várias recusas
   estão cobertos; o caso que mais assusta não está. `automatic_retention_preserves_manual_and_pre_restore_backups`
   indica que existe backup pré-restore — a metade certa do mecanismo. Falta o teste da
   outra metade.
2. **NH-012 — ciclo de atualização no app empacotado.** O buraco real: instalar N, criar
   conteúdo, instalar N+1, reabrir. Só existe como evidência manual de 0.7.4 em
   `docs/PHASE_0_1_QUALIFICATION.md`. É o único que teste unitário não fecha.
3. **NH-010 — fixtures nos schemas 13, 14 e 15.** Hoje só há fixture povoada em 10. Quem
   está na 0.9.1 está no 15.
4. **NH-013 — checklist de release como gate.**

## Nova ordem recomendada

`NH-011` primeiro: é o maior risco de perda de dados ainda sem teste, e é autocontido.
Depois `NH-012`, que é o mais caro e o mais valioso.

## Recomendação sobre a NH-012

Não force automação onde ela vai mentir. Um checklist honesto do que exige instalador real
vale mais que um teste verde que não exercita o instalador. A decisão do que é automatizável
faz parte da tarefa, não é pré-condição dela.

## Validação executada

```text
npm run test:architecture         ✅ 38/38
npm run release:validate-version  ✅
```

Nenhuma mudança de código — só a fila de trabalho.

## O que ficou de fora

Não comecei a implementar a NH-011. Redimensionar a fase e implementá-la são coisas
diferentes, e misturar as duas num PR só esconderia a primeira.
