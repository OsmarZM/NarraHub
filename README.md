# NarraHub

Aplicativo local-first para escrever livros, organizar universos narrativos e sincronizar dispositivos pela rede local.

O NarraHub é distribuído como aplicativo Windows e Android com Tauri 2. A interface Angular é empacotada dentro do aplicativo; não existe servidor web externo ou navegador no uso de produção.

## Versão 0.9.1

Fluxos implementados:

- universo → história → livro → capítulo;
- editor visual Tiptap com autosave, formatação, imagens, contagem de palavras, modo foco e tela cheia;
- árvore de histórias, livros e capítulos com reordenação por arrastar;
- personagens, lugares, eventos, objetos, organizações e notas;
- atributos dinâmicos de entidades;
- fichas editáveis com imagem principal, galeria e atributos próprios de cada tipo;
- grafo Cytoscape navegável com zoom, filtros, formas por tipo e nós reposicionáveis;
- canvas livre de Conexões com posição de nó persistida, elementos de título, imagem e nota, e ligações de diagrama separadas das relações canônicas;
- timeline persistida com datas reais ou fictícias e vínculo com eventos existentes;
- planejamento Kanban persistido, arrastável e capaz de reutilizar capítulos existentes;
- propriedades de planejamento com alcance explícito: universais para todos os cards ou restritas a um card, promovíveis nas duas direções sem perder valores;
- revisões automáticas de capítulos e histórico de alterações;
- temas claro, escuro e conforme o sistema;
- sincronização bidirecional por endereço local e código temporário;
- armazenamento SQLite independente em cada dispositivo;
- isolamento de coleções e respostas assíncronas por universo;
- layout responsivo para desktop e telas móveis;
- controles nativos de minimizar, maximizar/restaurar e fechar;
- carregamento sob demanda de cada seção do workspace por rota própria;
- exclusão segura de histórias, livros, capítulos, entidades e ligações;
- editor responsivo com largura de leitura, resumo recolhível e fichas fora da área de escrita;
- corretor ortográfico, autocomplete de personagens e vocabulário recorrente;
- avatares de personagens antes de menções e anotações por voz editáveis;
- comandos rápidos e atalhos de prompt personalizados no editor;
- IA opcional por API própria ou runtime local gerenciado e recomendado conforme o hardware;
- ações de IA sobre seleção, resumo de capítulo e criação/resumo de entidades;
- tags universais separadas dos campos próprios das fichas;
- sessões colaborativas com seleção de vários universos e permissões de leitura, anotação ou proposta de edição;
- revisão local com aprovação individual ou em lote antes de alterar o conteúdo canônico;
- servidor efêmero embutido, conteúdo e contribuições cifrados e Cloudflare Quick Tunnel incluído no instalador Windows;
- verificação e instalação de atualizações assinadas pelo updater do Tauri;
- pipeline manual de releases Windows no GitHub Actions;
- encerramento automático dos links ao fechar o aplicativo ou parar a sessão.

O estado inicial é vazio. O aplicativo não cria usuário, universo, personagem, métrica ou conteúdo de demonstração.

Para desenvolvimento Windows, use `npm run desktop:dev`. O inicializador reutiliza o cache Rust em `D:\DevTools\NarraHubTarget` quando o disco D está disponível e não ativa o updater sem uma configuração de release assinada.

## Arquitetura

```text
Angular 22
Componente → Feature Store → Gateway tipado → RustCoreService
   ↓ invoke()
Tauri 2 / Rust
interface/tauri → application → domain → repository
   ↓
SQLite local
   ↕ rede Wi-Fi privada
SQLite de outro dispositivo
```

Nenhum código Angular executa SQL nem conhece tabelas: o núcleo Rust é quem fala com o
banco. Isso é verificado por `npm run test:architecture`, não apenas combinado.

Consulte:

- [Arquitetura](docs/ARCHITECTURE.md)
- [Plano de evolução arquitetural](docs/ARCHITECTURE_EVOLUTION_PLAN.md)
- [Sincronização](docs/SYNC.md)
- [Distribuição](docs/DISTRIBUTION.md)
- [Compartilhamento online](docs/ONLINE_SHARING.md)
- [Assistência à escrita e IA](docs/WRITING_ASSISTANCE.md)
- [Planejamento em quadro e fichas](docs/PLANNING_CRM.md)
- [Compatibilidade aplicativo/schema](docs/APP_SCHEMA_COMPATIBILITY.md)
- [Atualizações](docs/UPDATES.md)
- [Histórico de versões](CHANGELOG.md)
- [Desenvolvimento seguro](docs/DEVELOPMENT.md)
- [Constituição dos agentes](AGENTS.md)
- [Estado corrente de engenharia](docs/ai/PROJECT_STATE.md)
- [Roadmap até a 1.0](docs/ai/ROADMAP.md)

Este README e [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) descrevem o **estado corrente** do produto. O histórico de versões vive no [CHANGELOG](CHANGELOG.md) e nas notas de release em `docs/RELEASE_*.md`; nenhum dos dois deve virar arquivo de arqueologia.

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

O compartilhamento online 0.7 é separado da sincronização Wi-Fi. Conteúdo, anotações e propostas são cifrados antes de passar pelo túnel e a chave fica no fragmento do link. Propostas recebidas só alteram o banco local depois da aprovação do autor. Isso não torna a sincronização Wi-Fi 0.2 criptografada.

Criptografia de transporte, descoberta mDNS e identidade persistente de dispositivos estão planejadas para a próxima revisão do protocolo. Até essa revisão, o aplicativo não apresenta a sincronização como segura para redes públicas.
