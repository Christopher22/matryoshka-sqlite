use std::convert::TryInto;
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};
use std::io::{Error as IoError, ErrorKind};

use rusqlite::Error as WrappedError;

use super::DatabaseError;

/// An error occurring during reading a file from the virtual file system.
#[derive(Debug, PartialEq)]
pub enum ReadError {
    /// The specified indices are out of bounds.
    OutOfBounds,
    /// The size of the indices or virtual files extend the bounds imposed by SQLite.
    FileSystemLimits,
    /// The sink written to raised an error.
    SinkError(ErrorKind),
    /// A general database error from SQLite.
    DatabaseError(DatabaseError),
}

impl super::Error for ReadError {}

impl From<WrappedError> for ReadError {
    fn from(error: WrappedError) -> Self {
        ReadError::DatabaseError(error.try_into().expect(DatabaseError::LOGIC_ERROR_MESSAGE))
    }
}

impl From<IoError> for ReadError {
    fn from(error: IoError) -> Self {
        ReadError::SinkError(error.kind())
    }
}

impl Display for ReadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str("Error during file reading: ")?;
        match self {
            ReadError::OutOfBounds => write!(f, "The specified indices are out of bounds"),
            ReadError::FileSystemLimits => write!(
                f,
                "The underlying database does not allow files of such size"
            ),
            ReadError::SinkError(error) => write!(f, "The data destination failed ('{:?}')", error),
            ReadError::DatabaseError(error) => {
                write!(f, "The underlying database failed ('{}')", error)
            }
        }
    }
}
