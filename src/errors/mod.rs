//! Various errors occurring during access of the file system.

use std::fmt::{Debug, Display};

mod database_error;

mod creation_error;
mod file_system_error;
mod loading_error;
mod read_error;

pub use self::creation_error::CreationError;
pub use self::database_error::DatabaseError;
pub use self::file_system_error::FileSystemError;
pub use self::loading_error::LoadingError;
pub use self::read_error::ReadError;

/// An error occurring while accessing the virtual file system.
pub trait Error: PartialEq + Debug + Display {
    /// Generate a human-readable version of the error.
    fn error_message(&self) -> String {
        format!("{}", &self)
    }
}
