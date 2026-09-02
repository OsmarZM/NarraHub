//! Acesso SQLite do core Rust.

pub mod canvas_repository;
pub mod collaboration_repository;
pub mod connection;
pub mod entity_repository;
pub mod knowledge_repository;
pub mod manuscript_repository;
pub mod planning_repository;
pub mod sync_repository;
#[cfg(test)]
pub mod test_support;
pub mod universe_repository;
pub mod workspace_repository;

pub use connection::SqliteDatabase;
