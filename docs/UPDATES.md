# Atualizações do aplicativo

## Contrato

Um `git push` altera apenas o código-fonte. O aplicativo instalado é atualizado somente quando existe uma release publicada com:

- versão SemVer superior à instalada;
- instalador e manifesto `latest.json` no GitHub Releases;
- assinatura gerada pela chave privada do NarraHub;
- assinatura validada pela chave pública embutida no aplicativo.

Ao iniciar, o aplicativo abre o banco local sem bloquear a interface e verifica o canal de atualização em segundo plano. Quando encontra uma versão superior, exibe um aviso global com as opções **Atualizar e reiniciar** e **Agora não**. Antes da instalação, qualquer capítulo pendente é salvo; se o salvamento falhar, a atualização é interrompida.

A assinatura do updater é obrigatória e não substitui a assinatura Authenticode do instalador Windows.

## Compatibilidade do banco

Uma atualização que contém migration deve criar um backup consistente antes de alterar o esquema e passar pela matriz de upgrade descrita em [`ARCHITECTURE_EVOLUTION_PLAN.md`](ARCHITECTURE_EVOLUTION_PLAN.md). Migrations aplicadas são imutáveis: corrigir uma versão existente altera seu checksum e impede a inicialização.

O contrato suportado é upgrade progressivo. Downgrade de executável depois de uma mudança de esquema não é tratado como rollback seguro; a recuperação usa o backup anterior à migration. Um executável que não entende a versão do banco deve bloquear a abertura com diagnóstico, nunca tentar apagar ou reconstruir o acervo.

O backup pré-migration deve ser publicado como snapshot imutável com manifesto, versão do aplicativo, versão do schema e hashes do banco/assets. A restauração acontece primeiro em diretório temporário e só substitui a base ativa depois de `integrity_check`, `foreign_key_check` e validação dos hashes.

O cliente cria e valida um backup com motivo `pre_update` antes de chamar `downloadAndInstall`. Se o snapshot, manifesto ou hash falhar, a instalação é bloqueada e o executável atual continua ativo.

## Primeira configuração

Gere o par de chaves fora do repositório, preferencialmente no disco D:

```powershell
New-Item -ItemType Directory -Force -Path 'D:\DevTools\NarraHubSigning'
npm run tauri signer generate -- -w 'D:\DevTools\NarraHubSigning\narrahub.key'
```

Configure no repositório GitHub:

- variável `TAURI_UPDATER_PUBLIC_KEY`: conteúdo integral da chave pública;
- secret `TAURI_SIGNING_PRIVATE_KEY`: conteúdo integral da chave privada;
- secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: senha definida na geração.

Nunca salve a chave privada ou a senha no Git, em `.env` ou no instalador.

## Publicação

1. Atualize a versão em `package.json`, `src-tauri/Cargo.toml` e `src-tauri/tauri.conf.json`.
2. Execute `npm run release:validate-version`.
3. Execute os gates de código, banco, runtime Tauri e upgrade sobre a versão anterior.
4. No GitHub Actions, execute manualmente **Release Windows**.
5. O workflow testa, compila, assina e publica a release.
6. Confirme no GitHub que NSIS, MSI, assinaturas e `latest.json` foram anexados.
7. Valide que a release aparece em `/releases/latest`; somente então a atualização está disponível aos usuários.

Regra operacional: compilar não comprova funcionamento; funcionamento não comprova segurança; segurança local não comprova que a versão foi publicada. O gate completo inclui build, testes, migration, runtime Tauri, upgrade real, reinício, recuperação e verificação da release remota.

O workflow gera temporariamente `src-tauri/tauri.release.conf.json`; o arquivo é ignorado pelo Git e recebe a chave pública pela variável protegida.

## Primeira instalação com updater

Versões antigas que foram compiladas sem endpoint e chave pública não conseguem descobrir o updater retroativamente. Por isso, o primeiro instalador gerado por esta pipeline deve ser baixado e instalado manualmente. A partir dele, versões SemVer superiores publicadas no mesmo canal passam a ser detectadas pelo inicializador.

No modo `tauri dev`, o plugin não é registrado porque a configuração assinada de release não existe. Isso evita que o ambiente de desenvolvimento tente desserializar um updater nulo. O comando `updater_configured` informa ao frontend se o canal está realmente disponível antes de qualquer verificação.

Referências oficiais: [Tauri Updater](https://v2.tauri.app/plugin/updater/) e [pipeline GitHub do Tauri](https://v2.tauri.app/distribute/pipelines/github/).
