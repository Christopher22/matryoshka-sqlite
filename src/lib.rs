//! This crate provides virtual filesystems stored in a SQLite database.
#![allow(dead_code)]
#![deny(missing_docs)]

pub mod errors;
mod file_system;
mod util;

pub use self::file_system::*;
