# NarraHub 0.7.0

Esta versão transforma o NarraHub em um ambiente de escrita mais completo: melhora o editor, adiciona assistência por IA local ou por API, organiza metadados e amplia o compartilhamento temporário para colaboração revisável.

## Destaques

- Editor responsivo com painel de contexto recolhível, ditado, correção ortográfica, autocomplete e retratos nas falas.
- IA acessível pelo balão sobre o texto selecionado e por comandos rápidos, sem abrir um modal que interrompa a escrita.
- IA local recomendada conforme a máquina, instalada somente após consentimento e iniciada automaticamente pelo aplicativo.
- Tags reutilizáveis separadas de campos personalizados das fichas de entidades.
- Exclusão de histórias, livros, capítulos, entidades e ligações.
- Planejamento Kanban com cartões arrastáveis.
- Compartilhamento de múltiplos universos com permissões de leitura, comentários ou propostas de edição.
- Revisão local das contribuições, com aprovação individual ou em lote pelo proprietário.

## Atualização

Quem já utiliza a versão 0.6.1 pode procurar a atualização nas configurações do aplicativo. O manifesto e o instalador do atualizador são assinados; o aplicativo valida a assinatura antes de instalar.

Uma instalação nova pode ser feita pelo instalador NSIS (`.exe`) ou pelo pacote MSI publicados nos arquivos desta release. Os dados continuam armazenados localmente no perfil do usuário.

## IA local

O modelo não é baixado silenciosamente. Na primeira ativação, o NarraHub analisa os recursos disponíveis, apresenta uma recomendação e solicita autorização. Depois de instalado, o processo local acompanha o ciclo do aplicativo e pode ser suspenso por inatividade.

A recomendação é um ponto de partida, não uma garantia de desempenho: máquinas com pouca memória podem responder mais lentamente. O usuário também pode configurar uma API própria; chaves não são incluídas no projeto nem na release.

## Colaboração e privacidade

O compartilhamento continua temporário e depende de o NarraHub do proprietário permanecer aberto. O proprietário escolhe universos e permissões. Comentários e propostas retornam criptografados e só alteram o projeto após aprovação local.

O túnel não substitui backup ou controle de versão. Ao encerrar a sessão, o endereço temporário deixa de funcionar, mas as contribuições já recebidas permanecem na fila local para revisão.

## Migração de dados

Na primeira abertura, a migração V9 cria as tabelas necessárias à colaboração. Ela não remove conteúdo existente. Como toda atualização que altera o banco local, é recomendável manter um backup recente do diretório de dados antes de instalar.

## Validação da versão

- Build de produção do Angular.
- Testes unitários e de integração do núcleo Tauri/Rust.
- Testes da API de compartilhamento.
- Fluxo web real de navegação, modal de entidade, anotação e proposta de edição.
- Empacotamento Windows e assinatura do atualizador executados pelo pipeline oficial do GitHub Actions.

O histórico técnico completo está em [CHANGELOG.md](../CHANGELOG.md), e os fluxos detalhados estão em [WRITING_ASSISTANCE.md](WRITING_ASSISTANCE.md), [ONLINE_SHARING.md](ONLINE_SHARING.md) e [UPDATES.md](UPDATES.md).
