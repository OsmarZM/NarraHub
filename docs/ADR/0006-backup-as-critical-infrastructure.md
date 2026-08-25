# ADR 0006 — Backup como infraestrutura crítica

## Contexto

No NarraHub, SQLite e arquivos locais são a fonte canônica. Uma migration defeituosa, falha de disco ou atualização interrompida pode afetar o único acervo do escritor. Copiar apenas o arquivo principal do SQLite também não produz necessariamente um snapshot consistente quando o banco usa WAL.

## Decisão

Backup e recuperação pertencem à camada de infraestrutura. O `BackupService` Rust cria snapshots autocontidos e imutáveis usando mecanismo consistente do SQLite, inclui assets e publica um manifesto versionado com hashes. Atualizações que alteram o schema exigem backup pré-migration verificado.

A restauração nunca ocorre diretamente sobre a base ativa. O snapshot é materializado em diretório temporário, seus hashes e manifestos são validados, e o banco passa por `PRAGMA integrity_check` e `PRAGMA foreign_key_check` antes da troca.

## Consequências

- Backup passa a ser gate de atualização e release.
- Snapshot parcial ou com hash inválido não pode ser oferecido para restauração.
- Retenção só remove backups automáticos antigos depois da confirmação de um backup novo válido; snapshots manuais e `pre_restore` ficam protegidos.
- Downgrade incompatível usa restauração de snapshot, não reversão improvisada de migration.
- Testes precisam cobrir WAL, interrupção, corrupção, restauração e reabertura no Tauri.
