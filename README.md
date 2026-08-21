# NarraHub

Aplicativo local-first para escrever livros, organizar universos narrativos e sincronizar dispositivos pela rede local.

O NarraHub é distribuído como aplicativo Windows e Android com Tauri 2. A interface Angular é empacotada dentro do aplicativo; não existe servidor web externo ou navegador no uso de produção.

## Versão 0.6.0

Fluxos implementados:

- universo → história → livro → capítulo;
- editor visual Tiptap com autosave, formatação, imagens, contagem de palavras, modo foco e tela cheia;
- árvore de histórias, livros e capítulos com reordenação por arrastar;
- personagens, lugares, eventos, objetos, organizações e notas;
- atributos dinâmicos de entidades;
- fichas editáveis com imagem principal, galeria e atributos próprios de cada tipo;
- grafo Cytoscape navegável com zoom, filtros, formas por tipo e nós reposicionáveis;
- timeline persistida com datas reais ou fictícias e vínculo com eventos existentes;
- planejamento Kanban persistido, arrastável e capaz de reutilizar capítulos existentes;
- revisões automáticas de capítulos e histórico de alterações;
- temas claro, escuro e conforme o sistema;
- sincronização bidirecional por endereço local e código temporário;
- armazenamento SQLite independente em cada dispositivo;
- isolamento de coleções e respostas assíncronas por universo;
- layout responsivo para desktop e telas móveis;
- controles nativos de minimizar, maximizar/restaurar e fechar;
- links temporários de leitura com seleção de universo, capítulos e fichas;
- servidor efêmero embutido, dados cifrados somente em memória e Cloudflare Quick Tunnel incluído no instalador Windows;
- verificação e instalação de atualizações assinadas pelo updater do Tauri;
- pipeline manual de releases Windows no GitHub Actions;
- encerramento automático dos links ao fechar o aplicativo ou parar a sessão.

O estado inicial é vazio. O aplicativo não cria usuário, universo, personagem, métrica ou conteúdo de demonstração.

Para desenvolvimento Windows, use `npm run desktop:dev`. O inicializador reutiliza o cache Rust em `D:\DevTools\NarraHubTarget` quando o disco D está disponível e não ativa o updater sem uma configuração de release assinada.

## Arquitetura

```text
Angular 22
   ↓ comandos Tauri e serviços locais
Tauri 2 / Rust
   ↓
SQLite local
   ↕ rede Wi-Fi privada
SQLite de outro dispositivo
```

Consulte:

- [Arquitetura](docs/ARCHITECTURE.md)
- [Sincronização](docs/SYNC.md)
- [Distribuição](docs/DISTRIBUTION.md)
- [Compartilhamento online](docs/ONLINE_SHARING.md)
- [Atualizações](docs/UPDATES.md)

## Desenvolvimento

Pré-requisitos Windows:

- Node.js 22 ou 24 LTS;
- Rust `stable-x86_64-pc-windows-msvc`;
- Visual Studio Build Tools 2022 com `Desktop development with C++`;
- WebView2 Runtime.

```powershell
npm install
npm run desktop:dev
```

Validação:

```powershell
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

Instalador Windows:

```powershell
npm run desktop:build
```

Os instaladores NSIS/MSI são gerados em `src-tauri/target/release/bundle`.

## Android

Além dos pré-requisitos gerais, instale Android Studio, SDK Platform, Platform Tools, NDK, Build Tools e Command-line Tools. Configure `JAVA_HOME`, `ANDROID_HOME` e `NDK_HOME`.

```powershell
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
npm run android:init
npm run android:dev
npm run android:build
```

Para gerar um APK ARM64 instalável com assinatura de desenvolvimento:

```powershell
$env:CARGO_BUILD_JOBS = '1'
npm run android:apk
```

O limite de um job evita falha de memória no primeiro link Rust em máquinas com pouca RAM. O projeto desativa a compilação incremental Kotlin porque as crates podem estar no disco C enquanto o workspace e o Gradle ficam no D; sem esse ajuste, o compilador Kotlin recompila após falhar ao relativizar caminhos entre discos.

Na máquina de desenvolvimento atual, os componentes configuráveis ficam em `D:\DevTools`: Node 24 portátil, Android SDK/NDK, cache Gradle, Visual Studio Build Tools, temporários e targets Rust. Java e o toolchain Rust já existentes continuam em suas pastas de sistema no C.

O projeto deve ser validado em aparelho físico antes de uma publicação na Play Store.

## Segurança da sincronização

A sincronização 0.2 usa um código temporário de seis dígitos para autorizar uma sessão na rede local. Use somente em uma rede Wi-Fi privada. Não exponha a porta do NarraHub na internet.

O compartilhamento online 0.6 é um fluxo separado e somente para leitura. A seleção é cifrada antes de passar pelo túnel e a chave fica no fragmento do link. Isso não torna a sincronização Wi-Fi 0.2 criptografada.

Criptografia de transporte, descoberta mDNS e identidade persistente de dispositivos estão planejadas para a próxima revisão do protocolo. Até essa revisão, o aplicativo não apresenta a sincronização como segura para redes públicas.
