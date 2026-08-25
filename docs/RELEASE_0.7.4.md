# NarraHub 0.7.4

Esta versão introduz a consulta segura ao acervo de produção no ambiente de desenvolvimento por meio de uma réplica somente leitura, além de infraestrutura nativa para verificação de saúde do banco SQLite, backups consistentes com validação SHA-256 e restauração protegida por snapshots de segurança.

## Novidades

### Réplica de Produção (Somente Leitura)

- O ambiente de desenvolvimento (`com.narrahub.app.dev`) agora pode capturar snapshots do banco instalado (`com.narrahub.app/narrahub.db`) de forma totalmente isolada.
- A base instalada de produção nunca é aberta para escrita durante os testes locais.
- Interface dedicada para navegar por universos, histórias, livros, capítulos e entidades de produção.
- Visualização de diffs entre a réplica atual e a anterior, indicando inclusões e exclusões de elementos narrativos.
- Leitor de capítulos da produção integrado sem controles de edição.

### Integridade e Backup Nativo

- Módulo Rust de verificação de integridade referencial (`PRAGMA foreign_key_check` e invariantes entre universos).
- Criação de backups locais com manifestos versionados, registro de schema e checksums SHA-256 de cada arquivo.
- Preparação de restauração com validação antecipada e criação obrigatória de snapshot de segurança da base ativa.
- Restauração atômica com capacidade de rollback automático caso ocorra qualquer falha na troca dos arquivos.

## Arquitetura e Performance

- Componentização da interface de réplica (`ProductionReplicaComponent`) mantendo o bundle de estilos dentro do orçamento de build.
- 29 testes de backend e suíte de IA aprovados.

## Atualização

Instale ou atualize o aplicativo via atualizador automático assinado ou pelos pacotes `.msi` / `.exe` disponíveis no GitHub Releases.
