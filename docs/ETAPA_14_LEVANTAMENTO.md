# Etapa 14 — Windows ↔ Android, ponta a ponta: levantamento

> Documento de **levantamento e plano**, escrito antes de qualquer implementação, a pedido da
> revisão que fechou a etapa 13. Ele descreve o que o repositório **tem hoje** (2026-09-11,
> `main` em `95f53a6`), não o que se deseja. Onde o desejo aparece, está marcado como decisão
> pendente.

A etapa 14 é a última da ordem do ADR 0009 §23. As treze anteriores estão entregues e
mescladas. O que segue é o resultado de ler o código, não de consultar as etapas anteriores.

---

## 1. O achado que redefine o tamanho da etapa

**O Sync V2 não tem nenhuma porta para o frontend.**

`src-tauri/src/lib.rs` registra **108 comandos** no `invoke_handler`. Nenhum deles é do Sync
V2: não há comando de pareamento, de troca, de roster, de bootstrap, de snapshot ou de
dispositivo. A busca por `pair|pake|bootstrap|roster|trust|exchange|snapshot|device|v2` na
lista de comandos volta vazia.

O que o aplicativo chama hoje quando o usuário aperta "sincronizar" é **o Sync V1**:

```text
src/app/core/native/sync.service.ts      invoke('sync_status' | 'sync_start' | 'sync_stop' | 'sync_connect')
                                          ↓
src-tauri/src/sync.rs                    TcpListener + UdpSocket + código de pareamento próprio,
                                          copiando 17 tabelas inteiras
```

Ou seja: treze etapas construíram o motor — log de eventos, outbox transacional, causalidade,
cursor, relay, roster Ed25519, Noise `XX`, pareamento por QR, PAKE, tombstones, bootstrap por
snapshot, assets por SHA-256 — e **nada disso está ligado a um botão nem a um socket**. É a
mesma classe de lacuna que a revisão da etapa 2.5 apontou em `load_or_create` e que a revisão
da etapa 13 apontou no backfill sem chamador, uma ordem de magnitude maior.

**Consequência para o planejamento:** "Windows ↔ Android ponta a ponta" não é uma etapa de
empacotamento nem de teste em dois aparelhos. Ela contém três trabalhos que não existem em
nenhum lugar do repositório — transporte real, descoberta e superfície de IPC/UI — mais a
decisão do destino do V1.

---

## 2. O que existe, verificado arquivo por arquivo

### 2.1 O motor do Sync V2 — completo e sem consumidor

| módulo | linhas | o que faz |
| --- | --- | --- |
| `infrastructure/sync_transport.rs` | 543 | Noise `XX` e o vínculo `h` assinado em Ed25519 → `SessaoAutenticada` |
| `infrastructure/sync_pairing.rs` | 636 | convite de pareamento aleatório, expirável, de uso único |
| `infrastructure/sync_pake.rs` | 757 | PIN por SPAKE2, sem dicionário offline, três tentativas |
| `infrastructure/sqlite/sync_*.rs` | — | log, aplicação, cursor, troca, sessão, confiança, GC, snapshot |
| `infrastructure/blob_store.rs` + `blob_backfill.rs` | — | assets por SHA-256 e o manifesto do bootstrap |

Tudo isso é exercitado por ~520 testes que passam. **Nenhum deles atravessa um socket:** a
busca por `TcpListener`, `TcpStream`, `UdpSocket` e `bind(` nos módulos do V2 volta vazia. O
Noise fala com buffers em memória, e os "três aparelhos" da etapa 6 são três bancos no mesmo
processo.

### 2.2 O Sync V1 — tem socket, tem descoberta, e é o que está no ar

`src-tauri/src/sync.rs` usa `TcpListener`, `TcpStream`, `UdpSocket` e `ToSocketAddrs`, mantém
`SyncState` com código de pareamento, e replica uma lista fixa de 17 tabelas
(`const TABLES`). É o único caminho de sincronização que o usuário alcança.

### 2.3 Android — o projeto existe e nunca foi exercido no CI

```text
src-tauri/gen/android/          42 arquivos versionados
  namespace / applicationId     com.narrahub.app
  compileSdk / targetSdk         36
  minSdk                         24
  usesCleartextTraffic           false
  AndroidManifest.xml            <uses-permission android.permission.INTERNET />
```

Os scripts existem: `android:init`, `android:dev`, `android:build`, `android:apk`,
`android:release:apk`. A toolchain está instalada nesta máquina — os quatro alvos
(`aarch64-linux-android` e os outros três), `ANDROID_HOME` e `NDK_HOME` apontando para o NDK
27.0.12077973.

O `lib.rs` já tem `#[cfg_attr(mobile, tauri::mobile_entry_point)]` e dois blocos
`#[cfg(desktop)]`, e o `Cargo.toml` restringe `tauri-plugin-updater` a
`cfg(any(target_os = "macos", windows, target_os = "linux"))`. O código específico de Windows
em `local_ai.rs` está isolado por `#[cfg(windows)]`.

**O que não existe:** nenhum job de CI para Android. `.github/workflows/` tem `ci.yml`
(Ubuntu: Angular + core Rust) e `release-windows.yml` (`workflow_dispatch`). Nunca houve APK
construído por pipeline — e a medição da fatia 0 mostrou que **hoje o alvo não compila**, por
uma capability que pede `updater:default` numa plataforma onde o plugin não existe. Detalhe em
§4, fatia 0.

### 2.4 Caminhos de dados — portáveis por construção

`database::app_data_path` usa `app.path().app_data_dir()`, que o Tauri resolve por plataforma,
e o blob store deriva tudo de `app_data/assets/blobs/sha256/<ab>/<hash>`. A identidade fica em
arquivo no diretório de dados, fora do banco. Nenhum caminho absoluto é persistido — foi
justamente o que a etapa 13 fechou, e é o que faz o mesmo documento HTML valer nos dois
sistemas.

---

## 3. O que falta, com o motivo

### 3.1 Transporte real

O `SessaoAutenticada` nasce de um handshake sobre buffers. Falta a camada que:

- abre e aceita conexão;
- **enquadra** as mensagens — o Noise tem limite de 65535 bytes por mensagem, e o bundle da
  etapa 12 e os blobs da etapa 13 passam disso com folga;
- impõe tempo limite e tamanho máximo, para um peer lento ou hostil não prender o aparelho;
- propaga erro de rede como erro de domínio, sem inventar sucesso parcial.

### 3.2 Descoberta na rede local

O V1 usa broadcast UDP. No Android isso não é simétrico: multicast/broadcast tem restrição por
versão e por fabricante, e a rede pode isolar clientes (`AP isolation`). Decisão pendente
entre mDNS, broadcast UDP com *fallback*, e endereço digitado — com a observação de que o QR
da etapa 9 pode carregar o endereço, o que reduz a descoberta a um caso de conveniência.

### 3.3 A ponte de IPC e a interface

Nenhum comando, nenhuma tela. O pareamento da etapa 9 é QR: o Windows precisa **exibir** e o
Android precisa **ler** — e não há dependência de câmera nem de leitura de código no
`package.json`. O PIN da etapa 10 é a alternativa que não precisa de câmera, e por isso é o
caminho mais curto para o primeiro E2E.

### 3.4 O destino do V1

Três saídas possíveis, e a escolha é do humano:

| saída | consequência |
| --- | --- |
| **substituir** | um caminho só; exige que o V2 cubra tudo que o usuário já faz hoje |
| **conviver** | dois botões, duas semânticas; risco de um acervo sincronizado pelos dois |
| **congelar** | V1 continua funcionando e não recebe nada; V2 entra como recurso novo |

Convivência é a que mais preocupa: o V1 copia tabelas inteiras sem passar pelo log de eventos,
e o V2 decide por causalidade. Um acervo tocado pelos dois teria estado que nenhum dos dois
explica.

---

## 4. Plano executável

Cada fatia é uma PR com gate próprio, na ordem em que uma destrava a seguinte.

### As quatro decisões — fechadas em 2026-09-11

```text
1  Sync V1        congelar e substituir. NAO coexistir.
                  Fica no codigo durante a etapa 14 para referencia e rollback,
                  e deixa o fluxo de produto quando o E2E do V2 fechar.
                  Nenhuma interoperabilidade V1<->V2 e construida.
                  Nenhum refactor no V1: sem framing, sem Noise, sem blob.

2  Descoberta     nenhuma, na primeira versao. IP/endereco + porta + PIN.
                  mDNS, multicast e broadcast UDP ficam para a fatia 5, depois
                  de o protocolo estar provado.

3  Pareamento     PIN primeiro. QR vira outra forma de transportar os MESMOS
                  dados, sem inventar outro protocolo. O material de conexao
                  precisa ser representavel como
                  { endpoint, identidade publica do aparelho, info de sessao,
                    PIN } desde agora.

4  E2E            nao fecha com "um capitulo atravessou". Ver a fatia 4.
```

O que cada decisão elimina do trabalho: a 2 tira rede multicast, permissão extra
de Android e diferença entre roteadores do caminho crítico; a 3 tira câmera e
plugin; a 1 tira a pior classe de defeito possível, que seria um acervo com
escritas de dois mecanismos e origem inexplicável.

### Fatia 0 — **Fechada.** O alvo compila, e o CI passou a provar isso

Executado em 2026-09-11, `cargo check --lib --target aarch64-linux-android`, duas vezes.

**Primeira tentativa** — falha em `aws-lc-sys v0.44.0`:

```text
error occurred in cc-rs: failed to find tool "clang.exe": program not found
```

Diagnóstico, e não conclusão: o `clang.exe` **existe** no NDK 27
(`toolchains/llvm/prebuilt/windows-x86_64/bin/clang.exe`), só não está no `PATH`. O
`cargo check` cru não configura o que o `tauri android build` configura, então esta falha é
do meu comando, não do projeto. `aws-lc-sys` entra por
`reqwest`/`hyper-rustls`/`rustls` → `aws-lc-rs`, tanto pelo `tauri-plugin-http` quanto pelo
`reqwest` direto.

**Segunda tentativa**, com o `bin` do NDK no `PATH` — `aws-lc-sys` compila, e a falha passa a
ser do próprio crate:

```text
error: failed to run custom build command for `narrahub v0.9.2`
  Permission updater:default not found, expected one of core:default, …
```

**Este é o bloqueador real, e é de configuração.** `src-tauri/capabilities/default.json`
(`identifier: main-capability`) declara `updater:default` e **não** declara `platforms`. O
`Cargo.toml` restringe `tauri-plugin-updater` a
`cfg(any(target_os = "macos", windows, target_os = "linux"))`, então no Android o plugin não
existe, a permissão não existe, e o `build.rs` do Tauri para antes de qualquer verificação de
tipo. Nada do código do NarraHub foi rejeitado — o build nunca chegou lá.

**A correção.** A permissão saiu da capability comum e foi para
`src-tauri/capabilities/updater-desktop.json`, com `"platforms": ["windows", "macOS", "linux"]`
— arquivo próprio para a restrição ficar ao lado da permissão em vez de escondida numa lista
de quinze. Depois disso:

```text
cargo check  --lib --target aarch64-linux-android      0 erros, 1 aviso
cargo clippy --lib --target aarch64-linux-android -D warnings   limpo
cargo clippy --all-targets -D warnings (desktop)                limpo
cargo fmt --check                                               limpo
```

O aviso era `unused variable: app` no `setup` do `lib.rs`: os dois blocos de lá são
`cfg(desktop)` — updater e ícone de janela —, então no Android `app` não é usado por nada. Com
`clippy -D warnings` no CI isso seria erro, e ficou explícito com um `let _ = &app;` comentado,
que diz "a ausência de uso é por plataforma", em vez de renomear para `_app` e perder a
informação no desktop.

**Nenhuma dependência nativa precisou mudar.** `aws-lc-sys`, `rusqlite`, `sqlx`,
`x25519-dalek`, `lol_html`, `spake2`, `zip` e `sysinfo` atravessaram a verificação para
`aarch64-linux-android` sem ajuste. A arquitetura de sync não foi tocada.

**O job de CI `Android`** (`.github/workflows/ci.yml`) faz o que o `cargo check` cru não faz:
`npm ci` → `npm run build` (o `frontendDist` aponta para `dist/narrahub-app/browser`) →
`clippy` para o alvo → **`npm run android:apk`** → o APK sobe como artefato com
`if-no-files-found: error`. Passar pelo Gradle e pelo manifest importa porque foi exatamente aí
que o alvo estava quebrado: a resolução de capability por plataforma não acontece no
`cargo check`.

O `NDK_HOME` do job aponta para o `ANDROID_NDK_LATEST_HOME` que o runner já traz, e o `clang`
do NDK entra no `PATH` porque `aws-lc-sys` compila C para o alvo.

#### O segundo bloqueador, que só o Gradle revela

Com o core compilando, `npm run android:apk` parou em outro lugar:

```text
Error Project directory …\gen/android\app/src/main\java/com/narrahub/app/dev
      does not exist. … delete the `gen/android` folder and run `tauri android init`
```

O `identifier` do `tauri.conf.json` é `com.narrahub.app.dev` — sufixo deliberado, para o app de
desenvolvimento do desktop ter diretório de dados próprio e não pisar no instalado. No Android
o identifier decide o **pacote Java** que o Tauri procura, e o projeto versionado em
`gen/android` foi gerado como `com.narrahub.app`, igual ao `applicationId` do
`build.gradle.kts`.

Seguir a sugestão da mensagem apagaria 42 arquivos versionados por causa de um sufixo. A
correção é uma linha em `src-tauri/tauri.android.conf.json`, que o Tauri **mescla
automaticamente** em qualquer build de Android:

```json
{ "identifier": "com.narrahub.app" }
```

A explicação não cabe dentro do arquivo: o schema do Tauri recusa propriedade desconhecida
(`Additional properties are not allowed ('_comentario_identifier' was unexpected)`), e a
primeira tentativa de comentar ali falhou por isso. Por isso o motivo está aqui.

Fica um aviso pré-existente, do projeto gerado, que **não** foi mexido porque mudá-lo
regeneraria `gen/android`: o Tauri recomenda não terminar o identifier em `.app`, por
conflitar com a extensão de bundle do macOS. Não afeta Windows nem Android.

### Fatia 1 — Transporte com enquadramento

Camada de socket com enquadramento por tamanho, teto explícito, tempo limite, e um teste que
faz dois `SessaoAutenticada` conversarem por **`TcpListener` de verdade** em `127.0.0.1`.

> Gate: mensagem maior que o limite do Noise atravessa íntegra; mensagem maior que o teto é
> recusada sem alocar; peer que abre e não fala é derrubado pelo tempo limite.

### Fatia 2 — A ponte de IPC

Comandos para: estado do dispositivo, iniciar/parar a escuta, iniciar pareamento por PIN,
responder pareamento, sincronizar com um peer, ler pendências. Nenhum caminho de arquivo e
nenhum segredo atravessando a fronteira — a regra que a etapa 13 já aplicou aos blobs.

> Gate: um comando por caminho do motor, e a prova de que `device_id` não é parâmetro de
> entrada em nenhum deles. Quem fala não escolhe quem é.

### Fatia 3 — Pareamento por PIN na interface

A tela mínima nos dois sistemas: mostrar o PIN num lado, digitar no outro, ver o resultado.
QR fica para a fatia 5, porque depende de câmera.

### Fatia 4 — O E2E, e o escopo é obrigatório

Três cenários, e a etapa **não** fecha com menos:

```text
1  BOOTSTRAP                Windows A com acervo  ->  Android B novo
   atravessa:               identidade/roster conforme o contrato do V2
                            snapshot
                            baseline e vetores necessarios
                            capitulo
                            imagem referenciada
                            blob fisico da imagem
   no Android:              texto aparece
                            imagem aparece
                            hash do blob confere

2  INCREMENTAL B -> A       editar o capitulo no Android
                            evento V2  ->  Windows converge

3  INCREMENTAL A -> B       nova alteracao no Windows
                            evento V2  ->  Android converge
```

Só com os três é "Windows ↔ Android E2E comprovado". Um capítulo atravessando prova transporte;
o que se quer provar é **convergência nas duas direções sobre um acervo real**, com o contrato
de blobs da etapa 13 incluído.

> Gate: o roteiro escrito e executado, com evidência — hash conferido no aparelho de destino,
> não "a imagem apareceu". É o único gate da etapa que não é automatizável hoje, e isso fica
> declarado em vez de disfarçado.

### Fatia 5 — QR e descoberta, como UX sobre protocolo provado

**Não muda o protocolo provado na fatia 4.** O QR serializa o mesmo material de conexão que o
PIN já usa, e a descoberta automática substitui a digitação do endereço — nada além disso. Se
alterar mensagem, handshake ou ordem, deixou de ser esta fatia.

---

## 5. Decisões — respondidas, mantidas aqui pelo motivo

Respondidas em 2026-09-11 (resumo em §4). O registro do que foi perguntado fica porque o
motivo de cada escolha é o que evita reabrir a discussão em três meses.

1. **Destino do V1** → **congelar e substituir.** Não coexistir. O motivo é o pior defeito
   possível: um acervo com parte das escritas vindas do snapshot/LWW do V1 e parte da
   causalidade do V2 teria estado cuja origem o V2 não consegue explicar. O V1 fica no código
   durante a etapa para referência e rollback, e **não** recebe refactor — sem framing novo,
   sem Noise, sem blob, sem adaptação. Reaproveitar dele só o que for genérico de verdade e
   não carregar semântica V1.
2. **Descoberta** → **nenhuma agora.** IP/endereço + porta + PIN. Provar o transporte antes de
   envolver multicast, permissão de Android e diferença entre roteadores.
3. **Primeiro pareamento** → **PIN.** Sem câmera, sem plugin, testável nos dois sistemas
   imediatamente, e separa autenticação/Noise/transporte da UX.
4. **Escopo do E2E** → os três cenários da fatia 4, com blob e convergência bidirecional.

### Critério de parada, a partir daqui

Parar para decisão **somente** se algo exigir alterar: modelo causal do V2, identidade/roster,
handshake criptográfico, bootstrap da etapa 12, contrato de blobs da etapa 13, ou o modelo de
confiança entre aparelhos. Framing, comandos Tauri, organização de tarefa assíncrona, formato
de mensagem e divisão de módulos: decidir e seguir.
