---
name: narrahub-database-safety
description: Guardas para qualquer trabalho que toque migrations, schema SQLite, backup, restauração ou os perfis dev/qualification/production do NarraHub. Use SEMPRE que a tarefa envolver `src-tauri/src/database/migrations.rs`, uma tabela nova ou alterada, o `BackupService`, restauração, ou testar contra um banco com dados reais — mesmo que o pedido pareça simples ("adiciona uma coluna", "testa com o banco de verdade"). O objetivo desta skill é impedir que uma sessão de IA corrompa ou sobrescreva o acervo real de um escritor.
---

# Segurança de banco de dados

O banco local é o acervo do escritor — livros, fichas, histórico. Trate
qualquer operação aqui como se fosse irreversível até provar o contrário,
porque no dispositivo do usuário real, ela é.

Documentos completos (leia antes de mexer em algo não coberto aqui):
[`docs/BACKUP_AND_RECOVERY.md`](../../../docs/BACKUP_AND_RECOVERY.md),
[`docs/ADR/0004-immutable-migrations-and-updates.md`](../../../docs/ADR/0004-immutable-migrations-and-updates.md),
[`docs/ADR/0006-backup-as-critical-infrastructure.md`](../../../docs/ADR/0006-backup-as-critical-infrastructure.md).

## Nunca toque no banco de produção real diretamente

O app já isola três perfis por `identifier` do Tauri — cada um com seu
próprio diretório de dados do sistema operacional, então eles nunca colidem:

| Perfil | Comando | `identifier` |
| --- | --- | --- |
| Desenvolvimento | `npm run desktop:dev` | `com.narrahub.app.dev` |
| Qualificação (dados de teste, espelha produção) | `npm run desktop:qualification` | `com.narrahub.app.qualification` |
| Produção | `npm run desktop:build` (instalado) | `com.narrahub.app` |

Teste migration, backup ou restauração sempre em `dev` ou `qualification`.
Se a tarefa pede validar contra "o banco de verdade", isso significa: copiar
o banco de produção para uma réplica isolada e abrir essa réplica sob o
perfil de qualificação — nunca apontar o app para o arquivo de produção
original. Depois de qualquer teste, confirme por SHA-256 e data de
modificação que o arquivo original de produção não mudou.

## Migration: append-only, sempre

Uma migration publicada é imutável — o `tauri-plugin-sql` guarda o checksum
de cada migration aplicada e rejeita reabrir um banco se o checksum não
bater. Isso significa, na prática:

- **Nunca edite** uma migration que já existe em `migrations.rs` com número
  já publicado (verifique `docs/ARCHITECTURE_EVOLUTION_PLAN.md` para saber
  qual é a última migration congelada).
- Toda mudança de schema é uma **migration nova**, mesmo para corrigir um
  erro de uma migration anterior.
- Prefira migrations aditivas (`ALTER TABLE ADD COLUMN`, tabela nova). Um
  rebuild de tabela exige teste específico de foreign keys, índices e
  triggers — não é o caminho padrão.
- Depois de escrever a migration, teste a cadeia completa: banco vazio
  aplicando tudo em sequência, e uma cópia do banco na versão publicada
  anterior fazendo upgrade — feche e reabra sem reaplicar a migration.

## Backup: fluxo que não pode ser pulado

Ao mexer no `BackupService` ou em qualquer operação que crie/restaure um
snapshot, o fluxo publicado é:

```text
1. Abrir banco de origem só para leitura
2. Snapshot via Online Backup API do SQLite (funciona com WAL ativo)
3. integrity_check na cópia
4. Copiar assets (sem seguir symlink)
5. Calcular hashes e tamanhos
6. Gravar manifest.json
7. Publicar o diretório por rename atômico no mesmo volume
8. Só ENTÃO o backup existe — falha antes disso não deixa resíduo restaurável
```

Nunca sobrescreva um backup já publicado. Retenção só roda depois que um
snapshot novo foi validado, e nunca remove backups `manual` ou `pre_restore`.

Restaurar segue o mesmo espírito: valida tudo numa área temporária
(`.restore-<token>`, expira em 10 min), cria um `pre_restore` da base atual
antes de trocar qualquer coisa, e só troca os arquivos depois de fechar o
pool SQL do frontend — com rollback automático se a troca falhar no meio.

## Checklist antes de considerar pronto (Gate 2 do plano de arquitetura)

- `PRAGMA foreign_key_check` limpo.
- `PRAGMA integrity_check` limpo.
- Migration 1→N testada em memória e em arquivo.
- Cópia do banco da versão publicada anterior faz upgrade sem perder
  contagem de linhas nas tabelas canônicas.
- Reabrir o banco depois de fechar não reaplica nenhuma migration.
- Se mexeu em backup/restore: round-trip completo (criar → corromper/simular
  falha → confirmar rejeição; criar → restaurar → confirmar dado idêntico).

Se qualquer um desses não foi verificado, a mudança não está pronta — não é
"detalhe para depois". Veja [[narrahub-validate]] para o resto da checagem
local, e [[narrahub-architecture]] para os princípios que essa skill protege.
