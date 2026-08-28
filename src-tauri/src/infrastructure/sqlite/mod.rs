//! Acesso SQLite do core Rust.

pub mod connection;
#[cfg(test)]
pub mod test_support;
pub mod universe_repository;
pub mod workspace_repository;

pub use connection::SqliteDatabase;
