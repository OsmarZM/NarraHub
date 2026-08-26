# Desenvolvimento seguro

## Objetivo

Impedir que o NarraHub executado pelo código-fonte altere o banco usado pela versão instalada. Migrações de banco são progressivas: depois que uma versão nova atualiza o arquivo, um executável antigo pode se recusar a abri-lo.

## Perfis de dados

| Perfil | Identificador Tauri | Diretório de dados no Windows |
| --- | --- | --- |
| Desenvolvimento | `com.narrahub.app.dev` | `%APPDATA%\com.narrahub.app.dev` |
| Qualificação de upgrade | `com.narrahub.app.qualification` | `%APPDATA%\com.narrahub.app.qualification` |
| Produção | `com.narrahub.app` | `%APPDATA%\com.narrahub.app` |

Os dois perfis podem usar o nome `narrahub.db` porque ficam em diretórios diferentes.

## Como iniciar

Use o comando da raiz do projeto:

```powershell
.\iniciar-desktop.bat
```

Ou, dentro de `narrahub-app`:

```powershell
npm run desktop:dev
```

O arquivo base `src-tauri/tauri.conf.json` é deliberadamente o perfil de desenvolvimento. Assim, até uma execução direta de `npm run tauri -- dev` permanece isolada.

## Builds

`npm run desktop:build` aplica `src-tauri/tauri.production.conf.json` explicitamente. O pipeline de release incorpora o mesmo perfil na configuração assinada do atualizador.

Antes de publicar, `npm run release:validate-desktop` confirma que os identificadores são diferentes. A validação falha se desenvolvimento e produção voltarem a compartilhar dados.

## Testes com dados próximos da produção

Não aponte o modo de desenvolvimento para o banco instalado. Faça uma cópia com o aplicativo fechado e coloque-a no diretório de desenvolvimento. Essa cópia pode receber migrações sem impedir que a versão instalada continue abrindo o banco original.

Nunca resolva uma incompatibilidade apagando `%APPDATA%\com.narrahub.app\narrahub.db`; esse arquivo contém o acervo local do usuário.

## Qualificação de upgrade

O perfil de qualificação existe para testar migrations, reinício e recuperação sem abrir a base canônica com o código em desenvolvimento.

1. Feche o NarraHub instalado e confirme que não existem arquivos `narrahub.db-wal` ou `narrahub.db-shm` ativos.
2. Remova ou arquive somente o diretório `%APPDATA%\com.narrahub.app.qualification` de uma execução anterior.
3. Copie o snapshot de teste para `%APPDATA%\com.narrahub.app.qualification\narrahub.db`.
4. Execute `npm run desktop:qualification`.
5. Confirme o upgrade, feche o aplicativo e execute o mesmo comando novamente para provar a reabertura.
6. Valide o banco resultante com `NARRAHUB_REAL_DB` apontando para a cópia de qualificação.

O título da janela contém `Qualification`, e o identificador próprio impede acesso acidental aos diretórios de desenvolvimento e produção.
