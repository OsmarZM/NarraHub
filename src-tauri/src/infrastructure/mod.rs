//! Implementações concretas de persistência. É a única camada que conhece
//! `rusqlite`; domínio e aplicação falam em tipos, não em linhas.

pub mod blob_document;
pub mod blob_store;
pub mod identity_store;
pub mod sqlite;
pub mod sync_pairing;
pub mod sync_pake;
pub mod sync_transport;
