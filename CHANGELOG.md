# Histórico de versões

As alterações relevantes do NarraHub são registradas neste arquivo. O projeto segue versionamento semântico: versões menores adicionam funcionalidades compatíveis e versões de correção tratam falhas sem alterar o fluxo principal.

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
