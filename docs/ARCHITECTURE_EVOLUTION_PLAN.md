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
| Connections | Diagrama do grafo: elementos livres, layout e ligações visuais | `canvas_nodes`, `canvas_edges`, `canvas_entity_positions` |
| Planning | Timeline e kanban editorial | `timeline_events`, `planning_items` |
| Revision | Histórico, propostas e conflitos | `change_log`, `collaboration_contributions`, `sync_conflicts` |
| Assistance | Contexto, memória criativa e provedores de IA | armazenamento atual da IA; tabela local futura |
| Integrations | Share, colaboração, sync, atualização e plataforma | tabelas de colaboração/sync e módulos Rust existentes |

Uma entidade do tipo `Evento` representa um elemento de worldbuilding. `timeline_events` representa sua posição ou ocorrência na cronologia; os conceitos não devem ser fundidos.

Pela mesma razão, `relations` (Knowledge) e `canvas_edges` (Connections) são domínios distintos: `relations` guarda um FATO do universo ("X é irmão de Y") e aparece na ficha da entidade; `canvas_edges` guarda uma anotação de diagrama, cujas pontas podem ser elementos que não existem no cânone. Uma seta de uma imagem para uma nota não é conhecimento sobre o universo.

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

#### Ordem 2 — universo ✔

Comandos: `universe_create`, `universe_update`, `universe_delete`. O domínio de
Universo não delega mais nada ao legado.

Duas mudanças de comportamento, ambas para melhor e ambas com teste:

- **Criar virou uma gravação só.** O caminho antigo inseria e depois fazia um
  `UPDATE` separado quando havia capa — duas idas ao banco e uma janela em que
  o universo existia sem capa.
- **Excluir agora limpa em cascata.** Todas as FKs para `universes` são
  `ON DELETE CASCADE`, mas o `tauri-plugin-sql` não liga `foreign_keys`: o
  caminho antigo deixava histórias, livros, capítulos e entidades órfãos no
  arquivo. A conexão do core liga, então a cascata acontece.

O `UPDATE` monta o `SET` só com o que veio, porque `None` significa "não
mexer" e não "gravar vazio" — sem isso, salvar só o nome apagaria a capa.

#### Ordem 3 — timeline e planejamento ✔

Timeline: `timeline_create`, `timeline_rename`, `timeline_delete`.
Planejamento: `planning_list`, `planning_create`, `planning_delete`,
`planning_save_order`, `planning_field_links`, `planning_field_definitions`,
`planning_field_definition_create/rename/delete`.

`saveCard` continua no `planning_save_card` que já existia antes da Fase 4, em
`database/planning.rs`. Ele entra no core organizado junto da Ordem 4, porque
a validação cruzada que ele faz é sobre os campos de relação (`story`,
`character`, `tags`) — migrar as duas coisas em fatias separadas duplicaria a
regra.

`saveOrder` deixou de montar um `CASE` gigante com um placeholder por card.
Agora é um `UPDATE` por card dentro de uma transação, e a conferência que já
existia continua: se o número de linhas atingidas não bater com o número de
cards enviados, o quadro mudou entre o arrasto e a gravação — a transação é
revertida e o erro pede recarregar, em vez de gravar meia reordenação. Há
teste para o rollback, que é uma das exigências de "testes reais" do plano.

#### Ordens 4 e 5 — entidades, atributos, tags, relações e menções ✔

Entidades: `entity_list`, `entity_details`, `entity_create`, `entity_update`,
`entity_delete`, `entity_attribute_save`, `entity_attribute_delete`.
Tags e menções: `tags_list`, `tags_for_owner`, `tag_assignments`,
`tag_create`, `tag_set`, `tag_delete`, `mentions_list`, `mentions_sync`.
Relações: `relation_create`, `relation_delete`.

**Criar entidade virou uma transação.** Antes eram N gravações soltas — a
entidade, um `INSERT` por atributo padrão do tipo, um por template do universo
e mais um `UPDATE` para a imagem. Qualquer falha no meio deixava uma entidade
pela metade no arquivo do usuário, sem como desfazer. A ordem dos atributos é a
mesma de antes: padrões do tipo, depois os templates que o padrão não cobre,
depois o que veio do formulário.

**A lista de atributos padrão existe nos dois lados**, porque o Rust precisa
dela para montar a ficha e a tela precisa dela para desenhar o formulário. A
alternativa — o frontend mandar a lista no comando — deixaria o cliente decidir
o formato do dado gravado. `tests/rust-core-contract.test.mjs` compara as duas
listas; divergir quebra o teste.

**Menções: `INSERT OR IGNORE` não servia.** A tabela `mentions` não tem
`UNIQUE(chapter_id, entity_id)` — era o `SELECT` prévio do caminho antigo que
evitava a duplicata. A inserção agora é condicionada a um `NOT EXISTS`, e a
lista é deduplicada antes do banco (o texto salvo repete a mesma entidade
várias vezes). Manter a linha existente preserva o `created_at`, que é o que
ordena "onde apareceu pela primeira vez". O teste que pegou isso é o mesmo que
protege contra a regressão.

**Salvar atributo carimba a ficha na mesma transação.** Sem isso, uma falha
depois do atributo deixaria a entidade alterada com `updated_at` antigo, e a
sincronização decidiria que nada mudou.

`entity_details` traz as relações das duas pontas num `UNION ALL` com uma
coluna `is_source`, em vez de dois `SELECT` juntados em memória. A coluna não é
decoração: sem ela, uma relação de uma entidade com ela mesma apareceria
invertida na ficha, e "pai de" viraria "filho de".

A galeria de imagens continua no legado: anexo é `AttachmentService`,
compartilhado com capítulo e universo, e sai junto do último domínio que o usa.

#### Ordens 6 e 7 — história, livro, capítulo e autosave ✔

`story_*`, `book_*` e `chapter_*`. Os quatro `updateChapter*` do contrato viram
um comando só com patch parcial: `null` significa "esta tela não mexeu nisso".
Sem isso, o inspetor salvando o resumo sobrescreveria o texto que o editor
acabou de gravar.

`content` sem `word_count` é recusado. A estatística do universo soma
`word_count`, então gravar um sem o outro faz o total mentir até o próximo
salvamento — e o método antigo (`updateContent(id, content, wordCount)`) não
tinha como obrigar quem chamava a recontar.

**Revisões de capítulo não foram implementadas.** A tabela `chapter_revisions`
existe desde a migration 1 e **nunca teve escrita nenhuma**, nem no frontend
nem no Rust. Não havia o que migrar. Criar a revisão antes de sobrescrever o
capítulo é um recurso novo, com decisão de produto por trás (quando criar,
quanto guardar, como mostrar), e ficou fora desta fase de propósito.

#### Ordem 8 — colaboração e aplicação de propostas ✔

`collaboration_*`. É o domínio em que o valor vem de fora, de um convidado com
link, então é onde a fronteira importa mais:

- **A lista de campos graváveis virou código de domínio.** A tabela e a coluna
  saem de `domain::collaboration::writable_column`, **nunca** do texto
  recebido. O caminho antigo montava ``SET ${item.field} = ?`` interpolando o
  campo direto no SQL — era só a checagem da lista que separava isso de uma
  injeção.
- **Revisar virou uma transação.** Aplicar a mudança, registrar no histórico e
  marcar a proposta como decidida eram três comandos soltos: se a marcação
  falhasse depois de aplicar, a proposta continuava pendente e podia ser
  aplicada de novo, sobrescrevendo o que o autor tivesse escrito no meio.
- **Aprovar tudo dá uma transação por proposta**, para uma proposta que aponta
  para um capítulo já excluído não derrubar a aprovação das outras.

#### Ordem 9 — hardening pós-revisão de PR ✔

Revisão externa do PR #1 encontrou um bug funcional que os testes existentes
não pegavam. Esta fatia fecha os três pontos apontados.

**`canon_status` salvava em silêncio sem salvar.** `UpdateEntityInput` era um
`Partial<Pick<Entity, ...>>`, então mandava `canon_status`; o struct
`EntityUpdate` tem `rename_all = "camelCase"` e espera `canonStatus`. O serde
descarta chave desconhecida **sem erro**: o comando devolvia sucesso, a tela
mostrava o estado novo e o banco guardava o antigo, até o usuário reabrir a
ficha. O contrato virou uma `interface` camelCase explícita — assim o
compilador aponta o chamador, em vez de o erro aparecer só em runtime.

Auditei a fronteira inteira atrás do mesmo padrão: dos nove structs de entrada
com `rename_all`, só este divergia. O teste novo compara campo a campo cada
patch do gateway com o struct do Rust, e foi verificado reintroduzindo o bug
para confirmar que ele reprova.

**O N+1 de estatísticas não tinha morrido, só encolhido.** A revisão pegou o
comentário otimista: de seis consultas por universo para duas, ainda em laço.
Agora a biblioteca inteira sai em três consultas, independente de quantos
universos existam, e há teste garantindo que o resultado agrupado bate com o
cálculo universo a universo.

**O GitHub não validava nada.** O único workflow rodava por
`workflow_dispatch`, voltado a release. `.github/workflows/ci.yml` roda em todo
Pull Request: build Angular, os quatro conjuntos de teste do frontend, e
`cargo fmt --check`, `cargo clippy -D warnings` e `cargo test` no core. Os
cinco avisos de clippy que existiam — três meus, dois anteriores à Fase 4 —
foram corrigidos para o portão entrar já verde.

**Fica para a próxima fatia de hardening:** integridade entre universos. Hoje
as FKs garantem que a entidade existe, mas não que as duas pontas de uma
relação pertencem ao mesmo universo — vale igual para `timeline → entity`,
`mention → entity/chapter`, `planning → chapter` e as tags polimórficas. Já era
assim no legado, então não é regressão; mas agora que existe um core de
domínio, é ele que deve ser o guardião dessa regra, e isso precisa estar
resolvido **antes** de o sync trazer ids de outras máquinas.

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
2. Timeline e planejamento. *(Correção: só a Timeline recebeu gateway/store nesta fase. O quadro de Planejamento continuou falando com `PlanningService`/`MetadataService` direto e recebendo dados por `@Input` do layout — dívida só paga na Fase 3, quando ele precisou virar rota.)*
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

## Canvas livre de Conexões (recurso de produto, fora da sequência de fases)

Pedido do autor durante a Fase 3, não previsto no plano original. Está documentado aqui porque **mexeu no schema** (migration 14) e porque a separação que ele introduz é uma decisão de domínio que precisa sobreviver a quem chegar depois.

### O que é

A tela de Conexões deixou de ser só um grafo automático de entidades e virou um diagrama que o autor monta como quiser: arrastar os nós e a posição fica salva, acrescentar elementos que **não são entidades** (título, imagem, nota) e ligar qualquer coisa a qualquer coisa.

### Decisão de domínio: cânone ≠ diagrama

`relations` continua guardando **fatos do universo** — aparecem na ficha da entidade, com FK exigindo entidade nas duas pontas. As ligações do diagrama foram para `canvas_edges`, com pontas polimórficas (`entity` | `canvas`).

Duas razões, e a primeira é a que importa:

1. **Semântica.** Uma seta de uma imagem para uma nota não é um fato sobre o universo. Se ela entrasse em `relations`, apareceria na ficha do personagem como se fosse cânone.
2. **Custo técnico.** Aceitar pontas não-entidade em `relations` exigiria remover a FK, e o SQLite só faz isso reconstruindo a tabela — caminho que a ADR-0004 evita.

No grafo as duas convivem: a ligação de diagrama é **tracejada**, para não se confundir com uma relação de verdade.

### Schema (migration 14, aditiva)

| Tabela | Papel | Observação |
| --- | --- | --- |
| `canvas_nodes` | Título, imagem ou nota posicionados no diagrama | `CHECK` restringe `kind` |
| `canvas_entity_positions` | Onde cada entidade foi solta, por universo | Tabela à parte em vez de colunas em `entities`: apagar layout nunca pode arriscar dado canônico |
| `canvas_edges` | Ligações visuais, pontas polimórficas | Sem FK nas pontas; ver integridade abaixo |

`relations` **não foi tocada** pela migration — um teste de fronteira garante isso.

### Integridade sem FK

Como as pontas de `canvas_edges` são polimórficas, não há FK protegendo `source_id`/`target_id`. Duas defesas substituem:

- **na leitura:** `listEdges` só retorna ligação cujas duas pontas ainda existem, então uma entidade excluída por fora não deixa aresta quebrada na tela;
- **na exclusão:** apagar um elemento livre apaga as ligações dele na mesma operação — o que a FK faria.

Um teste Rust executa o SQL real do `CanvasService` contra o schema publicado, cobrindo o `ON CONFLICT` da posição, o filtro de órfãos e essa exclusão em cascata manual.

### Lição registrada: migration existir ≠ migration rodar

A `MIGRATION_V14` foi escrita, testada e **publicada na 0.7.6 sem nunca ser executada no app**. Ela estava em `migrations.rs` e no `match` de `sql_for_version` (usado por recovery e pelos testes Rust), mas não na lista do `tauri-plugin-sql` em `lib.rs` — que é o que de fato aplica migrations em runtime. As tabelas nunca eram criadas, todo INSERT falhava e "adicionar elemento" não fazia nada.

Os testes passavam porque chamavam `sql_for_version` direto, contornando o registro: provavam que o SQL era válido, não que o app algum dia o executaria. Existe agora um teste de fronteira comparando as versões declaradas em `migrations.rs` com as registradas em `lib.rs`.

### Pendente

Persistência real do canvas nunca foi confirmada no app Tauri empacotado — o `ng serve` não tem banco. Ao validar, o teste mínimo é: criar um título, fechar e reabrir o app, e conferir que ele voltou na mesma posição.

## Alcance dos campos do planejamento (recurso de produto, fora da sequência de fases)

Pedido do autor depois da Fase 4. Está documentado aqui porque **mexeu no schema** (migration 15) e porque a regra de visibilidade que ele introduz precisa valer no core, não só na tela.

### O que é

Toda propriedade criada no quadro valia para todos os cards do universo, e a tela não dizia isso: o autor criava um campo dentro de uma ficha e ele aparecia em todas. Agora o campo declara seu **alcance**:

- `universal` — aparece em todas as fichas do universo (o comportamento anterior, e o padrão);
- `card` — aparece só na ficha que o criou, guardada em `owner_item_id`.

A troca entre os dois é reversível pela própria ficha, sem tocar nos valores já preenchidos: promover expõe o campo vazio nas outras fichas, restringir devolve a propriedade ao card que a estava usando.

### Schema (migration 15, aditiva)

| Coluna | Papel | Observação |
| --- | --- | --- |
| `scope` | `universal` \| `card` | `DEFAULT 'universal'` — nenhuma linha existente muda de significado no upgrade |
| `owner_item_id` | O card dono, quando `scope = 'card'` | `NULL` obrigatório como default: o SQLite exige isso para adicionar coluna com `REFERENCES` |

Dois gatilhos (`BEFORE INSERT` e `BEFORE UPDATE OF`) recusam os dois estados inconsistentes: campo de card sem dono (sumiria de todas as fichas) e campo universal com dono (confundiria a leitura).

### A UNIQUE(universe_id, name) da v11 ficou como estava

Deliberadamente. Um nome de propriedade significa uma coisa só dentro do universo, e é isso que permite promover um campo de card para universal mexendo **só no alcance** — não existe homônimo em outro card para colidir. Relaxar a UNIQUE exigiria reconstruir a tabela, caminho que a ADR-0004 evita e que aqui não compraria nada.

### A regra de visibilidade vive no core

Filtrar na tela não bastaria: um card poderia gravar valor num campo privado de outro. Duas defesas no Rust:

- `planning_save_card` carrega apenas as definições universais mais as do próprio card, então um valor endereçado a campo de terceiro é recusado com erro, não ignorado em silêncio;
- `planning_field_definitions` aceita um `cardId` opcional e devolve a mesma fatia; sem ele devolve o catálogo do universo, que é o que a lista de propriedades gerencia.

### Exclusão do card

`planning_delete` roda numa transação: apaga as definições que pertenciam só àquele card e depois o card. As conexões do core ligam `foreign_keys`, então o `ON DELETE CASCADE` já cobriria isso; a limpeza explícita existe porque o pool do `tauri-plugin-sql` não liga a pragma.

### Pendente

Não foi validado no app Tauri empacotado nem num upgrade real a partir de um banco 0.8.0 com dados. O teste mínimo: abrir um universo que já tenha propriedades, conferir que todas continuam aparecendo em todas as fichas, criar uma "só neste card", reabrir o app e confirmar que ela não vazou para os outros cards.

## Fase 3 — Router e carregamento por feature

### Estado de implementação

A fundação do Router começou sem duplicar telas nem trocar a fonte de estado das features. A direção consolidada para a fase é:

```text
App (somente RouterOutlet)
└── RootLayout (titlebar e infraestrutura global)
    ├── /library
    ├── /settings
    └── WorkspaceLayout (/workspace/:universeId)
        ├── writing
        ├── entities
        ├── connections
        ├── timeline
        ├── planning
        └── history
```

O Router é a fonte do estado de navegação (`rota`, `universeId`, `section` e IDs profundos). Stores continuam sendo a fonte do estado de domínio. Signals locais continuam sendo a fonte do estado efêmero de UI. `AppState` não deve competir com a URL; sua redução será gradual conforme cada rota assumir o próprio ciclo de vida.

#### Fase 3.0 — infraestrutura e piloto de Settings

Implementada nesta fatia:

- `App` foi reduzido ao `RouterOutlet`; o shell anterior passou a ser um `RootLayoutComponent` roteado, sem reescrever seu visual;
- `/settings` passou a carregar `SettingsPageComponent` por `loadComponent`, como primeira feature lazy real;
- Settings foi mantida como rota global: não lê `AppState.activeUniverseId` nem recebe universo ativo implicitamente;
- as orientações de IA que são globais continuam em `/settings`; a memória criativa específica de universo fica reservada para uma futura rota explícita `/workspace/:universeId/settings`, sem misturar os dois escopos;
- ações de backup, restauração, update, sync e colaboração são orquestradas pela própria feature; o layout não faz binding de eventos de domínio da página roteada;
- metadados básicos de navegação (`navigationId` e `label`) foram colocados em `route.data`; a substituição do menu hardcoded por esse contrato ocorrerá junto do `WorkspaceLayout`;
- `/library` e `/workspace/:universeId/:section` ainda usam o ciclo de vida legado dentro do `RootLayout`. Isso é uma ponte temporária, não uma segunda arquitetura nem dual-write.

Não foi incluído nesta fatia: `WorkspaceLayout`, `AppBootstrapService`, resolver de universo, guard do editor ou migração das demais features para componentes roteados.

#### Fase 3.1 — WorkspaceLayout, bootstrap e resolver

Ordem obrigatória, sem inversão de dependências:

```text
3.1.1 AppBootstrapService
      ↓
3.1.2 provideAppInitializer
      ↓
3.1.3 WorkspaceLayout
      ↓
3.1.4 UniverseResolver
      ↓
3.1.5 route.data para sidebar/breadcrumb
```

- mover a inicialização global para `AppBootstrapService`;
- registrar o serviço com `provideAppInitializer`, garantindo que a inicialização termine antes de qualquer resolver consultar SQLite;
- criar `WorkspaceLayoutComponent` para sidebar, cabeçalho contextual e outlet do workspace;
- manter `UniverseResolver` pequeno: validar o ID, selecionar/carregar o universo e retornar estado recuperável; não carregar entidades, manuscrito, timeline, planejamento ou demais domínios;
- fazer menu e breadcrumb consumirem `route.data` como fonte única de rótulos e identidade de navegação;
- validar cold start e deep link no runtime Tauri, além do navegador.

O `UniverseResolver` nunca pode depender de inicialização ainda executada em `RootLayout.ngOnInit()`. Quando o resolver for registrado, o acesso ao banco necessário para resolvê-lo já deve estar garantido pelo initializer.

Estado do corte atual:

- **3.1.1 concluída:** `AppBootstrapService`, localizado em `app/bootstrap` como composition root (e não em `core`), assumiu inicialização de IA, SQLite, biblioteca, previews de Knowledge, colaboração e versão/update, além dos timers globais;
- **3.1.2 concluída:** `provideAppInitializer` aguarda esse serviço antes da navegação inicial; `RootLayout` não implementa mais `ngOnInit` nem inicializa banco/IA;
- **3.1.3 concluída:** `WorkspaceLayoutComponent` assumiu sidebar, cabeçalho contextual, modais e a única árvore legacy de páginas; Biblioteca ganhou host de rota lazy próprio e `RootLayout` ficou sem imports de páginas de domínio;
- o outlet do workspace já existe, mas permanece vazio até a migração unitária da primeira feature na Fase 3.2; a árvore legacy não possui uma segunda instância escondida;
- **3.1.4 concluída:** `UniverseResolver` valida o ID depois do initializer, reutiliza/recarrega apenas a lista de universos, seleciona o universo no estado de compatibilidade e redireciona falhas de bootstrap, IDs inválidos e universos inexistentes para a Biblioteca com mensagem recuperável; nenhum domínio do workspace é carregado pelo resolver;
- **3.1.5 concluída:** as seções do workspace são rotas explícitas; identidade ativa, labels, ícones, ordem da sidebar e label raiz do breadcrumb vêm de `route.data`. A rota curinga de seção volta para Escrita sem montar uma segunda árvore;
- build de produção e testes arquiteturais cobrem a composição do bootstrap, o limite do resolver e o consumo dos metadados. `cargo check` no `src-tauri` também foi validado (backend Rust compila sem erros). Cold start (`/`), deep link para rota global lazy (`/settings`) e deep link para universo inexistente (`/workspace/:id/...`) foram exercitados via `ng serve` em abas novas: o primeiro carrega limpo, o segundo carrega o chunk lazy sem erro de console, o terceiro cai no ramo de erro recuperável do resolver (mensagem "Não foi possível validar o universo desta rota." e redirect para `/library`) sem tela em branco — os erros de console nesse terceiro caso são só chamadas de banco tentadas fora do Tauri (`isTauri()` falso), esperado no navegador. **Falta** validar cold start/reload/deep link real dentro do app Tauri empacotado (IPC nativo, `tauri.qualification.conf.json`) antes da rota piloto de History — isso exige o app rodando localmente e não pôde ser feito de forma headless.

#### Fase 3.2 — rotas das features

Migrar, uma por vez, History, Timeline, Planning, Entities e Connections para filhos lazy de `WorkspaceLayout`. Cada página carrega o próprio domínio usando o `universeId` resolvido e descarta respostas atrasadas. Não manter a árvore antiga e a árvore roteada ativas ao mesmo tempo.

**Histórico concluído (piloto):**

- `provideRouter` ganhou `withComponentInputBinding()` + `withRouterConfig({ paramsInheritanceStrategy: 'always' })`: a partir de agora, qualquer rota filha de `/workspace/:universeId` que declare um `@Input()` com o mesmo nome de um parâmetro da rota (`universeId`) recebe o valor automaticamente, sem o layout precisar passar `[universeId]` manualmente. Essa é a infraestrutura que as próximas fatias (Timeline, Planning, Entities, Connections) reaproveitam sem repetir a configuração;
- a rota `history` trocou `children: []` por `loadComponent: () => import('./features/history/history-page.component')...` — `HistoryPageComponent` não mudou nenhuma linha própria, só passou a ser resolvida pelo Router em vez de instanciada pelo `@if` do `WorkspaceLayout`;
- `WorkspaceLayoutComponent` perdeu o import de `HistoryPageComponent` e o ramo `@else if (appState.workspaceView() === 'history')` do template; a página chega pelo `<router-outlet />` que já existia (antes vazio);
- teste de fronteira novo confirma a rota lazy, a ausência de uma segunda instância de `<app-history-page>` na árvore legacy, e que o contrato de `@Input()` do componente não mudou;
- build de produção mostra `history-page-component` como chunk lazy separado (13kB) e `workspace-layout-component` encolheu na mesma proporção (código de Histórico saiu do bundle eager do layout); `npm run test:architecture` (20/20); verificado via `ng serve`: deep link para um universo inexistente com `/history` no final continua caindo no mesmo ramo de erro recuperável do resolver (sem crash), e `/settings` continua funcionando após a mudança de configuração do `provideRouter`.

**Timeline concluído:** mais complexo que o piloto de Histórico porque a página recebia `entities`/`tagsByOwner` por `@Input()` do `WorkspaceLayout` e emitia `metadataRequested` por `@Output()` — nenhum dos dois sobrevive quando o pai de template deixa de existir (o Router é quem instancia a página agora). `TimelinePageComponent` passou a injetar `EntityStore`/`KnowledgeStore` diretamente (mesmo padrão que `ManuscriptStore`/`ConnectionsStore` já usavam em páginas com store próprio) e a chamar `KnowledgeStore.openMetadata(...)` + `AppState.openModal('metadata')` direto, no lugar de emitir um evento para o layout. O botão "+ Evento" do cabeçalho persistente também não alcança mais a página por `@ViewChild` (ela não é mais filha de view do `WorkspaceLayout`); o `<router-outlet>` ganhou `(activate)`/`(deactivate)` para capturar a instância ativa, com um type guard `supportsCreate()` evitando `any` espalhado — mecanismo que Planning/Entities/Connections reaproveitam quando migrarem. `npm run build` confirma `timeline-page-component` como chunk lazy próprio (23kB); `npm run test:architecture` (21/21); checado via `ng serve`: deep link para universo inexistente em `/timeline` cai no mesmo ramo recuperável do resolver.

**Planning, Entities, Connections e Writing concluídos — Fase 3.2 e 3.3 fechadas.**

- **Dívida da Fase 2 paga primeiro:** Planning nunca tinha sido extraído. O quadro falava com `PlanningService` e `MetadataService` diretamente e recebia tudo por `@Input` do layout. Ganhou `PlanningGateway`/`LegacyPlanningGateway` + `PlanningStore`, e passou a ler tags do `KnowledgeStore` (que ganhou `universeTags`, `tagsForOwner`, `setTagOnOwner` e `createTagForOwner` para donos fora do modal).
- **Coordenação cross-domain saiu dos layouts** para `application/workspace-sync.service.ts`, como a regra da fase exige: excluir entidade recarrega conexões/menções/estatísticas; salvar capítulo reindexa menções. Nenhuma feature conhece as outras — cada página chama o que precisa do service.
- **Writing (3.3)** ganhou `canDeactivate: [unsavedChapterGuard]`. O guard **não** substitui o `saveNow()` explícito: fechar janela, instalar atualização e restaurar backup acontecem fora do Router e continuam salvando por conta própria. O guard nunca bloqueia a saída — prender o usuário na tela não recupera texto.
- **Gatilho de criação do cabeçalho** deixou de usar `@ViewChild` (que não alcança página vinda do outlet) e passou a usar a instância entregue pelo `(activate)` do `<router-outlet>`, com o type guard `supportsCreate()`.
- **Armadilha encontrada e corrigida:** `withComponentInputBinding()` **sobrescreve com `undefined`** todo `@Input()` sem correspondente em params/data da rota — o inicializador da classe não protege. Isso quebrou o template de Conexões (`reading 'length' of undefined`) assim que a página deixou de receber `[entities]` do layout. Conexões e Planning passaram a ler esses dados dos stores por getter, e um teste de fronteira agora exige que toda página roteada declare **apenas** `@Input() universeId`.

Resultado no bundle: cada seção virou chunk lazy próprio e o `workspace-layout-component` caiu de ~790kB para **58kB** — ele não empacota mais nenhuma página.

Validação: `npm run build` sem avisos, `npm run test:architecture` (28/28, incluindo três testes novos de critério de saída), `test:ai` (5/5), `test:planning` (4/4), e as **30 transições** entre seções por clique real medindo geometria + URL + vazamento → 30/30, sem nenhum erro de console que não seja a ausência de banco no navegador. **Não coberto:** app Tauri empacotado.

**Bug real encontrado e corrigido depois do piloto de Histórico/Timeline:** usuário reportou que Histórico (e depois confirmou que Timeline também) "parou de funcionar" — tela em branco, sem cabeçalho, sem erro visível. Reproduzido manualmente via `ng serve` seedando um universo fake direto no `UniverseStore` pelas devtools do Angular (`window.ng`) e navegando por URL (não pela sidebar): o conteúdo de Escrita (a view padrão) e o de Histórico apareciam **ao mesmo tempo** na tela.

Causa raiz: `restoreRoute()` usava `route.navId === this.activeNav()` como sinal de "essa navegação já foi processada, não precisa rodar `selectNav()` de novo". Isso fazia sentido quando `activeNav` era um Signal escrito só por `selectNav()`. Depois que `activeNav` virou `computed(() => navigation.activeData().navigationId)` — derivado direto de `route.data` —, ele passou a refletir a rota nova assim que o Router resolve, **antes** de `selectNav()`/`returnToLibrary()`/`openSettings()` rodarem. A comparação sempre dava "já processado", e `AppState.workspaceView()` nunca era corrigido depois de um deep link ou reload direto numa seção roteada — ficava preso na view antiga (ex.: `'editor'`, valor padrão que `AppState.openUniverse()` seta) por baixo do `<router-outlet>`. Isso não aparecia num clique normal na sidebar porque `selectNav()` já seta `workspaceView` diretamente, então o guard (mesmo errado) nunca chegava a importar.

Correção parcial (1/3): trocado o uso de `activeNav()` como sinal de "já processado" por uma chave própria (`lastSyncedRouteKey`, formato `${navId}:${universeId}`) atualizada explicitamente por `selectNav()`/`returnToLibrary()`/`openSettings()`/`setWorkspaceNavigation()` no início de cada um — antes de qualquer `await` — e comparada em `restoreRoute()`.

**Essa correção não bastou — o usuário reportou que o problema persistia.** A validação que a acompanhou foi falha: usei extração de **texto** da página (`get_page_text`), que encontra conteúdo mesmo quando ele está com altura zero, fora da viewport ou cortado por `overflow`. Passar a **medir geometria** (`getBoundingClientRect()`) revelou mais dois defeitos, ambos produzindo a mesma tela em branco:

**Defeito 2 — o `<router-outlet>` renderizava fora da área visível.** O outlet estava declarado como irmão logo **depois** de `<section class="workspace-view">`. Essa seção é `height:100%` dentro de um `.workspace-route-content` com `overflow:hidden`, então ela consome toda a altura e o que vier depois é empurrado para fora. Medido: `.workspace-route-content` de 64 a 720, `section.workspace-view` ocupando os mesmos 656px, e `app-history-page` começando em **top=720** (altura 656, base 1376) — 100% abaixo da borda visível e cortada. A página existia no DOM (por isso o teste por texto a "encontrava") e era invisível. Corrigido movendo o outlet para **dentro** da seção, no mesmo slot de conteúdo que as páginas legacy ocupam. Depois da correção: `app-history-page` em top=129, altura 599, visível.

**Defeito 3 — duas fontes de verdade para "qual seção mostrar".** A URL alimentava o `<router-outlet>` e `appState.workspaceView()` alimentava o `@if` legacy. Capturado no navegador um estado real com URL em `/planning` e `workspaceView` em `'history'`: nenhum dos dois renderizava nada e a área de conteúdo ficava vazia. Corrigido fazendo o `@if` legacy (e os botões contextuais do cabeçalho) lerem `activeNav()` — derivado de `route.data`, a mesma fonte do outlet. `workspaceView()` permanece só como sub-estado **dentro** de uma seção (lista de entidades vs. ficha aberta), onde não compete com a URL. Isso torna a dessincronia não representável, em vez de mais uma camada de sincronização manual.

**Defeito 4 (achado no caminho) — carregar dados abortava a navegação.** `selectNav()` dá `await` em `loadPlanning()` **antes** de chamar `navigation.navigate()`; uma rejeição ali (banco indisponível, consulta falhando) abortava a troca de seção inteira, deixando a URL na seção anterior e o `workspaceView` na nova — exatamente a dessincronia do defeito 3. `loadPlanning()` passou a tratar o próprio erro.

Quatro testes de fronteira novos fixam os invariantes: outlet dentro de `.workspace-view`, `@if` legacy sem `appState.workspaceView()`, `loadPlanning()` com `try/catch`, e `restoreRoute()` sem comparar com `activeNav()`.

Validação: **todas as 30 transições possíveis** entre as seis seções, por clique real na sidebar, medindo geometria + URL + vazamento de outras páginas no DOM → 30/30 sem falha, todas as páginas em top=129/altura=599, nunca mais de uma página montada. `npm run build` e `npm run test:architecture` (25/25) limpos. **Não coberto:** voltar/avançar do navegador (o harness de universo fake usa `pushState` manual, que não é uma navegação real do Router, e o `back()` escapa para reload) e o runtime Tauri empacotado.

#### Fase 3.3 — Writing por último

Migrar Writing somente depois das demais rotas. `CanDeactivate` protege navegação Angular, mas não substitui o primitivo explícito `saveNow()`, que continua obrigatório para fechar janela, instalar atualização, restaurar backup e outros eventos fora do Router.

### Regra de orquestração cross-domain

Layouts não assumem regras de domínio nem coordenação permanente entre features. Dependências cross-domain — por exemplo, capítulo persistido → atualizar menções → atualizar estatísticas — devem migrar para um application service, facade ou evento explícito quando a feature correspondente for roteada. Esta regra não exige event bus nesta fase; ela impede apenas que a desmontagem do layout transfira coordenação de domínio para outro componente monolítico.

### Critério de saída da navegação híbrida

**Atendido.** `workspaceView` e os métodos `openEditor()`/`openEntityList()`/`openGraph()`/etc. foram removidos do `AppState` — ele guarda o universo ativo e estado de UI, não a rota. `currentView` sobreviveu apenas como 'home' vs 'workspace' para busca global e shell, sem escolher página. Testes de fronteira travam cada item abaixo:

- `RootLayout` não decide qual feature renderizar;
- `RootLayout` não importa páginas de domínio;
- não existe `@if` baseado em `workspaceView`/`currentView` para escolher páginas;
- `AppState` não representa a rota ativa;
- Router é a única fonte de verdade da navegação;
- `activeNav`, `currentView` e `workspaceView` foram removidos da seleção de rota.

### Entregas

```text
/library
/settings
/workspace/:universeId/writing/:chapterId?
/workspace/:universeId/entities/:entityId?
/workspace/:universeId/connections
/workspace/:universeId/timeline
/workspace/:universeId/planning
/workspace/:universeId/history
```

- Resolver valida e seleciona o universo pelo ID sem carregar todos os domínios.
- Guard salva o editor antes de navegação Angular; eventos nativos continuam chamando `saveNow()`.
- URLs guardam IDs, nunca nomes mutáveis.
- Features pesadas são lazy-loaded.
- `route.data` é a fonte de navegação e breadcrumb.

### Testes reais

- abrir link profundo com app fechado;
- navegar durante autosave lento;
- renomear item sem quebrar URL;
- abrir URL de item excluído e mostrar estado recuperável;
- medir bundle inicial e garantir que grafo/configurações não carreguem na escrita.
- abrir `/settings` e deep links de workspace com o aplicativo Tauri totalmente fechado;
- validar fallback de rota no protocolo de produção do Tauri, não apenas no `ng serve`;
- confirmar visual da versão 0.7.5 em `/library`, `/settings` e no shell do workspace.

Para cada feature convertida em rota, os gates obrigatórios são:

- o layout não possui import estático da página;
- o build produz chunk lazy separado;
- deep link funciona;
- reload e reabertura funcionam no Tauri;
- URL inválida ou ID inexistente produz estado recuperável;
- sair da rota destrói o componente e suas subscriptions;
- nenhuma segunda instância da feature permanece montada pela árvore legacy.

### Rollback

Flag interna permite retornar temporariamente à navegação por Signals durante a fase, sem duplicar persistência.

#### Fase 3.4 — Hardening (fechamento da Fase 3)

Revisão externa do repositório apontou que a Fase 3 tinha sido dada como
concluída com pontas soltas. Esta fatia fecha o que dependia só de código; o
que depende do app Tauri empacotado está listado como pendência explícita.

**Deep links `:chapterId` e `:entityId` — estavam nas Entregas e não existiam.**
Angular não tem parâmetro opcional: `/writing/:chapterId?` é, na prática, duas
rotas. Cada seção com deep link agora declara o par (`writing` e
`writing/:chapterId`), ambas com o mesmo `navigationId` e a variante com
parâmetro marcada com `hiddenFromMenu: true` — sem isso a sidebar mostraria o
item duas vezes, repetindo a queixa que apareceu em Conexões.

A página lê o parâmetro por `@Input()` (via `withComponentInputBinding()`) e
reflete a seleção de volta na URL com `replaceUrl: true`: trocar de capítulo é
seleção dentro da seção, não navegação entre telas, e não deve encher o
histórico do voltar. Id ausente ou já excluído cai na seleção padrão em vez de
erro — um link antigo não pode travar a tela.

Isso obrigou a **generalizar o teste de fronteira de `@Input`**: em vez de uma
lista fixa (`['universeId']`), ele agora deriva de `app.routes.ts` quais params
cada página realmente recebe e reprova o inverso — `@Input` declarado sem
parâmetro correspondente, que vira `undefined` em runtime.

**Carga duplicada dos stores.** Só `EntityStore` tinha guarda de deduplicação.
O layout pré-carrega os cinco domínios e o `ngOnChanges` de cada página chamava
`load()` de novo logo em seguida — o mesmo SQL duas vezes por entrada de seção.
Todos os stores passaram a expor `load(universeId, force = false)` com saída
antecipada; quem quer recarregar de verdade (abrir universo, refresh após
mutação) passa `force: true`. Em `TimelineStore` a guarda precisa vir **antes**
de tocar em `loadRevision`: sair depois de incrementá-lo invalidaria a carga em
voo e deixaria a tela vazia.

O pré-carregamento **não** foi removido, e sim justificado no código: a busca
global do cabeçalho é cross-domain e precisa achar um capítulo que o usuário
nunca visitou. Fica um `TODO(Fase 4)` para trocá-lo por um índice próprio.

**Responsabilidades do WorkspaceLayout.** A busca global saiu para
`application/global-search.service.ts`, junto do `WorkspaceSyncService` — ela
cruza cinco domínios, então não pertence nem a um domínio nem ao layout. Junto
com isso caíram os membros que ficaram órfãos quando as páginas viraram rota
(`onManuscriptEntityOpen`, `onConnectionsInfo/Failed`, `onSettingsFailed`,
`openPlanningChapter`, `manuscriptStories/Chapters` e os aliases de signal que
só a busca usava). O layout continua grande; o resto da redução depende da
Fase 4 e não vale uma refatoração grande agora.

**CI.** O workflow de release passou a rodar `test:architecture`, `test:ai` e
`test:planning` além do build — antes os testes de fronteira só existiam para
quem rodasse localmente.

**Validado:** rotas casam sozinhas (a URL do deep link sobrevive ao load e o
`**` só captura o que sobra), build limpo, 26 testes de fronteira, 29 de
arquitetura, 5 de IA, 4 de planejamento e 40 no Rust.

**Pendente — exige o app Tauri empacotado, que o `ng serve` não cobre:**
abrir deep link com o app fechado, migration 13→14 num banco existente,
navegação back/forward, e autosave interrompido por troca de rota.

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

### Progresso

Fatia por fatia, na ordem de migração acima. O que está aqui já roda no app.

#### Estrutura (feita junto com a Ordem 1)

```text
src-tauri/src/
├── domain/                  tipos e regras; não conhece rusqlite nem tauri
├── application/             casos de uso; abre a conexão que a operação exige
├── infrastructure/sqlite/   único lugar que fala rusqlite
└── interface/tauri/         #[tauri::command]; só traduz argumento e erro
```

Decisões que precisam sobreviver:

- **Sem pool próprio.** SQLite abre conexão em microssegundos e o plano pede
  transações curtas. Um pool nosso conviveria mal com o pool que o
  `tauri-plugin-sql` ainda mantém aberto durante a migração gradual — o banco
  está em WAL desde a migration 1, e é isso que permite os dois convivendo.
- **`read()` abre somente-leitura de propósito.** Comando de consulta que
  grave por engano falha na hora, em vez de gravar em silêncio.
- **Serialização em `snake_case`.** Os modelos TypeScript já espelham as
  colunas; renomear para `camelCase` só na fronteira Rust obrigaria a
  reescrever templates sem ganho. Entrada de comando é a exceção, em
  `camelCase`, como `planning_save_card` já fazia.
- **Teste usa o schema real**, aplicando as migrations num banco em memória.
  Schema desenhado à mão no teste esconde justamente o defeito que este core
  precisa pegar — e pegou: `sort_key` é `REAL`, não inteiro, porque a
  ordenação da timeline é fracionária para permitir inserir um evento entre
  dois outros sem renumerar tudo.

No frontend, o adaptador `rust-<domínio>.gateway.ts` implementa o mesmo
contrato do legado e **delega ao legado o que ainda não migrou**. A lista de
delegações está documentada no topo de cada adaptador e encolhe a cada fatia;
chegar a zero é o sinal de que o `legacy-*` pode sumir. `RustCoreService` é a
única porta para `invoke()` — ela existe porque `invoke()` rejeita com o objeto
serializado do Rust, não com um `Error`, e sem normalizar isso todo `catch` do
frontend mostraria `undefined` na tela.

`tests/rust-core-contract.test.mjs` cobre os três riscos da convivência:
adaptador que perde um método na troca, comando chamado que não existe no
Rust, e — o que já custou caro com a migration v14 — comando que existe mas
não foi registrado no `invoke_handler`, que compila dos dois lados e só falha
no clique do usuário.

#### Ordem 1 — queries somente leitura e estatísticas ✔

Comandos: `universe_list`, `universe_get`, `universe_stats`, `timeline_list`,
`relations_list`, `history_list`.

As estatísticas deixaram de ser N+1: o frontend fazia seis `SELECT` por
universo e repetia os seis para cada linha da biblioteca. Agora as contagens
escalares saem de uma query só, e apenas a quebra por tipo de entidade
continua separada, porque só ela é agrupada.

#### Ordem 10 — canvas, anexos e o fim da camada legada ✔

O canvas e os anexos não estavam na ordem de migração original — o canvas
nasceu depois do plano, e anexo é compartilhado entre entidade, capítulo e
universo. Mas eram os dois últimos pontos de SQL no frontend, então sem eles o
critério de saída da fase não fechava.

Comandos: `canvas_nodes`, `canvas_node_create/update/delete/position`,
`canvas_entity_positions`, `canvas_entity_position_save`,
`canvas_layout_clear`, `canvas_edges`, `canvas_edge_create/delete`,
`attachments_list`, `attachment_create`, `attachment_delete`.

**A integridade do canvas passou a valer também na gravação.** As pontas das
ligações são polimórficas, então não há FK; antes a defesa era só na leitura,
que descarta ligação órfã. Isso escondia o problema em vez de evitá-lo: a
ligação inválida entrava no arquivo, sumia da tela pelo filtro e ficava lá para
sempre. Agora as duas pontas são conferidas antes de gravar, e excluir um
elemento apaga as ligações dele na mesma transação.

**O anexo devolve a posição calculada pelo banco.** Ela sai de uma subquery no
`INSERT`; devolver o zero montado na memória faria a imagem nova aparecer no
começo da galeria até a próxima recarga.

**A camada de serviços SQL foi removida.** Doze `*.service.ts` e os dez
`legacy-*.gateway.ts` deixaram de existir. Os tipos que os contratos importavam
de dentro deles (`CollaborationSession` e vizinhos, `PlanningCardUpdate`)
viraram modelos de domínio. `RelationService` já estava morto antes disso —
nada o importava.

`DatabaseService` sobreviveu **sem executar SQL**, reduzido ao ciclo de vida do
pool: o `tauri-plugin-sql` ainda aplica as migrations na abertura, e restaurar
backup precisa fechar o pool antes de trocar o arquivo no disco.

### Critério de saída ✔

`sql:allow-execute` **foi removido** de `src-tauri/capabilities/default.json`.
`sql:default` fica, pelos dois motivos acima.

Três testes de fronteira seguram a decisão: nenhum arquivo do frontend contém
SQL, a camada de serviços legada não pode voltar, e a capability não pode
reganhar a permissão sem que o SQL volte junto.


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
