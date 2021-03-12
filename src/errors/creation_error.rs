use std::convert::TryInto;
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};
use std::io::{Error as IoError, ErrorKind};

use rusqlite::Error as WrappedError;

use super::DatabaseError;

/// An error occurring during the creation of a file in the virtual file system.
#[derive(Debug, PartialEq)]
pub enum CreationError {
    /// A file already exists under this path.
    FileExists,
    /// The data source raised an error.
    SourceError(ErrorKind),
    /// A general database error from SQLite.
    DatabaseError(DatabaseError),
}

impl super::Error for CreationError {}

impl From<WrappedError> for CreationError {
    fn from(error: WrappedError) -> Self {
        CreationError::DatabaseError(error.try_into().expect(DatabaseError::LOGIC_ERROR_MESSAGE))
    }
}

impl From<IoError> for CreationError {
    fn from(error: IoError) -> Self {
        CreationError::SourceError(error.kind())
    }
}

impl Display for CreationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str("Error during file creation: ")?;
        match self {
            CreationError::FileExists => write!(f, "File does already exists"),
            CreationError::SourceError(error) => {
                write!(f, "The data source failed ('{:?}')", error)
            }
            CreationError::DatabaseError(error) => {
                write!(f, "The underlying database failed ('{}')", error)
            }
        }
    }
}
