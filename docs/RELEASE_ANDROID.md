# Release Android — APK assinado em toda release

Toda execução do workflow **Release Windows + Android** (`.github/workflows/release-windows.yml`)
publica os dois artefatos na mesma release do GitHub:

```text
instaladores Windows      (tauri-action, com o manifesto assinado do updater)
NarraHub-<versão>-Android.apk     assinado para distribuição
```

## Dois APKs, e nunca um no lugar do outro

| onde | APK | assinatura | serve para |
| --- | --- | --- | --- |
| CI de toda PR (`ci.yml`, job `Android`) | `narrahub-debug-apk` | chave de **debug** do Gradle | testar a PR num aparelho |
| Release (`release-windows.yml`, job `android`) | `NarraHub-<versão>-Android.apk` | keystore de **release** | distribuição |

O APK de debug da CI **não** é promovido a release em hipótese alguma. Sem assinatura de release,
o workflow de release falha no primeiro passo e nenhuma release é criada.

## O gate

```text
job android    exige os quatro segredos               faltou algum? para aqui
               constrói com a mesma action da CI
               exige APK *release* que NÃO seja o -unsigned
               apksigner verify                        não verifica? para aqui
job release    só roda se o android passou
               cria a release como RASCUNHO
               anexa o APK e confere nos assets da própria release
               só então publica
```

O Gradle sem assinatura gera `app-universal-release-unsigned.apk` **sem erro nenhum**, e esse
arquivo não instala em aparelho algum. É por isso que o gate procura o APK assinado pelo nome e
confere com `apksigner`, em vez de aceitar "algum `.apk` existe".

A release nasce rascunho e só é publicada depois de o APK estar nos assets: uma release visível
sem APK não pode acontecer.

## Segredos do GitHub

Em **Settings → Secrets and variables → Actions → New repository secret**:

| segredo | conteúdo |
| --- | --- |
| `ANDROID_KEYSTORE_BASE64` | o arquivo `.jks` inteiro, em base64 |
| `ANDROID_KEYSTORE_PASSWORD` | senha do keystore |
| `ANDROID_KEY_ALIAS` | alias da chave dentro do keystore |
| `ANDROID_KEY_PASSWORD` | senha da chave |

A pipeline decodifica o keystore para o diretório temporário do runner, escreve
`src-tauri/gen/android/keystore.properties` (ignorado pelo Git), constrói, e apaga os dois no fim —
inclusive se o build falhar.

## Criar o keystore (uma vez, fora do repositório)

> **O keystore nunca entra no Git.** Perder o keystore significa não poder mais publicar
> atualizações do mesmo aplicativo: aparelhos com o NarraHub instalado recusam um APK assinado
> por outra chave. Guarde o arquivo e as senhas num cofre, com cópia.

Com o JDK 17 instalado:

```bash
keytool -genkeypair -v -keystore narrahub-release.jks -alias narrahub -keyalg RSA -keysize 4096 -validity 10000
```

Para gerar o conteúdo do segredo `ANDROID_KEYSTORE_BASE64` no Windows (PowerShell):

```powershell
[Convert]::ToBase64String([IO.File]::ReadAllBytes("narrahub-release.jks")) | Set-Clipboard
```

O `.gitignore` da raiz recusa `*.jks`, `*.keystore` e `keystore.properties` como defesa extra.

## Estado atual

- **Assinatura de release: não configurada.** Não existia keystore Android no projeto nem nesta
  máquina. `D:\DevTools\NarraHubSigning\narrahub.key` é a chave **minisign do updater do Tauri** —
  outra coisa, e não serve para Android.
- Até os quatro segredos existirem, o workflow de release **falha no job `android`**, de propósito.
- O APK de debug continua saindo em toda PR, pela CI.

## Build local

```bash
npm run android:apk
```

APK de debug em `src-tauri/gen/android/app/build/outputs/apk/universal/debug/`. Precisa de JDK 17,
Android SDK e NDK; ver a fatia 0 em `docs/ETAPA_14_LEVANTAMENTO.md`.
