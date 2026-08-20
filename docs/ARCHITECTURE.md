# Arquitetura

## Objetivo

Manter o NarraHub funcional sem internet, com persistência por dispositivo e evolução controlada do esquema.

## Camadas

### Interface

Angular controla navegação, editor, temas e apresentação. Componentes não devem inventar conteúdo ou métricas. Todos os estados vazios representam o banco real.

### Persistência

SQLite é a fonte de verdade de cada dispositivo. Migrations são executadas pelo `tauri-plugin-sql` e ficam em `src-tauri/src/database/migrations.rs`.

O esquema principal contém:

- `universes`, `stories`, `books`, `chapters`;
- `entities`, `entity_attributes`, `entity_templates`;
- `relations`, `mentions`;
- `timeline_events`, `planning_items`;
- `chapter_revisions`, `change_log`;
- `devices`, `sync_peers`, `sync_events`, `sync_conflicts`.

### Integração nativa

Rust controla janela, rede local e operações que não devem ser expostas diretamente à interface. Novas regras críticas devem migrar gradualmente dos serviços SQL Angular para comandos Rust transacionais.

### Compartilhamento online

O NarraHub Share é um serviço opcional e separado do banco local. A interface cifra uma cópia de leitura com AES-256-GCM; o servidor armazena apenas o envelope cifrado e nunca participa da sincronização editável.

### Atualizações

O plugin updater do Tauri verifica um manifesto `latest.json` publicado no GitHub Releases. Artefatos sem assinatura válida são recusados. A configuração contendo a chave pública é injetada somente no build oficial de release.

## Regras

- IDs são UUIDs e não dependem de sequência local.
- Datas são armazenadas em UTC no formato ISO compatível com SQLite.
- Toda exclusão em cascata respeita foreign keys.
- Capítulos geram revisão antes de alteração de título ou conteúdo.
- Um snapshot remoto mais antigo não substitui um registro local mais novo.
- Conteúdo de capítulo alterado simultaneamente gera `sync_conflicts`.
- Dados de demonstração só podem existir em fixtures explícitas de teste.

## Limites atuais

- Parte do CRUD ainda usa `tauri-plugin-sql` diretamente no Angular.
- Exclusões ainda não são propagadas como tombstones entre dispositivos.
- A sincronização inicial não possui criptografia de transporte.
- Descoberta automática mDNS ainda não está implementada.

Esses limites são documentados para não transformar uma entrega parcial em uma promessa falsa.
