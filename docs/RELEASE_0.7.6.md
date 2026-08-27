# NarraHub 0.7.6

Esta versão introduz o novo **Design System Cósmico Glassmorphic**, a reformulação de painéis e menus em cartões flutuantes arredondados, a nova identidade visual oficial do aplicativo com ícones nativos e o desbloqueio completo de cliques e interações na barra de título do Windows.

---

## Novidades

### 1. Fundo Cósmico & Glassmorphism Translúcido
- **Arte de Nebulosa Cósmica**: Fundo estelar imersivo em alta resolução (`cosmic-nebula.jpg`).
- **Camadas de Estrelas Animadas**: Efeito de cintilação cósmica multi-camada (`stars-twinkle-1`, `2` e `3`) acelerado por hardware GPU em CSS puro, sem impacto em consumo de memória ou CPU.
- **Folha Celestial Iluminada**: Editor com cantoneiras ornamentadas, bússola estelar `✦` e moldura de luz translúcida.

### 2. Menus Suaves em Cartões de Vidro Flutuantes (18px)
- **Eliminação de Divisórias Rígidas**: Fim das linhas secas e verticais que cortavam a tela de fora a fora.
- **Sidebar de Navegação**: Flutua como um cartão translúcido com `border-radius: 18px` e acabamento em vidro com desfoque profundo (`backdrop-filter: blur(24px)`).
- **Árvore de Livros e Capítulos**: Painel flutuante arredondado integrado harmonicamente ao ambiente de escrita.
- **Inspetor Contextual**: Módulo flutuante direito com cantos suaves de 18px e transições elegantes.
- **Workspace Header em Cápsula**: Barra de breadcrumb e ações rápidas (`Universos / teste / capítulo`) transformada em cápsula de vidro flutuante com cantos arredondados de 14px.

### 3. Novas Logos Oficiais e Ícones Nativos do Desktop
- **Logo Horizontal na Barra de Título**: Marca estilizada com livro aberto cósmico e tipografia refinada no canto superior esquerdo com brilho estelar.
- **Ícones Multi-Tamanho Nativos do Windows**: Geração completa de ícones (`.ico`, `.png`, `32x32`, `128x128`, `Square...Logo`) a partir da arte cósmica oficial.
- **Atribuição Nativa no Bootstrap**: O Tauri agora atribui o ícone programaticamente a todas as janelas do Windows logo na inicialização (`win.set_icon(...)`), eliminando o ícone genérico do Angular na barra de tarefas.
- **Favicons com Cache-Busting**: Suporte a ícones de 32x32 e quebra de cache (`?v=2`) no `index.html`.

### 4. Responsividade e Desbloqueio da Barra Superior (Titlebar)
- **Região de Arrasto Segura**: Aplicação de `-webkit-app-region: no-drag` em todos os elementos interativos (busca global, alternador de tema claro/escuro, configurações e botões de minimizar/maximizar/fechar).
- **Cliques Imediatos**: O campo de busca e os controles de janela respondem no primeiro clique, sem serem interceptados pelo gerenciador de janelas do SO.

### 5. Legibilidade e Alto Contraste
- **Clareamento de Fontes Secundárias**: Otimização dos tokens de cor (`--nh-ink: #ffffff`, `--nh-ink-muted: #dfd3e7`) para leitura nítida e confortável sobre o tema cósmico escuro.
- **Tags e Metadados**: Badges de capítulos e entidades com bordas luminosas e contraste semântico reforçado.

### 6. Banco de Dados e Canvas
- **Migração v14 (SQLite)**: Tabelas `canvas_nodes` e `canvas_edges` preparadas para suportar mapas mentais e visualizações narrativas sem interferir no acervo de produção.

---

## Verificação e Qualidade

- **Validação de Consistência de Versão**: 0.7.6 sincronizada em `package.json`, `src-tauri/tauri.conf.json` e `src-tauri/Cargo.toml`.
- **40 Testes Rust Aprovados**: Cobertura de integridade referencial, migrações, restauração atômica e sync.
- **31 Testes de Frontend Aprovados**: Arquitetura modular, rotas lazy, painéis CRM e validação de UI de produção.

---

## Atualização

Instale ou atualize o aplicativo via atualizador automático assinado ou pelos pacotes `.msi` / `.exe` disponíveis no GitHub Releases da versão 0.7.6.
