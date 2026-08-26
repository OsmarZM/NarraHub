use serde::Serialize;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseErrorKind {
    Validation,
    NotFound,
    Conflict,
    Storage,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseCommandError {
    pub kind: DatabaseErrorKind,
    pub message: String,
}

impl DatabaseCommandError {
    pub fn new(kind: DatabaseErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(DatabaseErrorKind::Validation, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(DatabaseErrorKind::NotFound, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(DatabaseErrorKind::Conflict, message)
    }

    pub fn storage(message: impl Into<String>) -> Self {
        Self::new(DatabaseErrorKind::Storage, message)
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(DatabaseErrorKind::Unavailable, message)
    }
}

impl Display for DatabaseCommandError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for DatabaseCommandError {}

pub type DatabaseCommandResult<T> = Result<T, DatabaseCommandError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_a_stable_error_contract_for_the_frontend() {
        let error = DatabaseCommandError::conflict("Operação já está em andamento.");
        let value = serde_json::to_value(error).expect("serialize database error");
        assert_eq!(value["kind"], "conflict");
        assert_eq!(value["message"], "Operação já está em andamento.");
    }
}
