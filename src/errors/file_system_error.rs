use std::convert::TryInto;
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};

use rusqlite::Error as WrappedError;

use super::DatabaseError;

/// An error occurring during the access of the virtual file system.
#[derive(Debug, PartialEq)]
pub enum FileSystemError {
    /// The SQLite does neither contains a virtual file ststem neither should it be created.
    NoFileSystem,
    /// One of the underlying SQL statements is invalid. Should not occur in the wild.
    InvalidBaseCommand(&'static str, WrappedError),
    /// The virtual file system has a version not supported by this version of the library.
    UnsupportedVersion(u32),
    /// A general database error from SQLite.
    DatabaseError(DatabaseError),
}

impl super::Error for FileSystemError {}

impl From<WrappedError> for FileSystemError {
    fn from(error: WrappedError) -> Self {
        FileSystemError::DatabaseError(error.try_into().expect(DatabaseError::LOGIC_ERROR_MESSAGE))
    }
}

impl Display for FileSystemError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str("Error during loading of virtual file system from database: ")?;
        match self {
            FileSystemError::NoFileSystem => write!(f, "No virtual file system exists neither should it be created"),
            FileSystemError::InvalidBaseCommand(sql, _) => write!(f, "Preparing an base SQL command '{}' failed", sql),
            FileSystemError::UnsupportedVersion(version) => write!(f, "The version of the virtual file system '{}' is not compatible with the current library version", version),
            FileSystemError::DatabaseError(error) => write!(f, "The underlying database failed ('{}')", error)
        }
    }
}
