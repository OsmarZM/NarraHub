# Histórico de versões

As alterações relevantes do NarraHub são registradas neste arquivo. O projeto segue versionamento semântico: versões menores adicionam funcionalidades compatíveis e versões de correção tratam falhas sem alterar o fluxo principal.

## 0.10.0-beta.6 — 2026-09-25 (pré-release)

Pré-release só Android, correções achadas na qualificação física do Sync V2. Instala por cima da 0.10.0-beta.5.

- **Atualização no celular**: o app volta a procurar a beta nova sozinho ao abrir.
- **Conflito entre decisões legível**: mostra o capítulo e o texto que cada decisão produz, em vez dos dados
  internos da decisão.
- **Decisão com confirmação**: escolher uma versão pede um segundo toque, porque a decisão vale para os dois
  aparelhos e não pode ser desfeita.

## 0.10.0-beta.5 — 2026-09-24 (pré-release)

Pré-release só Android, correções achadas na qualificação física do Sync V2. Instala por cima da 0.10.0-beta.4.

- **Conflitos legíveis**: a tela rola, o texto aparece lado a lado no computador e, no celular, só as linhas
  que mudaram, uma seguida da outra. Sem identificadores internos à mostra. "Copiar texto" e "… e abrir para
  editar": escolha a versão mais próxima e ajuste no editor.
- **Pareamento mais claro**: a tela avisa quando o código venceu, e um código vencido ou errado mostra o que
  fazer em vez de uma mensagem técnica.

## 0.10.0-beta.4 — 2026-09-24 (pré-release)

Pré-release só Android, correção achada na qualificação física do Sync V2. Instala por cima da 0.10.0-beta.3.

- **Sincronização visível**: ao fim de cada sessão, dos dois lados, aparece o resumo (alterações recebidas
  e enviadas) e a biblioteca é relida na hora. Antes, o aparelho que escutava só mostrava o que chegou
  depois de reabrir o app.

## 0.10.0-beta.3 — 2026-09-24 (pré-release)

Pré-release só Android, para a qualificação física do Sync V2 (Etapa I). Instala por cima da 0.10.0-beta.2.

### Sincronização V2

- Sincronização por eventos assinados entre aparelhos pareados, com semeadura inicial de aparelho novo,
  encaminhamento por um aparelho intermediário e conflitos que nunca apagam uma versão.
- Resolução de conflitos propagada como decisão; decisões concorrentes viram novo conflito.
- O Sync V1 saiu do runtime; conflitos antigos ficam numa caixa de versões antigas para recuperação.

## 0.10.0-beta.2 — 2026-09-15 (pré-release)

Pré-release só Android, para teste. Instala por cima da 0.10.0-beta.1 e pode ser recebida pela atualização do
próprio app.

### Um aplicativo de celular de verdade

- **Sem zoom** por pinça ou toque duplo na interface.
- **Barra de cima própria**: voltar, busca e "•••" com as ações do universo. Sem barra lateral nem
  linha de botões espremida.
- **Navegação gestual mais fácil de achar**: três barras douradas na borda direita, dica na primeira
  vez, abre por toque, arrasto ou peteleco; vibração leve.
- **Escrita**: o editor ocupa a tela; capítulos e resumo abrem em folhas; cada item da árvore tem "⋯".
- **Diálogos** sobem de baixo e continuam acima do teclado.
- **Planejamento** uma coluna por vez; **Timeline** em lista vertical; **Conexões** com o grafo em
  tela cheia.
- Mais leve: sem desfoque de fundo e com menos animação contínua.

## 0.10.0-beta.1 — 2026-09-14 (pré-release)

Pré-release só Android, para teste: não há instalador Windows nesta versão, e quem usa a 0.9.2 no Windows continua nela.

### NarraHub no Android

- **Navegação gestual**: uma alça discreta na borda direita abre uma pilha de cartões com os
  destinos; arrastar na vertical gira a pilha, tocar no cartão da frente navega.
- **Temas claro e escuro** próprios do celular.
- **Atualização pelo próprio app**: o NarraHub verifica as releases do GitHub, baixa o APK
  assinado, confere o SHA-256 e abre o instalador do Android. Seus dados continuam no aparelho.
- A release passa a trazer `NarraHub-Android.apk` e `NarraHub-Android.apk.sha256`.

### Sincronização Windows ↔ Android (Sync V2)

- Pareamento por endereço e PIN, bootstrap do acervo com imagens e sincronização incremental nos
  dois sentidos. Em validação no aparelho.

## 0.9.2 — 2026-09-01

### Quando o aplicativo é mais antigo que os seus dados

- **Correção**: instalar uma versão anterior por cima de um banco mais novo fazia o NarraHub
  **não abrir** — sem janela e sem mensagem — enquanto os dados continuavam intactos no disco.
  Agora aparece uma tela que explica o que houve, mostra qual versão de dados você tem, e
  oferece atualizar o aplicativo ou voltar para um backup compatível.
- O aplicativo nunca abre um banco que não entende. Isso já era verdade e continua sendo: a
  diferença é que agora ele diz por quê, em vez de fechar calado.
- A lista de backups oferecida nessa tela mostra só os que aquela versão consegue abrir.

### Telas menores

- **Correção**: em janelas de 1366×768 o conteúdo que passava da altura da tela ficava
  cortado **e sem barra de rolagem** — em Ajustes, a lista de backups simplesmente sumia.
  Cinco telas eram afetadas: Ajustes, Conexões, Histórico, Timeline e Entidades.

### Tema claro e desempenho

- **Correção**: os painéis de vidro do tema claro caíam em cores de reserva porque as
  variáveis existiam só no tema escuro.
- Texto de apoio e destaques do tema claro escureceram, para leitura mais confortável nos
  tamanhos pequenos que a interface usa.
- As artes de fundo e a logo passaram a ser servidas em WebP: o fundo caiu de 805 KB para
  86 KB e a logo de 1073 KB para 16 KB. O tema claro ganhou arte própria em vez de reaproveitar
  a nebulosa escura clareada por um véu.

## 0.9.1 — 2026-08-29

### Tema claro legível

- **Correção**: no tema claro os painéis, a barra de título e os cartões continuavam escuros, com o texto azul-escuro por cima — quase ilegível. Seis variáveis de vidro (`--nh-glass-panel`, `--nh-glass-card`, `--nh-glass-border` e vizinhas) eram usadas em 44 lugares desde a 0.7.6 mas **nunca haviam sido definidas**, então cada uso caía num valor de reserva escuro, nos dois temas.
- **Correção**: a foto de fundo voltou ao tema claro, agora clareada por um véu de pergaminho, em vez do degradê sem imagem da 0.9.0.
- **Legibilidade**: o texto secundário do tema claro escureceu (4,47:1 → 6,24:1 de contraste) e o texto de apoio subiu de 2,90:1 para 4,95:1. Boa parte da interface usa fontes de 8-10px, onde o mínimo de acessibilidade importa de verdade.
- Seis lugares usavam `--nh-glass-border` sem valor de reserva, o que virava `currentColor`: a borda saía da cor do texto. Isso também afetava o tema escuro.

## 0.9.0 — 2026-08-29

### Correções da interface após a 0.8.0

- **Correção**: o fundo cósmico e a logo do topo esquerdo tinham sumido. Os arquivos continuam sendo `cosmic-nebula.jpg` e `narrahub-logo-full.png`, mas o código passou a pedir `.webp` — extensão que nunca existiu no projeto.
- **Correção**: alternar entre tema claro e escuro não mudava quase nada. O fundo do aplicativo é desenhado por variáveis de tema (`--nh-bg-nebula-*`, `--nh-vignette`) que nunca chegaram a ser definidas, então a nebulosa escura ficava por cima nos dois temas. No tema claro o fundo agora é um pergaminho luminoso, sem estrelas.

### Planejamento: campos em todos os cards ou só em um

- **Escolha o alcance ao criar**: ao adicionar uma propriedade no quadro, você decide se ela vale **em todos os cards** do universo ou **só neste card**. Antes toda propriedade valia para todos, sem a tela dizer isso.
- **Promova ou restrinja depois**: cada propriedade tem um botão que a torna universal ou a devolve ao card de origem. Os valores já preenchidos não são perdidos nas duas direções.
- A ficha passa a listar só o que vale para ela: as universais mais as do próprio card.
- Excluir um card leva junto, na mesma operação, as propriedades que existiam só nele.

### Fichas de entidade

- **Correção**: abrir uma entidade que estava no fim da lista mostrava a ficha já rolada até o rodapé. A ficha agora abre no começo, e voltar devolve a lista ao ponto em que você estava.

## 0.8.0 — 2026-08-28

### Canvas livre na tela de Conexões

- **Monte o diagrama do seu jeito**: arraste os nós e a posição fica salva; "Organizar grafo" volta ao arranjo automático e "↺ Layout" descarta o layout guardado.
- **Elementos que não são fichas**: acrescente **Título**, **Imagem** e **Nota** ao grafo e ligue qualquer coisa a qualquer coisa — entidade a título, imagem a nota.
- **Cânone continua separado de anotação**: relações entre entidades seguem aparecendo na ficha; ligações do diagrama são tracejadas e não viram fato do universo.
- **Correção**: as tabelas do canvas não eram criadas no banco (a migration existia mas não estava registrada no runtime), então adicionar elemento não fazia nada. A migration é aditiva e é aplicada ao abrir o app.
- **Correção**: o botão "＋ Conexão" aparecia duplicado, no cabeçalho e na barra da página.

### Navegação por rotas

- Cada seção do workspace (Escrita, Entidades, Conexões, Timeline, Planejamento, Histórico) agora carrega sob demanda, deixando a abertura do app mais leve.
- **Correção**: Histórico e Timeline abriam em branco — a página era renderizada fora da área visível.
- Sair da Escrita salva o capítulo automaticamente, sem substituir o salvamento já feito ao fechar a janela, atualizar ou restaurar backup.

## 0.7.6 — 2026-08-27

### Tema Cósmico Glassmorphic, Menus Arredondados e Identidade Visual

- **Fundo Cósmico & Estrelas Animadas**: Adicionado fundo estelar em alta resolução com camada tripla de estrelas cintilantes com aceleração por GPU via CSS.
- **Folha Celestial Iluminada**: Editor de escrita estilizado com bússola estelar, cantoneiras e moldura suave.
- **Menus Flutuantes Arredondados (18px)**: Substituição das divisórias retangulares secas por cartões flutuantes translúcidos com bordas arredondadas e efeito de vidro fosco (`backdrop-filter: blur(24px)`).
- **Workspace Header em Cápsula**: Barra de breadcrumbs e ações rápidas redesenhada como cápsula flutuante arredondada de 14px.
- **Novas Logos Oficiais e Ícones Nativos**:
  - Nova logo horizontal instalada na barra de título do aplicativo.
  - Novo ícone estelar multi-resolução gerado para o Windows (`.ico`, `.png`, `32x32`, `128x128`) e favicons web.
  - Injeção programática do ícone nativo nas janelas do Windows no bootstrap do Tauri (`win.set_icon(...)`), eliminando o ícone genérico do Angular da barra de tarefas.
- **Desbloqueio da Barra Superior (Titlebar)**: Isolamento de arrasto com `-webkit-app-region: no-drag` para todos os controles interativos, garantindo resposta imediata a cliques na busca, tema e controles de janela.
- **Contraste de Tipografia**: Clareamento de todas as fontes secundárias, textos inativos, metadados e tags para conforto visual e alta legibilidade.
- **Migração v14 do Banco**: Preparação de tabelas de Canvas (`canvas_nodes`, `canvas_edges`) para grafos conceituais.

## 0.7.5 — 2026-08-26

### Modernização Global dos Menus (Padrão CRM)

- **Planejamento Kanban**: Altura total fluida com cabeçalhos de coluna fixos e rolagem interna independente por etapa, eliminando o scroll vertical externo.
- **Toolbar CRM no Planejamento**: Pílulas interativas com contadores por etapa (`Ideias`, `Planejado`, `Escrevendo`, `Revisão`, `Finalizado`), filtro ágil ao clicar, busca por texto e botão de criação direta por coluna.
- **Linha do Tempo (Timeline)**: Remoção de margens vazias e scroll duplo, adição de toolbar compacta com busca por marcos, contador e suporte à rolagem horizontal via roda do mouse (`wheel`).
- **Histórico de Auditoria**: Interface reformulada em estilo audit log com rolagem interna suave, busca por registros, e badges semânticos de ação (`Criado`, `Editado`, `Excluído`).
- **Worldbuilding (Entidades)**: Substituição de banners gigantes por toolbar compacta com pílulas de categorias (`Personagens`, `Lugares`, `Eventos`, etc.) e badges com contagem em tempo real.
- **Grafo de Conexões**: Canvas Cytoscape maximizado para ocupar a altura total da viewport e gaveta de conexões integrada em badges com remoção rápida.
- **Ficha CRM e Cards**: Interface limpa sem placeholders repetitivos ou elementos de instrução desnecessários.

## 0.7.4 — 2026-08-25

### Réplica de Produção e Integridade

- Consulta segura e somente leitura ao acervo de produção no perfil de desenvolvimento, sem abrir a base instalada para gravação.
- Detecção e exibição de diferenças entre snapshots de produção (inclusões e exclusões de histórias, livros, capítulos e entidades).
- Módulo nativo de integridade de banco de dados SQLite (`health.rs`), com verificação de invariantes referenciais.
- Mecanismo de backup consistente com cálculo de hash SHA-256 e geração de manifestos versionados.
- Preparação de restauração com criação prévia de snapshot de segurança e capacidade de rollback em caso de falha.
- Extração da interface de réplica para componente isolado (`ProductionReplicaComponent`), otimizando o bundle de estilos da aplicação.

## 0.7.3 — 2026-08-25

### Correções

- O compartilhamento só é marcado como online depois que a URL pública responde ao `/health` identificado do NarraHub por HTTPS.
- Hostnames temporários publicados pelo Quick Tunnel sem DNS funcional são descartados, e o aplicativo tenta criar um novo túnel até três vezes.
- Sessões já marcadas como ativas são revalidadas antes da criação de outro link, evitando reutilizar um túnel morto.
- A captura da URL tolera mensagens do `cloudflared` divididas em mais de um bloco de saída.
- Falhas após todas as tentativas informam se o problema ocorreu em DNS, conexão, timeout ou resposta pública inválida.

## 0.7.2 — 2026-08-25

### Correções

- O CSS completo do aplicativo desktop passa a ser carregado diretamente, sem depender de um evento inline bloqueado pela política de segurança do Tauri.
- Os temas claro, escuro e sistema voltam a funcionar no aplicativo instalado e a preferência permanece aplicada após reiniciar.
- A navegação e os formulários das configurações voltam a respeitar o layout planejado, inclusive na seção de inteligência local.
- A pipeline de release agora compila e inspeciona o bundle de produção, impedindo a publicação quando tema ou configurações dependem do carregamento de CSS incompatível com o desktop.

## 0.7.1 — 2026-08-25

### Correções

- Desenvolvimento e produção agora usam identificadores e diretórios de dados separados, impedindo que uma execução local aplique migrações no banco do aplicativo instalado.
- Builds oficiais e builds desktop explícitos preservam o identificador de produção `com.narrahub.app`; o modo de desenvolvimento usa `com.narrahub.app.dev`.
- A troca de tema passou a ter permissão nativa no Tauri, sincronização após o carregamento do documento e estado acessível nos controles de tema.
- A validação de configuração falha caso os perfis de desenvolvimento e produção voltem a compartilhar o mesmo identificador.

## 0.7.0 — 2026-08-24

### Escrita

- Editor reorganizado para priorizar o texto, adaptar-se a janelas menores e evitar rolagem horizontal no título e na barra de ferramentas.
- Painel de contexto do capítulo recolhível, com resumo, personagens, lugares e deslocamentos.
- Ditado por áudio para anotações e capítulos, respeitando a disponibilidade do reconhecimento de voz no sistema.
- Correção ortográfica nativa, sugestões de nomes e autocomplete baseado no conteúdo recorrente do escritor.
- Retrato do personagem junto a falas no formato `Nome - "fala"`, quando a entidade possui imagem cadastrada.
- Balão contextual de IA sobre o texto selecionado, sem bloquear o restante do editor.
- Comandos rápidos com `/nome`, `/lugar` e atalhos de prompt personalizados.

### Inteligência artificial

- Assistente local gerenciado pelo aplicativo, com consentimento antes do primeiro download, inicialização automática e suspensão quando ocioso.
- Recomendação de modelo conforme memória, processador e recursos disponíveis na máquina.
- Compatibilidade com provedor local ou API configurada pelo usuário, sem segredo embutido no código.
- Ações para reescrever, corrigir, detalhar, resumir capítulos e apoiar a criação ou o resumo de entidades.
- Tratamento de indisponibilidade do servidor local e reinicialização segura do processo de IA.

### Organização do universo

- Tags independentes e reutilizáveis para universos, histórias, livros, capítulos e entidades.
- Campos personalizados restritos às fichas de entidades, como personagens, lugares, eventos e objetos.
- Exclusão explícita de histórias, livros, capítulos, entidades e ligações, com confirmação quando necessária.
- Arrastar e soltar cartões entre colunas do planejamento Kanban.
- Navegação de livros e capítulos corrigida para preservar hierarquia, nomes e espaço útil.

### Colaboração temporária

- Compartilhamento de um ou vários universos em uma mesma sessão.
- Permissões de visualização, comentário ou edição definidas pelo proprietário.
- Espaço web com capítulos, fichas de entidades em modal e anotações por seção.
- Contribuições criptografadas de ponta a ponta durante o transporte pelo túnel temporário.
- Fila local para revisar e aprovar alterações individualmente ou em lote antes de aplicá-las ao projeto.
- Encerramento da sessão sem publicar permanentemente o acervo do usuário.

### Dados e compatibilidade

- Migração local V9 adiciona as estruturas de colaboração sem recriar nem apagar o conteúdo existente.
- Migrações antigas permanecem imutáveis para preservar bancos que já foram atualizados.
- Atualização automática assinada mantida para instalações compatíveis do Windows.

## 0.6.1

- Correções de empacotamento e estabilização do atualizador assinado para Windows.

## 0.6.0

- Compartilhamento temporário local-first com Cloudflare Quick Tunnel e conteúdo criptografado no navegador.
- Primeira distribuição desktop com canal de atualização assinado.
