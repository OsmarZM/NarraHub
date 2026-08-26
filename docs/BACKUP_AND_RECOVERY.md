# Backup e recuperação local

## Objetivo

Proteger o acervo local antes de migrations, atualizações e falhas do dispositivo. O banco ativo nunca é sobrescrito durante a criação ou validação de um backup.

## Estado atual

Os dois primeiros incrementos da Fase 1 implementam:

- diagnóstico somente leitura com `PRAGMA integrity_check` e `PRAGMA foreign_key_check`;
- verificação dos invariantes estruturais centrais;
- snapshot consistente pela Online Backup API do SQLite, inclusive com WAL ativo;
- cópia de assets externos quando existir o diretório local `assets`;
- manifesto versionado com versão do schema, versão do app, motivo, tamanhos e SHA-256;
- publicação por rename somente depois de banco, assets e manifesto estarem completos;
- listagem que ignora diretórios temporários interrompidos;
- validação de banco, manifesto, hashes e assets;
- criação e validação manual em `Configurações > Geral > Backup e integridade`;
- criação e validação obrigatórias antes de `downloadAndInstall` no updater;
- restauração preparada fora da base ativa, com rejeição de schema futuro;
- comparação dos checksums SQLx aplicados para impedir o retorno de migrations modificadas;
- backup `pre_restore` obrigatório antes de qualquer troca;
- encerramento explícito do pool SQLite do frontend;
- troca recuperável de banco e assets, com rollback automático em caso de falha;
- reinício do Tauri após a confirmação da troca;
- retenção dos cinco backups automáticos mais recentes, sem excluir backups manuais ou `pre_restore`.

Permanece pendente o gate completo de release: validar backup, atualização, restauração e reinício usando um instalador assinado publicado em ambiente de teste.

## Armazenamento

Os backups pertencem ao mesmo escopo local do aplicativo:

```text
<app-data>/
├── narrahub.db
├── assets/
├── backups/
│   └── 2026-08-25_180000_a1b2c3d4/
│       ├── manifest.json
│       ├── narrahub.db
│       └── assets/
└── recovery/
    └── restore-2026-08-25_183000_e5f6a7b8/
        ├── restore-rollback.json
        ├── narrahub.db
        └── assets/
```

Diretórios iniciados por `.tmp-` são staging incompleto. Eles nunca aparecem como backup restaurável. Para não interferir em outra instância do aplicativo, somente stagings com mais de 24 horas podem ser removidos automaticamente.

## Fluxo de criação

1. Impedir dois backups simultâneos no mesmo processo.
2. Abrir o banco de origem somente para leitura.
3. Gerar snapshot usando a API de backup do SQLite.
4. Executar `integrity_check` na cópia.
5. Copiar assets sem seguir links simbólicos.
6. Calcular hashes e tamanhos.
7. Gravar e sincronizar `manifest.json`.
8. Publicar o diretório completo por rename no mesmo volume.
9. Validar novamente quando solicitado pela interface.

Falha antes do passo 8 não publica um backup. O staging da própria tentativa é removido imediatamente; resíduos de encerramento abrupto ficam invisíveis e só são limpos depois da janela segura de 24 horas.

## Diagnóstico de domínio

Além das verificações do SQLite, o relatório detecta:

- capítulo sem livro;
- livro sem história;
- história ou entidade sem universo;
- relação sem uma das entidades;
- relação cruzando universos;
- menção sem capítulo ou entidade.

Problemas de domínio são reportados como avisos no snapshot, pois o backup precisa preservar uma base legada antes de qualquer reparo. Corrupção física, manifesto inválido ou hash divergente tornam o backup inválido.

## Fluxo de restauração

1. Validar manifesto, hashes, assets, integridade, versão do schema e checksums SQLx do backup escolhido.
2. Rejeitar snapshots criados por uma versão com schema mais novo que o aplicativo atual.
3. Criar e validar um backup `pre_restore` da base canônica atual.
4. Copiar o snapshot escolhido para `<app-data>/.restore-<token>` e validá-lo novamente.
5. Solicitar confirmação textual do usuário.
6. Bloquear a operação enquanto compartilhamento ou sincronização estiverem ativos.
7. Salvar o capítulo atual e encerrar o pool do `plugin-sql`.
8. Mover a base e os assets atuais para `recovery/restore-<id>`.
9. Instalar a cópia preparada e verificar novamente hash, schema e integridade.
10. Em qualquer falha da troca, remover a cópia parcial e recolocar os arquivos anteriores.
11. Reiniciar o processo Tauri; migrations append-only atualizam um snapshot antigo na abertura.

A preparação expira em 10 minutos. Stagings abandonados continuam invisíveis e só são removidos depois de 24 horas.

## Retenção

A retenção roda somente após o novo snapshot ser publicado e validado. A política inicial mantém os cinco backups automáticos mais recentes (`periodic`, `pre_update` e `pre_migration`). Backups `manual` e `pre_restore` são permanentes nesta fase e nunca entram na remoção automática.

Falha de retenção não invalida nem remove o snapshot recém-criado; ela é registrada no log nativo e a operação de backup continua válida.

## Comandos Tauri

| Comando | Função |
| --- | --- |
| `database_health` | Diagnostica o banco ativo sem alterar dados. |
| `backup_create` | Cria snapshot com motivo `manual`, `pre_migration`, `pre_update`, `pre_restore` ou `periodic`. |
| `backup_list` | Lista somente snapshots publicados com manifesto. |
| `backup_validate` | Recalcula hashes e executa diagnóstico na cópia. |
| `backup_restore_prepare` | Valida a origem, cria `pre_restore` e prepara uma cópia temporária com token de curta duração. |
| `backup_restore_commit` | Após o pool SQL ser fechado, troca os arquivos com rollback e exige reinício. |

## Testes

Os testes Rust cobrem:

- snapshot de escrita confirmada ainda presente no WAL;
- cópia e hash de asset real;
- corrupção posterior detectada pelo hash;
- staging interrompido ignorado e retenção segura entre processos;
- rejeição de traversal no identificador do backup;
- rejeição de backup com schema futuro;
- rejeição de migration aplicada com checksum diferente do código atual;
- round-trip de restauração de banco e assets;
- falha injetada depois da troca e restauração automática da base anterior;
- retenção limitada dos automáticos com preservação de `manual` e `pre_restore`;
- detecção de relação entre universos;
- cascade de relação ao excluir entidade;
- snapshot temporário de uma base desktop real informada explicitamente.

O teste real é opt-in e nunca grava na base informada:

```powershell
$env:NARRAHUB_REAL_DB = Join-Path $env:APPDATA 'com.narrahub.app.dev\narrahub.db'
cargo test --manifest-path src-tauri/Cargo.toml real_desktop_database_creates_a_valid_temporary_backup -- --ignored
```

## Próximo incremento

O runtime Tauri já foi exercitado duas vezes sobre uma cópia controlada da base instalada, incluindo migration 10→13, backup online, integridade e preservação das contagens. O relatório está em [`PHASE_0_1_QUALIFICATION.md`](PHASE_0_1_QUALIFICATION.md).

1. Publicar uma versão assinada de teste e executar upgrade real com `pre_update`.
2. Validar restauração e reinício por interação real no desktop empacotado.
3. Expor gerenciamento dos pontos técnicos de rollback sem confundi-los com backups do usuário.
