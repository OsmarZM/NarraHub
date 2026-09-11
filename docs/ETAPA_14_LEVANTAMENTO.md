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
construído por pipeline, e portanto nenhuma garantia de que o alvo compila hoje.

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

### Fatia 0 — O alvo compila? *(sem código de produto)*

`cargo check --lib --target aarch64-linux-android`, depois `npm run android:apk`. Resultado
factual antes de qualquer plano: se o core não compila para Android, a etapa começa por aí.
Adicionar um job de CI `Android` que construa o APK de debug — sem ele, toda garantia de
portabilidade continua sendo afirmação.

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

### Fatia 4 — O primeiro E2E de verdade

Windows e Android na mesma rede, pareados por PIN, um capítulo criado num lado aparecendo no
outro — **incluindo uma imagem**, que é o que a etapa 13 tornou possível verificar: o mesmo
HTML com `data-narrahub-blob`, o blob transferido e verificado, e o `<img>` resolvido em
execução no aparelho de destino.

> Gate: o roteiro E2E escrito e executado, com evidência. É o único gate da etapa que não é
> automatizável hoje, e isso precisa estar declarado em vez de disfarçado.

### Fatia 5 — Descoberta e QR *(pode virar etapa 15)*

Depende da decisão de §3.2 e de uma dependência de câmera. Candidata a sair do escopo se a
fatia 4 fechar com endereço digitado.

---

## 5. Decisões que preciso do humano antes da fatia 2

1. **Destino do V1** — substituir, conviver ou congelar (§3.4).
2. **Descoberta** — mDNS, broadcast, ou só endereço/QR nesta etapa (§3.2).
3. **Primeiro pareamento** — PIN (sem câmera, mais curto) ou QR (precisa de câmera).
4. **Escopo do E2E da fatia 4** — capítulo com imagem já cobre o essencial, ou a etapa só fecha
   com bootstrap completo de acervo entre os dois aparelhos.

Nada aqui está implementado. A fatia 0 é a única que pode andar sem essas respostas, porque ela
não decide nada: só mede se o alvo compila.
