//! This crate provides virtual filesystems stored in a SQLite database.
#![allow(dead_code)]
#![deny(missing_docs)]

pub mod errors;
/// cbindgen:ignore
pub mod file_system;
mod util;

mod ffi;
pub use self::ffi::*;
