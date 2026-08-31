# Arquitetura

## Objetivo

Manter o NarraHub funcional sem internet, com persistência por dispositivo e evolução controlada do esquema.

Este documento descreve a arquitetura **corrente**. A fila de trabalho e a fase ativa estão em [`ai/PROJECT_STATE.md`](ai/PROJECT_STATE.md) e [`ai/ROADMAP.md`](ai/ROADMAP.md); as regras que todo agente segue estão em [`AGENTS.md`](../AGENTS.md). Decisões permanentes ficam registradas em [`ADR/`](ADR/). O plano histórico fatia a fatia continua em [`ARCHITECTURE_EVOLUTION_PLAN.md`](ARCHITECTURE_EVOLUTION_PLAN.md), com a numeração de fases antiga — a ordem de execução válida é a do roadmap.

O primeiro incremento de backup e diagnóstico da Fase 1 está descrito em [`BACKUP_AND_RECOVERY.md`](BACKUP_AND_RECOVERY.md).

## Fluxo

```text
Componente de feature
   ↓
Feature Store            estado da tela
   ↓
Gateway tipado           contrato do domínio, sem SQL
   ↓
RustCoreService          único lugar que chama invoke()
   ↓  invoke()
interface/tauri          DTO de entrada e saída
   ↓
application/*_service    caso de uso, transação
   ↓
domain                   invariantes
   ↓
repository
   ↓
SQLite
```

Nenhum arquivo do frontend executa SQL, e a capability `sql:allow-execute` foi removida.
Isso não é convenção: `tests/frontend-boundaries.test.mjs` reprova o build se qualquer
`.ts` sob `src/app/` voltar a conter SQL, se um adaptador legado reaparecer ou se um dos
serviços SQL removidos for recriado.

## Camadas

### Interface

Angular controla navegação, editor, temas e apresentação. Componentes não devem inventar conteúdo ou métricas. Todos os estados vazios representam o banco real.

### Persistência

SQLite é a fonte de verdade de cada dispositivo. Migrations são executadas pelo `tauri-plugin-sql` e ficam em `src-tauri/src/database/migrations.rs`.

O esquema principal contém:

- `universes`, `stories`, `books`, `chapters`;
- `entities`, `entity_attributes`, `entity_templates`;
- `content_tags`, `content_tag_assignments`;
- `relations`, `mentions`;
- `timeline_events`, `planning_items`, `planning_field_definitions`, `attachments`;
- `chapter_revisions`, `change_log`;
- `devices`, `sync_peers`, `sync_events`, `sync_conflicts`.
- `collaboration_sessions`, `collaboration_contributions`.

### Integração nativa

Rust é o núcleo de aplicação e domínio, não apenas uma camada de integração: ele controla
janela e rede local, mas também é quem detém as regras críticas e as transações.

```text
src-tauri/src/
├── interface/tauri/   comandos expostos ao frontend
├── application/       casos de uso por domínio
├── domain/            invariantes
├── infrastructure/    repositórios
└── database/          migrations
```

`src-tauri/src/commands/` é o caminho anterior, ainda presente. Sua remoção é a Fase 3 do
roadmap; até lá, **comando novo nasce em `interface/tauri/`**, nunca em `commands/`.

### Compartilhamento online

O NarraHub Share é um serviço opcional e separado da sincronização entre dispositivos. A interface cifra o workspace selecionado e as contribuições com AES-256-GCM; o servidor armazena apenas envelopes opacos. Anotações e propostas recebidas entram numa fila SQLite local, e o conteúdo canônico só muda após aprovação explícita.

### Atualizações

O plugin updater do Tauri verifica um manifesto `latest.json` publicado no GitHub Releases. Artefatos sem assinatura válida são recusados. A configuração contendo a chave pública é injetada somente no build oficial de release.

### Assistência à escrita

Ortografia, autocomplete, avatares de personagens e anotação por voz ficam na camada do editor. Tags categorizam qualquer conteúdo do universo. Campos de entidades continuam em `entity_attributes`; campos tipados do planejamento usam definições próprias por universo, valores escalares em JSON validado e relações normalizadas com foreign keys, sem misturar esses conceitos. Origem e destino de cena são colunas próprias do capítulo. A IA é opcional, pode usar o runtime local gerenciado ou uma API compatível configurada pelo usuário e recebe um contexto compacto; detalhes do contrato estão em `docs/WRITING_ASSISTANCE.md`.

## Regras

- IDs são UUIDs e não dependem de sequência local.
- Uma migration publicada ou aplicada nunca é alterada; qualquer evolução do esquema recebe uma nova versão para preservar o checksum registrado pelo `tauri-plugin-sql`.
- Contribuições de sessões compartilhadas entram numa fila local; conteúdo canônico só muda após aprovação explícita do autor.
- Datas são armazenadas em UTC no formato ISO compatível com SQLite.
- Toda exclusão em cascata respeita foreign keys.
- Capítulos geram revisão antes de alteração de título ou conteúdo.
- Um snapshot remoto mais antigo não substitui um registro local mais novo.
- Conteúdo de capítulo alterado simultaneamente gera `sync_conflicts`.
- Dados de demonstração só podem existir em fixtures explícitas de teste.
- Toda consulta de workspace é limitada pelo universo ativo; respostas assíncronas de um universo anterior são descartadas após a troca.

## Limites atuais

- `src-tauri/src/commands/` coexiste com `interface/tauri/`; há dois caminhos até o banco.
- `WorkspaceLayout` ainda orquestra domínios demais (preload, busca, sharing, backup, updates).
- Não existe teste que reprove uma variável CSS usada sem definição — foi a causa da 0.9.0/0.9.1.
- Exclusões ainda não são propagadas como tombstones entre dispositivos.
- A sincronização inicial não possui criptografia de transporte.
- Descoberta automática mDNS ainda não está implementada.
- A memória criativa atual registra apenas orientações explícitas e decisões aceitas, com limite e escopo por universo. Busca semântica/embeddings e o AI Router ainda não estão implementados.

Esses limites são documentados para não transformar uma entrega parcial em uma promessa falsa.
