# Desenvolvimento seguro

## Objetivo

Impedir que o NarraHub executado pelo código-fonte altere o banco usado pela versão instalada. Migrações de banco são progressivas: depois que uma versão nova atualiza o arquivo, um executável antigo pode se recusar a abri-lo.

## Perfis de dados

| Perfil | Identificador Tauri | Diretório de dados no Windows |
| --- | --- | --- |
| Desenvolvimento | `com.narrahub.app.dev` | `%APPDATA%\com.narrahub.app.dev` |
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
