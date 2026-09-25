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

pub mod arranque;
pub mod atualizacao_android;
pub mod blob_fields;
pub mod blob_upgrade;
pub mod canvas_service;
pub mod cobertura_total;
pub mod collaboration_service;
pub mod conflitos;
pub mod entity_service;
pub mod epoca;
#[cfg(test)]
mod epoca_testes;
pub mod genese;
#[cfg(test)]
mod hardening_testes;
pub mod knowledge_service;
pub mod legado_recuperacao;
#[cfg(test)]
mod legado_testes;
pub mod manuscript_service;
pub mod mutacao;
pub mod planning_service;
pub mod resolucao_divergencia;
#[cfg(test)]
mod resolucao_testes;
pub mod sync_bootstrap;
#[cfg(test)]
mod sync_manuscrito_testes;
pub mod sync_panorama;
pub mod sync_pin_pairing;
pub mod sync_sessao;
pub mod universe_service;
pub mod workspace_service;
