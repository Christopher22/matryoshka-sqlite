//! Various errors occurring during access of the file system.
use std::convert::{TryFrom, TryInto};
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};
use std::io::{Error as IoError, ErrorKind};

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

impl From<WrappedError> for FileSystemError {
    fn from(error: WrappedError) -> Self {
        FileSystemError::DatabaseError(error.try_into().expect(DatabaseError::LOGIC_ERROR_MESSAGE))
    }
}

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

/// An error occurring during the loading of a file from the virtual file system.
#[derive(Debug, PartialEq)]
pub enum LoadingError {
    /// The requested file is not found in the virtual file system.
    FileNotFound,
    /// A general database error from SQLite.
    DatabaseError(DatabaseError),
}

impl From<WrappedError> for LoadingError {
    fn from(error: WrappedError) -> Self {
        LoadingError::DatabaseError(error.try_into().expect(DatabaseError::LOGIC_ERROR_MESSAGE))
    }
}

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
