# Release Android — APK assinado, SHA-256 e atualização pelo app

Toda execução do workflow **Release Windows + Android** (`.github/workflows/release-windows.yml`)
publica na mesma release do GitHub:

```text
instaladores Windows              (tauri-action, com o manifesto assinado do updater)
NarraHub-Android.apk              assinado com o keystore de release
NarraHub-Android.apk.sha256       SHA-256 do APK, no formato do sha256sum
```

Os dois nomes do Android são **estáveis de propósito**: o app procura exatamente eles
(`src-tauri/src/application/atualizacao_android.rs`). A versão está na tag da release
(`app-v0.10.0`). O teste `tests/android-release.test.mjs` reprova se o workflow e o app passarem a
usar nomes diferentes.

## Dois APKs, e nunca um no lugar do outro

| onde | APK | assinatura | serve para |
| --- | --- | --- | --- |
| CI de toda PR (`ci.yml`, job `Android`) | artefato `narrahub-debug-apk` | chave de **debug** gerada no runner | ver a PR num aparelho |
| Release (`release-windows.yml`, job `android`) | `NarraHub-Android.apk` | keystore de **release** | distribuição e atualização |

> **APK de debug não atualiza nem é atualizado.** Os runners do GitHub geram uma chave de debug
> nova a cada execução, então dois APKs de debug de execuções diferentes têm assinaturas diferentes.
> E um aparelho com APK de debug recusa o de release pelo mesmo motivo. Para testar atualização,
> as duas versões precisam sair do workflow de release, assinadas com o mesmo keystore.

## O gate da release

```text
job android    exige os quatro segredos                          faltou algum? para
               contrato: nomes dos assets e ordem do versionCode  reprovou? para
               versionCode calculado da versão
               build de release, APK *não* -unsigned, apksigner verify
               SHA-256 do APK final, conferido com sha256sum --check
job release    só roda se o android passou
               cria a release como RASCUNHO (pré-release se a versão tiver sufixo)
               anexa APK e SHA-256, confere o SHA de novo e os dois nos assets
               só então publica
```

Se qualquer passo falhar, **a release não é publicada**: o APK e o SHA são anexados e conferidos
enquanto ela ainda é rascunho, e a publicação é o último passo.

## versionCode

O Android só instala por cima quando o `versionCode` não diminui. O cálculo padrão do Tauri
ignora o pré-release — `0.10.0-beta.1` e `0.10.0-beta.2` sairiam com o mesmo número —, então
`scripts/prepare-android-release-config.mjs` calcula:

```text
(maior·1.000.000 + menor·1.000 + patch) · 100 + sufixo
    alpha.N -> N        beta.N -> 32 + N        rc.N -> 64 + N        estável -> 99
```

Pré-releases aceitas: `alpha.N`, `beta.N`, `rc.N`, com N de 1 a 32.

## Canal: estável e beta

Versão com sufixo (`0.10.0-beta.1`) é publicada como **pré-release** no GitHub.

- O **atualizador do Windows** lê `releases/latest`, que ignora pré-release: quem usa a estável no
  desktop nunca recebe beta.
- O **atualizador do Android** aplica a mesma regra pela versão instalada: estável só recebe
  estável; beta recebe betas mais novas e, depois, a estável.

## Segredos do GitHub

Em **Settings → Secrets and variables → Actions → New repository secret**:

| segredo | conteúdo |
| --- | --- |
| `ANDROID_KEYSTORE_BASE64` | o arquivo `.jks` inteiro, em base64 |
| `ANDROID_KEYSTORE_PASSWORD` | senha do keystore |
| `ANDROID_KEY_ALIAS` | alias da chave dentro do keystore |
| `ANDROID_KEY_PASSWORD` | senha da chave |

**Os nomes estão corretos para o projeto real.** Eles só existem no workflow: o passo "Keystore a
partir dos segredos" os converte em `src-tauri/gen/android/keystore.properties` com as chaves
`storeFile`, `storePassword`, `keyAlias` e `keyPassword`, que são as que o
`app/build.gradle.kts` lê. O Gradle nunca vê os nomes dos segredos.

O keystore vai para o diretório temporário do runner e é apagado no fim, junto com o
`keystore.properties` — inclusive se o build falhar.

## Criar o keystore (uma vez, fora do repositório)

> **O keystore nunca entra no Git, e perdê-lo é definitivo.** Aparelhos com o NarraHub instalado
> recusam qualquer APK assinado por outra chave: sem o keystore, não há mais como atualizar quem já
> instalou. Guarde o arquivo e as duas senhas num cofre, com cópia.

Com o JDK 17 (o comando pede as senhas de forma interativa):

```bash
keytool -genkeypair -v -keystore narrahub-release.jks -alias narrahub -keyalg RSA -keysize 4096 -validity 10000
```

Base64 para o segredo `ANDROID_KEYSTORE_BASE64`, no PowerShell:

```powershell
[Convert]::ToBase64String([IO.File]::ReadAllBytes("narrahub-release.jks")) | Set-Clipboard
```

O `.gitignore` da raiz recusa `*.jks`, `*.keystore` e `keystore.properties`.

## Estado atual

- **Assinatura de release: não configurada.** Não havia keystore Android no projeto nem nesta
  máquina. `D:\DevTools\NarraHubSigning\narrahub.key` é a chave **minisign do updater do Tauri**,
  outra coisa, e não serve para Android.
- Até os quatro segredos existirem, o workflow de release **falha no job `android`**, de propósito.

## A atualização pelo app

Em Configurações → Atualizações, e também no aviso que aparece ao abrir o app:

```text
Versão atual: 0.10.0-beta.1   [Verificar atualizações]
Nova versão disponível 0.10.0-beta.2   [Atualizar agora] [Depois]
Baixando atualização... 42%
Atualização pronta. Abrir instalador?   [Abrir instalador]
```

1. `GET /repos/OsmarZM/NarraHub/releases` — a mais nova que esta instalação deve receber.
2. Só os dois assets de nome exato, e só com URL dentro de
   `https://github.com/OsmarZM/NarraHub/releases/download/<tag>/`.
3. Backup local validado antes de baixar — a mesma regra do desktop.
4. Baixa o `.sha256`, depois o APK, calculando o SHA durante o download. Não conferiu: apaga.
5. Antes do instalador: existe, tamanho > 0, SHA **de novo**.
6. Instalador do Android por `content://` do `FileProvider`. O usuário confirma.

Na primeira vez o Android pede que o NarraHub seja liberado em **"Instalar apps desconhecidos"**; o
app leva direto a essa tela.

A instalação por cima **preserva o diretório de dados do app** — banco SQLite, identidade do
aparelho, BlobStore e configurações. Nada no fluxo apaga `app_data`. O que decide se o APK é aceito
é o Android, pela assinatura.

## Build local

```bash
npm run android:apk
```

APK de debug em `src-tauri/gen/android/app/build/outputs/apk/universal/debug/`. Precisa de JDK 17,
Android SDK e NDK; ver a fatia 0 em `docs/ETAPA_14_LEVANTAMENTO.md`.
