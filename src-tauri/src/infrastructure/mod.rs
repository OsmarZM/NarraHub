//! Implementações concretas de persistência. É a única camada que conhece
//! `rusqlite`; domínio e aplicação falam em tipos, não em linhas.

pub mod identity_store;
pub mod sqlite;
