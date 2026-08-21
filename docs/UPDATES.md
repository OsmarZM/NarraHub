# Atualizações do aplicativo

## Contrato

Um `git push` altera apenas o código-fonte. O aplicativo instalado é atualizado somente quando existe uma release publicada com:

- versão SemVer superior à instalada;
- instalador e manifesto `latest.json` no GitHub Releases;
- assinatura gerada pela chave privada do NarraHub;
- assinatura validada pela chave pública embutida no aplicativo.

Ao iniciar, o aplicativo abre o banco local sem bloquear a interface e verifica o canal de atualização em segundo plano. Quando encontra uma versão superior, exibe um aviso global com as opções **Atualizar e reiniciar** e **Agora não**. Antes da instalação, qualquer capítulo pendente é salvo; se o salvamento falhar, a atualização é interrompida.

A assinatura do updater é obrigatória e não substitui a assinatura Authenticode do instalador Windows.

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
3. No GitHub Actions, execute manualmente **Release Windows**.
4. O workflow testa, compila, assina e cria uma release em rascunho.
5. Instale e valide o artefato em ambiente limpo.
6. Publique a release. Somente releases publicadas aparecem em `/releases/latest`.

O workflow gera temporariamente `src-tauri/tauri.release.conf.json`; o arquivo é ignorado pelo Git e recebe a chave pública pela variável protegida.

## Primeira instalação com updater

Versões antigas que foram compiladas sem endpoint e chave pública não conseguem descobrir o updater retroativamente. Por isso, o primeiro instalador gerado por esta pipeline deve ser baixado e instalado manualmente. A partir dele, versões SemVer superiores publicadas no mesmo canal passam a ser detectadas pelo inicializador.

Referências oficiais: [Tauri Updater](https://v2.tauri.app/plugin/updater/) e [pipeline GitHub do Tauri](https://v2.tauri.app/distribute/pipelines/github/).
