//! Casos de uso do NarraHub.
//!
//! A camada de aplicação existe para que o comando Tauri não vire o lugar onde
//! a regra mora. O comando só traduz argumento e erro; quem decide o que a
//! operação faz — e em que transação — é daqui.
//!
//! Cada caso de uso recebe o `SqliteDatabase` e abre a conexão que precisa:
//! leitura abre somente-leitura, escrita abre transação curta. O plano da Fase
//! 4 pede exatamente isso, e ter as duas coisas separadas é o que permite
//! testar a regra contra um banco em memória.

pub mod entity_service;
pub mod knowledge_service;
pub mod planning_service;
pub mod universe_service;
pub mod workspace_service;
