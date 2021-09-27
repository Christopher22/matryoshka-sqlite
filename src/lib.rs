//! This crate provides virtual filesystems stored in a SQLite database.
#![deny(
    missing_docs,
    missing_debug_implementations,
    missing_copy_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unstable_features,
    unused_import_braces,
    unused_qualifications
)]

extern crate static_vcruntime;

pub mod errors;
mod file_system;
mod util;

pub use self::file_system::{File, FileSystem};
pub use self::util::Handle;
pub use rusqlite::Connection as Database;
