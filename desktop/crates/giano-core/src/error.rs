//! Errors. `Validation` and `Unavailable` carry a user-facing pt-BR message: the UI shows
//! them like any refused operation (nothing typed is lost).

use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Invalid user data or a refused operation (pt-BR message).
    #[error("{0}")]
    Validation(String),
    /// Turso could not be reached or refused the access (no internet, bad token...).
    #[error("{0}")]
    Unavailable(String),
    /// A constraint of the schema was violated.
    #[error("{0}")]
    Integrity(String),
    /// Any other database error.
    #[error("{0}")]
    Database(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Template(String),
}

impl Error {
    pub fn validation(message: impl fmt::Display) -> Self {
        Error::Validation(message.to_string())
    }

    /// True for the errors whose message is meant for the user as it is.
    pub fn is_user_facing(&self) -> bool {
        matches!(self, Error::Validation(_) | Error::Unavailable(_))
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        use rusqlite::ErrorCode;
        match &e {
            rusqlite::Error::SqliteFailure(f, _) if f.code == ErrorCode::ConstraintViolation => {
                Error::Integrity(e.to_string())
            }
            rusqlite::Error::SqliteFailure(f, _)
                if matches!(f.code, ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) =>
            {
                Error::Validation(
                    "O banco de dados está ocupado (outra janela ou o terminal está gravando). \
                     Tente de novo."
                        .into(),
                )
            }
            _ => Error::Database(e.to_string()),
        }
    }
}

impl From<minijinja::Error> for Error {
    fn from(e: minijinja::Error) -> Self {
        Error::Template(format!("{e:#}"))
    }
}
