# Plano de evolução arquitetural

## Objetivo

Evoluir o NarraHub 0.7.x para um monólito modular local-first sem reescrita, perda de dados ou dependência de serviços online. Cada fase deve ser publicável isoladamente e só avança depois de testes reais em banco SQLite, runtime Tauri e atualização a partir da versão anterior.

## Princípios não negociáveis

- SQLite e arquivos locais são a fonte canônica.
- A interface não acessará persistência diretamente na arquitetura-alvo.
- Regras críticas e transações migram gradualmente para Rust.
- Nenhuma migration aplicada é alterada; toda evolução recebe nova versão.
- Atualização de aplicativo e atualização de banco são eventos diferentes.
- IA nunca altera conteúdo canônico sem confirmação explícita.
- Compartilhamento e colaboração são temporários; conteúdo e histórico pertencem ao dispositivo do autor.
- A nuvem pode entregar código estático ou transportar bytes, mas não persiste conteúdo narrativo.
- Sync nunca resolve conflito silenciosamente.
- Não serão introduzidos microserviços, CRDT, embeddings ou novos bancos antes de existir necessidade comprovada.

## Invariantes do domínio

Invariantes são regras executáveis, não apenas orientação de implementação. Commands Rust, adapters legados, imports, sync e colaboração devem respeitar o mesmo conjunto. Quando uma regra envolver mais de uma escrita, sua validação e alteração acontecem na mesma transação.

1. Um `Chapter` pertence a exatamente um `Book` existente.
2. Um `Book` pertence a exatamente uma `Story`, e a `Story` pertence a exatamente um `Universe` existente.
3. Uma `Entity` pertence a exatamente um `Universe` existente.
4. Uma `Relation` referencia duas entidades existentes no mesmo universo da relação.
5. Excluir uma entidade nunca deixa relações ou menções quebradas; referências opcionais usam `NULL` explícito e referências obrigatórias são removidas na mesma operação.
6. Uma revisão ou proposta nunca substitui conteúdo canônico sem um comando explícito de aprovação.
7. Uma sessão de compartilhamento nunca escreve diretamente em conteúdo canônico; ela cria anotações ou propostas pendentes.
8. Uma resposta de IA nunca altera conteúdo canônico antes da confirmação do escritor.
9. Uma migration publicada nunca é alterada; correções de esquema são sempre migrations novas.
10. Uma operação de domínio falha por inteiro ou é confirmada por inteiro.
11. IDs persistidos são estáveis; renomear um item não altera sua identidade nem quebra referências.

As foreign keys do SQLite ajudam a proteger essas regras, mas não substituem invariantes de domínio. Por exemplo, as FKs garantem que as pontas de uma relação existam, porém a regra de que ambas pertencem ao mesmo universo precisa ser validada pelo caso de uso.

Testes mínimos que acompanharão a migração para Rust:

```rust
#[test]
fn chapter_cannot_reference_missing_book() {}

#[test]
fn relation_cannot_reference_missing_or_foreign_universe_entity() {}

#[test]
fn deleting_entity_preserves_domain_integrity() {}

#[test]
fn collaboration_proposal_never_changes_canonical_content() {}

#[test]
fn failed_domain_operation_rolls_back_every_write() {}
```

## Arquitetura-alvo

```text
Angular UI
├── shell e router
├── features por domínio
├── stores com Signals
└── gateways tipados
        │
        ▼
Tauri Commands e Queries
        │
        ▼
Rust Application Core
├── workspace
├── manuscript
├── worldbuilding
├── knowledge
├── planning
├── revision
├── assistance
└── integrations
        │
        ▼
Infrastructure
├── SQLite
├── arquivos
├── backup e recuperação
├── runtime de IA
├── sync local
└── transporte temporário de compartilhamento
```

## Backup e recuperação como infraestrutura crítica

Em um produto local-first, o dispositivo do usuário é a fonte de verdade. Por isso, backup não é utilitário secundário: é requisito para migrations, atualização e recuperação de desastre.

Cada backup será um snapshot autocontido e imutável:

```text
backups/
├── 2026-08-25_1400/
│   ├── manifest.json
│   ├── narrahub.db
│   └── assets/
├── 2026-08-24_2200/
└── 2026-08-23_1800/
```

Contrato mínimo do manifesto:

```json
{
  "formatVersion": 1,
  "schemaVersion": 14,
  "appVersion": "0.9.2",
  "createdAt": "2026-08-25T17:00:00Z",
  "database": {
    "file": "narrahub.db",
    "sha256": "..."
  },
  "assets": {
    "count": 47,
    "manifestSha256": "..."
  }
}
```

Regras do `BackupService`:

- usar snapshot consistente do SQLite; copiar apenas o arquivo principal enquanto WAL está ativo é proibido;
- copiar assets para diretório temporário e publicar o backup somente depois de hashes e manifesto válidos;
- nunca sobrescrever um backup concluído;
- aplicar retenção somente depois de existir ao menos um backup novo verificado;
- restaurar primeiro em área temporária, executar `integrity_check`, `foreign_key_check` e validar hashes antes de substituir a base ativa;
- registrar motivo do backup: manual, pré-migration, pré-update ou periódico;
- manter o caminho de backup configurável e nunca misturar backups com arquivos temporários da aplicação.

## Limites de domínio

| Domínio | Responsabilidade | Dados atuais principais |
| --- | --- | --- |
| Workspace | Universo, preferências do workspace, escopo e assets | `universes`, `attachments` |
| Manuscript | História, livro, capítulo, revisão e autosave | `stories`, `books`, `chapters`, `chapter_revisions` |
| Worldbuilding | Personagem, lugar, objeto, organização e demais fichas | `entities`, `entity_attributes`, `entity_templates` |
| Knowledge | Relações, menções, tags e fatos canônicos | `relations`, `mentions`, `content_tags`, `content_tag_assignments` |
| Planning | Timeline e kanban editorial | `timeline_events`, `planning_items` |
| Revision | Histórico, propostas e conflitos | `change_log`, `collaboration_contributions`, `sync_conflicts` |
| Assistance | Contexto, memória criativa e provedores de IA | armazenamento atual da IA; tabela local futura |
| Integrations | Share, colaboração, sync, atualização e plataforma | tabelas de colaboração/sync e módulos Rust existentes |

Uma entidade do tipo `Evento` representa um elemento de worldbuilding. `timeline_events` representa sua posição ou ocorrência na cronologia; os conceitos não devem ser fundidos.

## Estratégia de substituição gradual

Cada feature Angular dependerá de um gateway. No início, o gateway chama o serviço SQL legado. Depois, a implementação é trocada por um comando Rust. Componentes e stores não mudam durante essa troca.

```text
Feature Store ──► Gateway ──► Legacy SQL Adapter
                      └─────► Tauri/Rust Adapter
```

Não haverá dual-write. Um comando usa exatamente um adaptador. A troca só ocorre depois de testes de contrato executarem o mesmo cenário nos dois caminhos e compararem o estado final do banco.

## Política de versões e compatibilidade

As seguintes versões são independentes:

```text
appVersion
databaseSchemaVersion
shareProtocolVersion
syncProtocolVersion
aiContextVersion
```

Regras de atualização:

1. O atualizador baixa e valida a assinatura do novo executável.
2. Antes da primeira migration, o aplicativo fecha conexões ativas e cria backup consistente do SQLite e arquivos vinculados.
3. A migration roda no runtime Tauri e registra versão/checksum.
4. O aplicativo executa health check do banco antes de abrir o workspace.
5. Se a migration falhar, o banco original permanece recuperável e o aplicativo apresenta recuperação explícita.
6. Uma versão antiga não deve abrir silenciosamente um esquema incompatível. Ela deve bloquear com mensagem clara.
7. Downgrade não é tratado como rollback de banco. Rollback usa backup criado antes da migration.
8. Migrations aditivas são preferidas. Rebuild de tabela exige teste específico de foreign keys, índices e triggers.

## Gates obrigatórios de cada fase

Toda fase deve passar por cinco gates:

### Gate 1 — Código

- build Angular de produção;
- testes unitários da feature;
- testes Rust;
- validação de formato e `git diff --check`;
- nenhum segredo ou endpoint crítico hardcoded.

### Gate 2 — Banco

- banco vazio recebendo todas as migrations em sequência;
- cópia de banco da versão publicada anterior;
- banco populado com todos os tipos de entidade;
- foreign keys verificadas com `PRAGMA foreign_key_check`;
- integridade verificada com `PRAGMA integrity_check`;
- índices e triggers esperados inspecionados;
- teste de repetição da inicialização sem reaplicar migration.

### Gate 3 — Runtime Tauri

- inicialização no identificador de desenvolvimento;
- operações reais de criar, editar, excluir e reabrir;
- encerramento e reinício do aplicativo;
- banco de produção instalado permanece intocado;
- logs sem panic, erro de migration ou processo órfão.

### Gate 4 — Atualização

- instalar a versão publicada anterior em ambiente de teste;
- criar dados reais nessa versão;
- atualizar pelo instalador/updater candidato;
- confirmar preservação de conteúdo, preferências e arquivos;
- reiniciar duas vezes;
- validar backup e procedimento de recuperação;
- testar bloqueio de downgrade incompatível.

### Gate 5 — Aceitação funcional

- checklist manual dos fluxos afetados;
- comparação visual claro/escuro e tamanhos responsivos;
- editor com autosave, seleção e revisão;
- nenhuma alteração automática de conteúdo pela IA ou colaboração;
- release somente depois de registrar evidência dos quatro gates anteriores.

## Fase 0 — Estabilizar a linha 0.7.x

### Estado de implementação

Escopo de engenharia concluído localmente em 26/08/2026:

- migration 10 congelada; migrations 11, 12 e 13 são append-only;
- fixture representativa e anonimizada do schema 10 cobre todos os domínios persistidos;
- cadeia completa de migrations é testada em arquivo, fechada e reaberta;
- upgrade 10→13 preserva conteúdo e contagens das tabelas canônicas;
- prompts automatizados e geração com o modelo local real foram validados;
- build de produção, configuração desktop e carregamento dos estilos de tema/configurações passaram;
- uma réplica isolada do banco instalado abriu no runtime Tauri, migrou e reiniciou sem reaplicar migrations;
- o banco instalado permaneceu intocado, confirmado por SHA-256 e data de modificação.

O gate de publicação continua separado: a versão somente poderá ser classificada como publicável após assinatura e upgrade por instalador/updater em ambiente de teste. As evidências e o bloqueio externo estão em [`PHASE_0_1_QUALIFICATION.md`](PHASE_0_1_QUALIFICATION.md).

### Entregas

- Isolar renomeação/tags e correções de IA em commits temáticos.
- Congelar a migration 10; qualquer evolução seguinte começa na versão 11.
- Adicionar testes de prompt e validação opcional com o modelo local real.
- Documentar matriz de versão do aplicativo e esquema.
- Criar fixture de banco representativa e anonimizada.

### Testes reais

- `npm run test:ai`;
- `npm run validate:local-ai` com o Qwen instalado;
- `npm run build`;
- `npm run release:validate-ui`;
- `cargo test --manifest-path src-tauri/Cargo.toml`;
- migration 1→10 em memória e em arquivo;
- abertura no Tauri de desenvolvimento.

### Critério de saída

A versão de estabilização abre bancos anteriores, mantém capítulos e permite reabrir o app sem modificar checksums.

## Fase 1 — Rede de segurança e decisões

### Estado de implementação

Escopo de engenharia concluído localmente na versão de desenvolvimento 0.7.4 em 26/08/2026:

- diagnóstico Rust somente leitura implementado;
- invariantes estruturais centrais verificados pelo diagnóstico;
- criação, listagem e validação de backups implementadas;
- manifesto, hashes, WAL e staging atômico cobertos por testes;
- criação manual exposta em Configurações;
- backup pré-update integrado ao updater e configurado para bloquear instalação sem snapshot válido;
- restauração em duas etapas implementada com `pre_restore`, fechamento do pool SQL, rollback de arquivos e reinício;
- compatibilidade das migrations aplicadas é validada por checksum antes da troca;
- retenção implementada para os cinco automáticos mais recentes, preservando manuais e `pre_restore`;
- testes temporários cobrem round-trip, schema futuro e falha injetada após a troca;
- contrato de erros tipados implementado nos commands Rust e normalizado no frontend;
- banco desktop real validado por backup online somente leitura;
- réplica isolada migrada no Tauri e validada novamente depois de dois boots;
- contagens das tabelas canônicas comparadas entre a origem schema 10 e a réplica schema 13;
- interação real no desktop empacotado e teste com instalador assinado publicado permanecem pendentes como gate de release, não como código da fase.

O contrato operacional está documentado em [`BACKUP_AND_RECOVERY.md`](BACKUP_AND_RECOVERY.md).
O relatório de qualificação local está em [`PHASE_0_1_QUALIFICATION.md`](PHASE_0_1_QUALIFICATION.md).

### Entregas

- ADRs obrigatórios em `docs/ADR`.
- ADR de backup define snapshot, publicação, retenção e restauração como contratos de infraestrutura.
- Testes de caracterização para universo, manuscrito, entidades, tags, timeline e planejamento.
- `DatabaseHealthService` Rust somente leitura.
- `BackupService` Rust com snapshot consistente, manifesto versionado, hashes e política de retenção.
- Erros tipados: validação, inexistente, conflito, armazenamento e indisponível.

### Testes reais

- restaurar backup em diretório temporário e abrir pelo Tauri;
- comparar hash e contagem das tabelas antes/depois;
- validar backup com banco em modo WAL e escrita recente já confirmada;
- interromper a criação antes da publicação e garantir que backup parcial não seja oferecido para restauração;
- corromper banco ou asset do snapshot e confirmar rejeição por hash/integridade;
- simular banco corrompido e garantir bloqueio sem sobrescrita;
- injetar falha depois de instalar a cópia e confirmar rollback integral da base anterior;
- simular falha de migration e confirmar disponibilidade do backup.

### Rollback

Somente código; nenhuma tabela de domínio é alterada nesta fase.

## Fase 2 — Modularização Angular sem mudar persistência

### Estado de implementação

Primeira fatia vertical implementada no planejamento para a próxima versão após `0.7.4`:

- quadro e ficha extraídos de `App` para `PlanningBoardComponent`;
- SQL do planejamento isolado em `PlanningService`;
- drag-and-drop usa transformação pura testada e persiste todo o quadro em uma única instrução;
- migrations 11, 12 e 13 append-only adicionam imagem, valores escalares, definições tipadas, relações normalizadas por universo e migração do formato provisório;
- campos e tags permanecem domínios separados;
- salvamento integral da ficha usa um comando Rust transacional como primeiro piloto da Fase 4;
- snapshot de sincronização inclui definições e relações do planejamento;
- build Angular, teste da migration, runtime Tauri e suíte pura do quadro exercitados;
- automação visual de arrastar no WebView e matriz de upgrade do instalador ainda permanecem pendentes.

Segunda fatia vertical implementada na mesma linha de desenvolvimento:

- Timeline e Histórico foram extraídos do `App` para componentes e stores próprios;
- gateways tipados expressam `list`, `create`, `rename` e `delete` sem expor SQL à UI;
- adapters `LegacyTimelineGateway` e `LegacyHistoryGateway` preservam o comportamento SQLite atual até a migração para commands Rust;
- respostas atrasadas são descartadas por revisão de carga ao trocar de universo;
- estado, formulários e erros da Timeline, além da carga do Histórico, deixaram de pertencer ao componente raiz;
- testes automatizados impedem SQL e `WorkspaceService` de voltarem aos componentes, stores e contratos das features;
- nenhuma migration ou formato persistido foi alterado por essa modularização.

Terceira fatia vertical implementada na mesma linha de desenvolvimento:

- Biblioteca de universos (listar, criar, editar e excluir) foi extraída do `App` para `LibraryPageComponent`, com `UniverseStore` e `UniverseGateway` próprios;
- `LegacyUniverseGateway` preserva o comportamento atual via `UniverseService` até a migração para um command Rust; a entrada `UniverseGateway → LegacyUniverseGateway` é registrada em `app.config.ts`, no mesmo padrão de Timeline e Histórico;
- criação, edição, exclusão e atualização de estatísticas do universo ativo passam pelo gateway; `App` não injeta mais `UniverseService` diretamente;
- `UniversePickerComponent` permanece uma apresentação pura, agora hospedada pelo `LibraryPageComponent`, que também assumiu os modais de criar/editar e excluir universo (antes no modal host compartilhado do `App`);
- decisões que ainda cruzam domínios não extraídos — abrir um universo, tags da biblioteca e o refresh após revisão colaborativa — continuam orquestradas pelo `App`, que agora só lê/escreve universo através de `UniverseStore`;
- o carregamento da biblioteca respeita a mesma guarda de ambiente do `ngOnInit` do `App` (sem acesso a banco fora do runtime Tauri), preservando a iteração de UI via `ng serve`;
- teste de fronteira ampliado cobre os três arquivos da feature e confirma que `app.ts` não referencia mais `UniverseService`;
- build de produção Angular e a suíte `npm run test:architecture` foram exercitados; verificação da tela de biblioteca (estado vazio e modal de criação) foi feita via `ng serve` no navegador; runtime Tauri empacotado e matriz de upgrade do instalador permanecem como gate de release, não como código da fase.

Quarta fatia vertical implementada na mesma linha de desenvolvimento:

- Worldbuilding/Entidades (listar, filtrar por tipo, criar, editar, excluir, ficha completa, propriedades, galeria de imagens e as três ações de IA) foi extraído do `App` para `features/entities/`, com `EntityStore`, `EntityGateway`/`LegacyEntityGateway` e uma árvore de componentes própria (`EntitiesPageComponent` orquestrando `EntityToolbarComponent` → `EntityTypeFilterComponent`, `EntityCardComponent` e `EntitySheetComponent`);
- `LegacyEntityGateway` encapsula `EntityService` e `AttachmentService` (galeria); nenhuma migration ou formato persistido foi alterado;
- a lógica de IA (montar ficha nova, sugerir campos, resumir) e o parser tolerante a JSON malformado da IA local migraram junto, como orquestração da página, não do store — mantém o mesmo contrato de erro tipado por trás de `AiService`;
- `App` não injeta mais `EntityService`/`AttachmentService`; `AppState` perdeu `sidebarEntityFilter`/`activeEntityId`, que agora vivem em `EntityStore` (`filter`/`activeEntity`);
- decisões que cruzam domínios não extraídos — abrir a ficha a partir da busca global ou dos lugares mencionados no editor, atualizar estatísticas do universo após criar/excluir, atualizar menções e relações após renomear/excluir — continuam orquestradas pelo `App` via os eventos `(opened)/(mutated)/(info)/(failed)` da feature;
- estilos da feature usam `ViewEncapsulation.None` com todas as regras prefixadas por `app-entities-page`, evitando vazar CSS global e mantendo o visual da 0.7.5 inalterado; o CSS equivalente permanece (não utilizado) em `app.css` — remoção documentada como limpeza pendente, não como parte desta fatia;
- teste de fronteira ampliado cobre os sete arquivos da feature e confirma que `app.ts`/`app.html` não referenciam mais `EntityService`, `AttachmentService` ou o estado antigo da ficha ativa;
- build de produção Angular, `npm run test:architecture` e `npm run test:ai` foram exercitados; verificação de console limpo via `ng serve` cobre apenas a tela de biblioteca, já que abrir uma ficha de entidade exige um universo real (runtime Tauri) — isso permanece como verificação pendente no app empacotado, não como código da fase.

Essa fatia avança a Ordem 3 (Entidades) da fase — o restante do Knowledge (relações, menções e tags) continua no `App`, ainda chamando `WorkspaceService`/`MetadataService`/`MentionService` diretamente. Configurações, colaboração e o manuscrito (história/livro/capítulo/editor) também ainda precisam dos próprios gateways/stores.

Quinta fatia vertical implementada na mesma linha de desenvolvimento:

- Configurações e IA (Ordem 4) foram extraídas do `App` para `features/settings/`, com `SettingsPageComponent` hospedando as cinco abas (Geral, Inteligência, Dispositivos, Compartilhar, Atualizações) e um `SettingsStore` cobrindo backup, atualização e sincronização;
- diferente dos demais, este domínio não tem `LegacyXGateway`: `BackupService`, `SyncService` e `UpdateService` já são comandos Tauri nativos, sem SQL por trás — não existe fronteira SQL-vs-Rust para abstrair aqui. A única exceção documentada e testada é o `SettingsStore` injetar `DatabaseService` diretamente, só para fechar/reabrir o pool SQLite durante uma restauração de backup (ciclo de vida da conexão, não SQL de domínio);
- IA local (instalar, ativar, reiniciar, perfil recomendado, orientações de escrita, memória criativa) passou a ser 100% orquestrada pelo `SettingsPageComponent` injetando `AiService` diretamente, sem round-trip pelo `App`;
- backup/restauração e atualização continuam cruzando para fora da feature só onde precisam de `saveChapterNow()` (Editor, não extraído) — `criarBackup`, `prepararRestauração` e `instalarAtualização` são solicitados por evento e o `App` decide se salva o capítulo antes de delegar ao store;
- a aba "Compartilhar" mostra a UI completa de sessões colaborativas e revisão de contribuições, mas os dados e as ações continuam vindo do `App` por `@Input()`/`@Output()` — Colaboração é a Ordem 5, ainda não extraída, e essa aba será migrada para consumir o futuro `CollaborationStore` quando essa fatia acontecer;
- `AppState` não tinha estado de configurações para remover; o `App` perdeu `ThemeService`, `BackupService`, `SyncService` e `UpdateService` como injeções diretas, mantendo só os alias de leitura que o banner global de atualização (fora da tela de Configurações) ainda precisa;
- teste de fronteira cobre a feature nova com uma regra própria para o `SettingsStore` (permite `DatabaseService`, continua proibindo SQL e os serviços legados de outros domínios) e confirma que `app.ts`/`app.html` não referenciam mais os serviços de configurações;
- build de produção Angular, `npm run test:architecture` e `npm run test:ai` foram exercitados; as cinco abas foram verificadas via `ng serve` (a tela de Configurações não depende de um universo aberto, então deu para clicar em cada aba e no botão "Verificar agora" sem erro de console) — o fluxo real de backup/restauração/atualização com o SQLite de verdade continua exigindo o app Tauri empacotado.

Essa fatia conclui a Ordem 4 (Configurações e IA). Colaboração e o manuscrito (história/livro/capítulo/editor) são as duas fatias que restam na Fase 2.

Sexta fatia vertical implementada na mesma linha de desenvolvimento:

- Colaboração e compartilhamento (Ordem 5) foram extraídos do `App` para `features/collaboration/`, com `CollaborationGateway`/`LegacyCollaborationGateway` (envolvendo `CollaborationService`, que ainda é SQL sobre `DatabaseService` — aqui sim existe fronteira SQL-vs-Rust a abstrair, ao contrário de Configurações) e um `CollaborationStore` cobrindo sessões, contribuições e o ciclo de vida do link temporário (`OnlineShareService`, já nativo, injetado direto no store);
- o modal de compartilhar saiu do host de modais compartilhado do `App` para `ShareModalComponent`, com backdrop e formulário próprios — o `App` só decide QUANDO abrir (`appState.modalOpen() === 'share-content'`) e monta o documento a compartilhar (ainda depende de `ChapterService` e `EntityStore`, Manuscrito não extraído);
- a aba Configurações > Compartilhar, que na fatia anterior recebia dados/ações da colaboração por `@Input()`/`@Output()` do `App`, agora injeta `CollaborationStore` diretamente — a dívida documentada naquela fatia foi paga nesta;
- revisar ou aprovar em lote uma contribuição ainda pode alterar capítulo/ficha ativos e estatísticas do universo (Editor e Manuscrito não extraídos); por isso `SettingsPageComponent` emite `(collaborationApplied)` só quando uma aprovação de fato mudou conteúdo canônico, e o `App` decide o que recarregar;
- `App` não injeta mais `CollaborationService` nem `OnlineShareService` diretamente; as únicas chamadas que sobraram no `App` são orquestração cross-domínio (`saveChapterNow` antes de criar/restaurar, montar o payload compartilhado a partir de capítulos/entidades);
- teste de fronteira cobre os três arquivos novos e confirma que `app.ts`/`app.html` não referenciam mais os serviços de colaboração nem os campos de formulário do modal antigo;
- build de produção Angular e `npm run test:architecture` foram exercitados; a aba Configurações > Compartilhar foi conferida via `ng serve` sem erro de console. Durante a verificação, um provider do novo gateway ficou faltando em `app.config.ts` (erro `NG0201` só visível no runtime) — ficou registrado como lição na skill [[narrahub-feature-extraction]]: sempre confirmar o boot da página no navegador depois de registrar um gateway novo, não só o `npm run build`. O modal de compartilhar em si (aberto de dentro de um universo) e o fluxo real de link/túnel continuam exigindo o app Tauri empacotado.

Restou só uma fatia na Fase 2: o manuscrito (história, livro, capítulo) e o editor com autosave (Ordem 6 e 7).

Sétima fatia vertical implementada na mesma linha de desenvolvimento:

- Manuscrito (Ordem 6) e Editor com autosave (Ordem 7) foram extraídos juntos, não em duas fatias separadas — a seleção de capítulo alimenta diretamente o estado de rascunho do editor, sem costura limpa entre "escolher capítulo" e "editar/salvar capítulo", o mesmo motivo que já uniu Timeline+Histórico e Configurações+IA em fatias anteriores;
- `features/manuscript/` ganhou `ManuscriptGateway`/`LegacyManuscriptGateway` (envolvendo `StoryService`/`BookService`/`ChapterService`, todos SQL sobre `DatabaseService` — fronteira real a abstrair), um `ManuscriptStore` cobrindo história/livro/capítulo/árvore/autosave, e `WritingPageComponent` hospedando a árvore de projeto, o `<app-writing-editor>` existente e o painel de resumo/contexto;
- o autosave (debounce, `saveNow()`) mora no `ManuscriptStore`, não no component — é comportamento do domínio, não só da UI; o `App` continua podendo forçar um salvamento síncrono antes de trocar de universo, fechar a janela ou instalar uma atualização, exatamente como antes;
- **Knowledge (menções) fica de fora de propósito**: sincronizar menções ao salvar um capítulo é uma escrita cruzando para um domínio ainda não extraído, então o `ManuscriptStore` expõe um campo `onChapterPersisted` (um hook simples, não um `@Output()` — stores não têm outputs) que o `App` usa para chamar `MentionService` depois de cada salvamento bem-sucedido. É a mesma razão pela qual `WritingPageComponent` recebe `entities`/`mentionOccurrences` como `@Input()` do `App` em vez de injetar `MentionService` direto: a feature não conhece o serviço, só os dados já carregados;
- os modais de nova história/novo livro/novo capítulo e de renomear/excluir história/livro/capítulo saíram do host de modais compartilhado do `App` para dentro do próprio `WritingPageComponent` (mesmo padrão de Entidades/Configurações); o host compartilhado do `App` ficou reduzido a só duas ações que nenhuma feature extraída cobre ainda: renomear universo e excluir ligação (Conexões, fora do escopo desta fase);
- o CSS de árvore de projeto/editor/inspetor foi copiado de `app.css` para `writing-page.component.css` com encapsulamento padrão do Angular (sem prefixo de seletor, ao contrário de Entidades/Configurações) — mais simples porque não havia necessidade de reaproveitar nomes de classe genéricos entre features; `:host-context(.focus-mode)` substitui as regras `.focus-mode .writing-layout` que dependiam de um ancestral fora do componente. O CSS equivalente permanece (não utilizado) em `app.css`, mesma dívida documentada nas fatias anteriores;
- `activeStoryId`/`activeBookId`/`activeChapterId` saíram do `AppState` (nada além do próprio `App`/`app.html` os lia) e `openEditor()` perdeu o parâmetro `chapterId`, que nunca era consumido fora da própria navegação;
- `App` não injeta mais `StoryService`/`BookService`/`ChapterService` diretamente; o que sobrou são aliases de leitura (`activeStory`, `activeBook`, `activeChapter`, `saveMessage`, `isSaving`, `inspectorOpen`) e orquestração cross-domínio (abrir um resultado de busca global, navegar de um card do Planejamento até o capítulo, montar o payload de Compartilhamento a partir de `ManuscriptStore.listChaptersSnapshot()`);
- teste de fronteira cobre os três arquivos novos e confirma que `app.ts`/`app.html` não referenciam mais os serviços legados nem os campos do editor antigo;
- build de produção Angular e `npm run test:architecture` foram exercitados (12/12). O orçamento de bundle inicial do `angular.json` foi ajustado (1.4MB → 1.6MB de erro) para acomodar o crescimento esperado de sete fatias de features; verificado via `ng serve` em aba nova, sem erro de console — o fluxo real de escrita (abrir um universo com dados, criar história/livro/capítulo, autosave) continua exigindo o app Tauri empacotado, mesma limitação já registrada nas fatias anteriores.

Com esta fatia, as sete ordens da Fase 2 estão endereçadas. O que **não** foi tocado nesta fase, por estar fora do escopo original: o domínio de Knowledge propriamente dito (relações, menções, tags — `WorkspaceService`/`MetadataService`/`MentionService` continuam injetados direto no `App`) e a página de Conexões/Grafo (ainda é markup solto dentro de `app.html`, sem gateway/store própria). Ambos são candidatos naturais para uma Fase 2.1, se o time decidir levá-los adiante antes da Fase 3.

### Entregas

- Criar gateways e stores por feature.
- Extrair biblioteca de universos, timeline, planejamento, entidades, configurações e colaboração.
- Extrair manuscrito e editor por último.
- Reduzir `App` a shell e orquestração temporária.
- Manter navegação por Signals até o ciclo de vida das features estar isolado.

### Ordem

1. Universo.
2. Timeline e planejamento.
3. Entidades e Knowledge.
4. Configurações e IA.
5. Colaboração.
6. História, livro e capítulo.
7. Editor e autosave.

### Testes reais

- testes dos stores com gateway fake determinístico;
- teste de troca rápida entre universos descartando respostas antigas;
- selecionar, editar, navegar e confirmar autosave;
- abrir/fechar todos os modais;
- teste responsivo e temas no build de produção.

### Rollback

Os gateways continuam apontando para os serviços SQL existentes. Nenhuma migration é necessária.

## Fase 2.1 — Conexões e Knowledge (relações, tags, menções)

### Estado de implementação

Fase adicional, fora do escopo original das 7 ordens da Fase 2, para fechar os dois gaps deixados por ela: a página de Conexões/Grafo e o domínio de Knowledge (tags e menções). Implementada em uma única fatia:

- **Conexões** ganhou `features/connections/`: `ConnectionsGateway`/`LegacyConnectionsGateway` (envolvendo `WorkspaceService`, que já tinha os métodos de relação — o mesmo serviço que `LegacyTimelineGateway`/`LegacyHistoryGateway` também envolvem, cada um com sua própria fatia do contrato), um `ConnectionsStore` cobrindo a lista de relações do universo, e `ConnectionsPageComponent` hospedando o `<app-connections-graph>` (componente de apresentação já existente, sem mudanças) mais a lista de ligações e os próprios modais de criar/excluir conexão;
- diferente de Entidades/Timeline (que o `App` carrega antecipadamente porque outras áreas leem os dados deles), relações só importam para quem está na própria tela de Conexões — por isso `ConnectionsPageComponent` carrega sozinho (`ngOnChanges` em `[universeId]`) e o `App` não faz mais um carregamento antecipado ao trocar de universo ou navegar para "conexões";
- **Knowledge** ganhou `features/knowledge/`: um único `KnowledgeGateway`/`LegacyKnowledgeGateway` cobrindo tags (`MetadataService`) e menções (`MentionService`) — duas tabelas SQL diferentes, mas um único contrato, porque as duas são utilitários cross-cutting consumidos pelas mesmas features (Entidades, Timeline, Manuscrito, Biblioteca) sem ter uma tela própria; um `KnowledgeStore` cobrindo os previews de tags (biblioteca e workspace), o estado do modal de tags e o índice de menções; e `TagsModalComponent`, que substitui o caso `'metadata'` do modal compartilhado do `App` — o mesmo padrão do `ShareModalComponent` na fatia de Colaboração;
- o `KnowledgeStore` injeta `UniverseStore` diretamente (para saber quais universos existem ao recarregar os previews de tags da Biblioteca) — é uma composição entre dois stores de domínios diferentes, não um acesso a serviço legado, então não fere o limite que os testes de fronteira protegem;
- sincronizar menções ao salvar um capítulo continua uma escrita cross-domain: o hook `onChapterPersisted` do `ManuscriptStore` (criado na fatia anterior) agora chama `KnowledgeStore.syncChapterMentions(...)` em vez de `MentionService` direto — o `ManuscriptStore` nunca precisou saber qual serviço trata menções, só que *algo* precisa ser notificado;
- o host de modais compartilhado do `App` ficou reduzido a **um único caso**: renomear universo. Excluir relação e a organização de tags — os dois últimos usos do host genérico — agora são autocontidos nas respectivas features, mesmo padrão de Entidades/Manuscrito;
- CSS de grafo/relações e do modal de tags copiado para o CSS próprio de cada componente novo (encapsulamento padrão do Angular, já que não havia necessidade de reaproveitar nomes de classe fora do componente). As cópias antigas em `app.css` (`.graph-toolbar`, `.relation-*`, `.metadata-section`/`.metadata-tags`/`.metadata-new-tag`) ficam como dívida documentada, mesmo tratamento das fatias anteriores;
- teste de fronteira cobre os seis arquivos novos e confirma que `app.ts`/`app.html` não referenciam mais `WorkspaceService` (para relações), `MetadataService` nem `MentionService`, nem os campos de formulário dos modais antigos;
- build de produção Angular e `npm run test:architecture` foram exercitados (14/14). Verificado via `ng serve` em aba nova, sem erro de console — como o `App` injeta `KnowledgeStore`/`ConnectionsStore` diretamente (e cada um resolve seu gateway no construtor), um provider faltando em `app.config.ts` teria aparecido como `NG0201` já no boot da aplicação, não só ao navegar para uma tela específica.

Com esta fatia, os dois gaps documentados ao final da Fase 2 estão fechados. Não ficou nenhum domínio de conteúdo (história, livro, capítulo, entidade, timeline, planejamento, relação, tag, menção) chamando um serviço Angular legado direto do `App` — o que resta em `App` é orquestração cross-domínio genuína (busca global, restaurar rota, montar o payload de Compartilhamento) e o único modal que nenhuma feature reivindicou (renomear universo).

### Entregas

- `ConnectionsGateway`/`LegacyConnectionsGateway`, `ConnectionsStore`, `ConnectionsPageComponent`.
- `KnowledgeGateway`/`LegacyKnowledgeGateway`, `KnowledgeStore`, `TagsModalComponent`.
- Reduzir o host de modais compartilhado do `App` ao caso restante sem feature própria (renomear universo).

### Testes reais

- teste de fronteira cobrindo os seis arquivos novos e a delegação em `app.ts`/`app.html`;
- build de produção Angular;
- checagem visual via `ng serve` em aba nova, sem erro de console.

### Rollback

Os gateways continuam apontando para os serviços SQL existentes (`WorkspaceService`, `MetadataService`, `MentionService`). Nenhuma migration é necessária.

## Fase 3 — Router e carregamento por feature

### Estado de implementação

A fundação do Router foi iniciada sem duplicar telas nem trocar a fonte de estado das features:

- rotas de alto nível representam biblioteca, configurações e seções do workspace;
- URLs usam o ID estável do universo e nomes estáveis de seção;
- o shell restaura uma rota profunda válida após inicializar o banco local;
- uma rota para universo inexistente retorna à biblioteca com mensagem recuperável;
- Signals e `AppState` continuam controlando a apresentação enquanto os demais ciclos de vida são extraídos;
- navegação real entre `/library` e `/settings`, além do fallback de deep link inválido, foi validada em Chromium;
- resolvers, guards de autosave, componentes roteados e lazy loading permanecem para as próximas fatias desta fase.

### Entregas

```text
/universes
/universe/:universeId/write/:chapterId?
/universe/:universeId/entities/:entityId?
/universe/:universeId/connections
/universe/:universeId/timeline
/universe/:universeId/planning
/universe/:universeId/history
/universe/:universeId/share
/settings
```

- Resolver carrega o universo pelo ID.
- Guard salva o editor antes de sair.
- URLs guardam IDs, nunca nomes mutáveis.
- Features pesadas são lazy-loaded.

### Testes reais

- abrir link profundo com app fechado;
- navegar durante autosave lento;
- renomear item sem quebrar URL;
- abrir URL de item excluído e mostrar estado recuperável;
- medir bundle inicial e garantir que grafo/configurações não carreguem na escrita.

### Rollback

Flag interna permite retornar temporariamente à navegação por Signals durante a fase, sem duplicar persistência.

## Fase 4 — Rust Application Core

### Entregas

- Criar módulos `application`, `domain`, `infrastructure/sqlite` e `interface/tauri`.
- Reutilizar `rusqlite`; não adicionar ORM sem necessidade comprovada.
- Toda conexão ativa `foreign_keys`, `busy_timeout` e transações curtas.
- Commands retornam DTOs, não linhas cruas.
- IDs de criação normal são gerados no Rust; import/sync usam fluxo separado e idempotente.

### Ordem de migração

1. Queries somente leitura e estatísticas.
2. Universo.
3. Timeline e planejamento.
4. Entidades, atributos e tags.
5. Relações e menções.
6. História e livro.
7. Capítulo, revisão e autosave.
8. Colaboração e aplicação de propostas.

### Testes reais

- teste de contrato Legacy Adapter versus Rust Adapter;
- transação revertida diante de erro no meio da operação;
- concorrência entre autosave e leitura;
- revisão criada antes de mudar capítulo;
- exclusão mantendo foreign keys e limpeza de metadados;
- execução dentro do Tauri, não apenas Angular.

### Critério de saída

Depois que nenhum componente depender do SQL do frontend, remover `sql:allow-execute` da capability.

## Fase 5 — Context Engine e IA confiável

### Entregas

- Contrato `AI Context v1` com orçamento explícito.
- Contexto prioriza tarefa e trecho; cânone é compactado.
- Memória criativa migra do `localStorage` para SQLite por importação idempotente.
- Provedor local e API própria usam o mesmo contrato de saída.
- Respostas vazias, eco, JSON inválido e timeout são tratados sem alterar texto.
- Resumo longo usa divisão em blocos e síntese final, sem exceder o contexto local.

### Testes reais

- suíte editorial: corrigir, reescrever, expandir, encurtar, nome, entidade JSON e resumo;
- rodar nos perfis Lite, Standard e Advanced quando disponíveis;
- mock de API compatível retornando sucesso, 401, 429, 500, timeout, JSON inválido e eco;
- texto selecionado nunca muda antes da confirmação;
- prompt máximo permanece dentro do orçamento;
- nenhum conteúdo de outro universo entra no contexto.

### Rollback

Manter leitura do `localStorage` por uma versão após a importação. A migration apenas adiciona tabelas.

## Fase 6 — Compartilhamento com Local Ownership

### Entregas

- Visualizador estático em Cloudflare Pages, sem Worker, D1, KV, R2 ou Durable Objects.
- Quick Tunnel permanece transporte comunitário e temporário.
- Link público guarda endpoint, sessão e chave somente no fragmento.
- Servidor Rust grava contribuição cifrada no SQLite antes de responder HTTP 201.
- Inbox local recupera contribuições após queda.
- Área própria: ativas, histórico e revisões pendentes.
- Visualizador embutido permanece fallback se o domínio estiver indisponível.

### Testes reais

- contrato HTTP completo do servidor local;
- CORS aceita apenas o visualizador oficial;
- endpoint, sessão e chave inválidos são recusados;
- contribuição confirmada sobrevive a encerramento forçado;
- fechamento invalida o link e preserva histórico local;
- túnel externo real valida `/health`, leitura, anotação e proposta;
- inspeção confirma ausência de chamadas a armazenamento em nuvem;
- Pages indisponível aciona o viewer embutido.

### Rollback

O transporte atual e viewer embutido continuam disponíveis até a nova rota passar em testes externos.

## Fase 7 — Eventos, tombstones e Sync V2

### Entregas

- Promover `sync_events` a outbox técnica, sem criar tabela concorrente.
- Registrar evento na mesma transação do comando de domínio.
- Cursores por dispositivo e consumidor.
- Tombstones para exclusões.
- Anexos transferidos por hash e blocos.
- Conflitos ficam pendentes para decisão humana.

### Testes reais

- dois bancos temporários simulam dois dispositivos;
- criação, edição e exclusão fora de ordem;
- reenvio do mesmo evento não duplica dados;
- entidade excluída não ressuscita;
- edição simultânea de capítulo cria conflito;
- desconexão no meio da transferência retoma pelo cursor;
- arquivo com hash incorreto é rejeitado.

### Rollback

Sync V1 continua selecionável durante uma versão, mas nunca processa tabelas V2 parcialmente.

## Fase 8 — Consolidação e releases graduais

### Entregas

- Remover adaptadores legados somente após uma versão estável usando Rust.
- Separar visualizador estático como aplicativo real do repositório, sem reorganizar todo o monorepo prematuramente.
- Atualizar documentação ativa e protocolos.
- Canary release antes da publicação geral.

### Testes reais

- matriz fresh install e upgrade das duas versões anteriores;
- Windows suportado e Android quando o runtime estiver disponível;
- instalação, atualização automática e desinstalação sem apagar dados;
- restauração de backup em nova instalação;
- pacote assinado, artefatos e release remoto verificados separadamente.

## Definição de concluído

Uma fase não está concluída quando apenas compila. Ela termina quando o comportamento foi exercitado no runtime Tauri, o banco anterior foi atualizado com sucesso, os dados foram reabertos após reinício e existe uma forma documentada e testada de recuperação.

## Regra de publicação

```text
COMPILA != FUNCIONA
FUNCIONA != ESTÁ SEGURO
ESTÁ SEGURO != É PUBLICÁVEL

Build
  ↓
Testes automatizados
  ↓
Migration em bancos representativos
  ↓
Runtime Tauri real
  ↓
Upgrade real da versão publicada anterior
  ↓
Reinício e reabertura dos dados
  ↓
Teste de backup e recuperação
  ↓
Empacotamento, assinatura e publicação
  ↓
Verificação dos artefatos remotos e do updater
```

Falha em qualquer etapa impede a publicação. Build local, instalador gerado, push no Git e release remota são evidências diferentes e devem ser verificadas separadamente. Uma release só é declarada disponível depois que seus artefatos, assinaturas e metadados do updater forem encontrados no destino público e um cliente de teste conseguir detectar a atualização.
