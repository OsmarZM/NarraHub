# Backup automático antes de atualizar o banco

Toda vez que uma versão nova do NarraHub precisa atualizar o schema do banco, ela faz um backup
**antes**, confere esse backup, e só então atualiza. Se a atualização falhar — ou o aparelho
desligar no meio —, o banco que existia antes volta ao lugar.

## Onde fica o backup

| sistema | pasta de dados do app | backup |
| --- | --- | --- |
| Windows | `%APPDATA%\com.narrahub.app\` | `%APPDATA%\com.narrahub.app\backups\<id>\` |
| Android | armazenamento privado do app (`/data/data/com.narrahub.app/`) | `…/backups/<id>/` |

Cada backup é uma pasta com:

```text
narrahub.db       cópia consistente do SQLite (API de backup, não cópia de arquivo aberto)
manifest.json     versão do schema, versão do app, SHA-256, tamanho, motivo "preMigration"
```

O backup de migration **não** copia a pasta de imagens (`assets/`): migration de schema não mexe
nela, e copiar o acervo de imagens a cada atualização custaria minutos e espaço num celular.

A retenção automática mantém os backups automáticos mais recentes e **nunca** remove o que acabou de
ser criado.

## Como a atualização é protegida

```text
1 portão de compatibilidade   banco mais novo que o app → tela de recuperação, nada abre
2 preparar                    migration interrompida antes? devolve o banco original
                              schema atrás? backup + integrity_check + validação do manifesto
                              grava migration-em-andamento.json (com o id do backup)
                              backup falhou? o banco NÃO é aberto nem atualizado
3 plugin SQL aplica as migrations
4 confirmar                   versão == a esperada e integrity_check ok → apaga o registro
5 qualquer falha em 3 ou 4    devolve o banco do backup, apaga os -wal/-shm do banco migrado
```

## Garantias, e até onde elas vão

| situação | resultado |
| --- | --- |
| a migration falha | o banco anterior volta do backup |
| o processo morre no meio (fechar à força, travamento) | na próxima abertura o marcador é encontrado e o banco anterior volta |
| **queda de energia** no meio | idem: backup, manifesto e marcador são gravados com `sync_all` (fsync / `FlushFileBuffers`) antes de a próxima etapa começar, e a pasta é sincronizada em Android e Unix |
| o banco atualizado ainda está aberto (Windows) | a troca falha **antes** de tocar em qualquer coisa; o banco atual fica, o marcador fica, e a próxima tentativa conclui |
| banco mais novo que esta versão | recusado pelo próprio backend (`prepare`), mesmo que o frontend não pergunte antes |

No Windows, a pasta em si não pode ser sincronizada pela biblioteca padrão; o NTFS registra a troca de
nome no próprio journal de metadados. O que o NarraHub garante lá é que os **bytes** de cada arquivo
estão no disco antes de a troca de nome acontecer.

**A troca do arquivo não depende de `rename` sobrescrever o destino.** O banco atual sai do nome primeiro
(se estiver aberto pelo SQLite, isso falha sem mudar nada), o arquivo restaurado entra, e o antigo só é
apagado depois. Os `-wal`/`-shm` do banco atualizado saem de lugar antes e voltam se a troca falhar.
Código: `src-tauri/src/database/duravel.rs`.

## Nenhum comando toca o banco antes disso

O backend guarda o estado do banco (`Unprepared`, `Migrating`, `Ready`, `RecoveryRequired`), e o ponto
comum de acesso dos comandos (`interface::tauri::database`) e o Sync V1 recusam qualquer operação antes
de `Ready`. A proteção não depende de o frontend chamar as funções na ordem certa. Código:
`src-tauri/src/database/estado.rs`.

`migration-em-andamento.json` fica na pasta de dados do app enquanto a atualização acontece. Se ele
existir na próxima abertura, a atualização anterior não terminou — o app devolve o banco original e
tenta de novo.

Código: `src-tauri/src/database/upgrade.rs`. Testes: `database::upgrade` (12), `database::duravel` (4),
`database::estado` (2) no Rust, e `tests/migration-safety.test.mjs`.

## Restaurar um backup à mão

**Pelo app:** Configurações → Backup → escolha o backup "antes da migração" → Restaurar. O app valida
o backup, fecha o banco, troca o arquivo e reabre.

**Sem conseguir abrir o app (Windows):**

1. Feche o NarraHub.
2. Abra `%APPDATA%\com.narrahub.app\`.
3. Renomeie `narrahub.db` para `narrahub.db.antes-de-restaurar` e apague `narrahub.db-wal` e
   `narrahub.db-shm`, se existirem.
4. Copie `backups\<id>\narrahub.db` para `%APPDATA%\com.narrahub.app\narrahub.db`.
5. Abra a **mesma versão do NarraHub que criou o backup** (`appVersion` no `manifest.json`) ou uma mais
   nova. Versão mais antiga que o schema do backup não abre o banco.

## Voltar de versão

Não existe downgrade direto: uma versão antiga não abre banco atualizado por uma nova. Para voltar,
instale a versão antiga e restaure o backup "antes da migração" que a versão nova criou.
