# Qualificação local das fases 0 e 1

## Objetivo

Registrar evidências reproduzíveis de que a estabilização, as migrations, o runtime desktop e a rede de segurança local funcionam antes de publicar uma atualização. Este relatório não substitui o teste do instalador assinado.

## Escopo validado

Validação executada em 26/08/2026 na versão de desenvolvimento `0.7.4`:

- banco instalado de origem no schema 10;
- cópia isolada no identificador `com.narrahub.app.qualification`;
- aplicação real das migrations 11, 12 e 13 pelo plugin SQL do Tauri;
- dois boots completos do runtime sobre a cópia migrada;
- backup online durante o primeiro boot e depois do reinício;
- `integrity_check`, `foreign_key_check`, compatibilidade de migrations e hashes;
- comparação das contagens canônicas entre a origem e a cópia migrada;
- preservação comprovada do banco instalado por hash e data de modificação.

O perfil de qualificação usa `%APPDATA%\com.narrahub.app.qualification` e nunca deve apontar para `%APPDATA%\com.narrahub.app`.

## Evidências automatizadas

| Gate | Comando | Resultado |
| --- | --- | --- |
| Frontend | `npm run build` | aprovado; permanecem apenas avisos de orçamento do bundle e CSS |
| Pacote Windows | `npm run desktop:build` | NSIS e MSI locais gerados com sucesso |
| Testes Node | `node --test <todos os arquivos tests/*.test.mjs>` | 15 aprovados |
| Testes Rust | `cargo test --manifest-path src-tauri/Cargo.toml` | 38 aprovados e 1 opt-in ignorado por padrão |
| IA local real | `npm run validate:local-ai` | correção, reescrita e contexto longo aprovados |
| UI empacotável | `npm run release:validate-ui` | tema e configurações presentes no CSS de produção |
| Desktop | `npm run release:validate-desktop` | identificadores dev, produção e qualificação isolados |
| Versão | `npm run release:validate-version` | `0.7.4` consistente nos três manifests |
| Formatação | `git diff --check` | aprovado; somente avisos de normalização LF/CRLF |
| Banco real | teste opt-in `real_desktop_database_creates_a_valid_temporary_backup` | aprovado contra origem schema 10 e réplica schema 13 |

Os testes Rust incluem cadeia completa de migrations em arquivo reaberto, fixture representativa 10→13, WAL, assets, manifesto, corrupção, staging interrompido, retenção, schema futuro, checksum de migration, restauração completa e rollback após falha injetada.

O build local gerou os seguintes artefatos não assinados, úteis apenas para qualificação técnica:

| Artefato | Bytes | SHA-256 |
| --- | ---: | --- |
| `NarraHub_0.7.4_x64-setup.exe` | 20.159.952 | `0AB29C8DFED408CD497B5AF7DD95C9EFE49383361891FEEA49B48B4AC8CC3BEA` |
| `NarraHub_0.7.4_x64_en-US.msi` | 27.897.856 | `02B3C8A616BD48CA10F7AA61253C591AA445D567E897ABD14C304FC05C4983BA` |

O status Authenticode dos dois arquivos é `NotSigned`; portanto eles não são artefatos de release.

## Proteção do banco instalado

Antes e depois da qualificação, o arquivo `%APPDATA%\com.narrahub.app\narrahub.db` apresentou:

- SHA-256: `651FB05C170BCE71DF3FD0F619D9B03C85B03C1B80462AF8792F8284A97EF432`;
- tamanho: `6.307.840` bytes;
- última modificação UTC: `2026-08-25 20:41:21`.

A origem foi aberta somente para leitura pelos testes. Todas as migrations e escritas de runtime ocorreram na cópia de qualificação.

## Como repetir a qualificação do banco

Com o NarraHub fechado e o WAL vazio, crie a cópia isolada e execute:

```powershell
npm run desktop:qualification
```

Depois de confirmar o primeiro boot, encerre e execute o mesmo comando novamente. Para validar backup, integridade e preservação das contagens:

```powershell
$env:NARRAHUB_REAL_DB = Join-Path $env:APPDATA 'com.narrahub.app.qualification\narrahub.db'
$env:NARRAHUB_REFERENCE_DB = Join-Path $env:APPDATA 'com.narrahub.app\narrahub.db'
$env:NARRAHUB_EXPECTED_SCHEMA = '13'
cargo test --manifest-path src-tauri/Cargo.toml real_desktop_database_creates_a_valid_temporary_backup -- --ignored --nocapture
```

## Gate externo ainda obrigatório

`npm run release:validate-updater` bloqueou corretamente a publicação porque este ambiente não possui `TAURI_UPDATER_PUBLIC_KEY`, `TAURI_SIGNING_PRIVATE_KEY` e `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

Antes de chamar uma versão de publicável, ainda é obrigatório:

1. configurar as chaves no ambiente protegido do GitHub;
2. gerar e publicar o instalador assinado candidato;
3. instalar a versão pública anterior em uma máquina ou perfil Windows de teste;
4. criar dados e assets nessa versão;
5. atualizar pelo updater candidato;
6. validar backup `pre_update`, conteúdo, tema, configurações e dois reinícios;
7. executar uma restauração real pela interface e confirmar o reinício;
8. registrar os artefatos e resultados no relatório da release.

Até esse gate passar, as fases 0 e 1 estão concluídas no código e qualificadas localmente, mas a versão não está autorizada para publicação.
