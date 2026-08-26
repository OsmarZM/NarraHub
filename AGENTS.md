# NarraHub: regras permanentes de desenvolvimento

- O NarraHub é local-first; o SQLite local é a fonte canônica dos dados do usuário.
- Preserve o monólito modular. Não introduza microserviços ou reescritas amplas sem uma necessidade comprovada.
- O fluxo-alvo do frontend é `Angular UI -> Feature Store -> Gateway tipado -> Tauri/Rust`.
- Código novo de UI não deve conhecer SQL, tabelas ou detalhes do plugin SQLite.
- Adapters legados podem encapsular serviços Angular/SQL durante a migração incremental.
- Operações críticas, invariantes e transações devem migrar para comandos Rust testáveis.
- Migrations publicadas são imutáveis: nunca altere uma migration já aplicada; adicione uma nova.
- Preserve compatibilidade com bancos, backups, configurações e atualizações existentes.
- Não troque framework, banco ou layout global como parte de uma extração arquitetural.
- Antes de concluir uma mudança, execute os testes relevantes, o build Angular e as verificações Rust disponíveis.

