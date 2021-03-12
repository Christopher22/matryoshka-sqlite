use std::convert::TryFrom;
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};

use rusqlite::Error as WrappedError;
use rusqlite::ErrorCode as SQLiteError;

/// An error raised and described by SQLite.
#[derive(PartialEq, Eq)]
pub struct DatabaseError {
    error: SQLiteError,
    message: Option<String>,
}

impl DatabaseError {
    /// Message returned if SQLite does not specify an error.
    pub const MISSING_MESSAGE: &'static str = "<Unknown SQLite error>";
    /// Panic message returned if this library does not handle and logic error correctly.
    pub const LOGIC_ERROR_MESSAGE: &'static str = "Logic error during database access";
}

impl Debug for DatabaseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match &self.message {
            Some(message) => write!(f, "'{}' ({:?})", message, self.error),
            None => write!(f, "'{}' ({:?})", DatabaseError::MISSING_MESSAGE, self.error),
        }
    }
}

impl Display for DatabaseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match &self.message {
            Some(message) => f.write_str(message),
            None => f.write_str(DatabaseError::MISSING_MESSAGE),
        }
    }
}

impl TryFrom<WrappedError> for DatabaseError {
    type Error = ();

    fn try_from(value: WrappedError) -> Result<Self, Self::Error> {
        match value {
            WrappedError::SqliteFailure(error, message) => Ok(Self {
                error: error.code,
                message,
            }),
            _ => Err(()),
        }
    }
}
