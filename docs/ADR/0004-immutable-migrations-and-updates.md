# ADR 0004 — Migrations imutáveis e atualização recuperável

## Contexto

O plugin SQL registra checksum de migration. Alterar uma migration aplicada impede a inicialização e pode deixar um executável antigo incompatível com um banco novo.

## Decisão

Migrations são append-only. Antes de mudança de esquema, o aplicativo cria backup consistente. Upgrade é validado sobre bancos das versões anteriores; downgrade usa restauração do backup, não tentativa de desfazer SQL automaticamente.

## Consequências

- Cada alteração de esquema recebe versão nova.
- Releases com migration exigem teste de instalador/updater.
- Executável incompatível bloqueia abertura em vez de modificar dados.
