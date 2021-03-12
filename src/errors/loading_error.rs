use std::convert::TryInto;
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};

use rusqlite::Error as WrappedError;

use super::DatabaseError;

/// An error occurring during the loading of a file from the virtual file system.
#[derive(Debug, PartialEq)]
pub enum LoadingError {
    /// The requested file is not found in the virtual file system.
    FileNotFound,
    /// A general database error from SQLite.
    DatabaseError(DatabaseError),
}

impl super::Error for LoadingError {}

impl From<WrappedError> for LoadingError {
    fn from(error: WrappedError) -> Self {
        LoadingError::DatabaseError(error.try_into().expect(DatabaseError::LOGIC_ERROR_MESSAGE))
    }
}

impl Display for LoadingError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str("Error during file loading: ")?;
        match self {
            LoadingError::FileNotFound => write!(f, "The requested file does not exist"),
            LoadingError::DatabaseError(error) => {
                write!(f, "The underlying database failed ('{}')", error)
            }
        }
    }
}
