//! Library root for pickle's Rust backend.
//!
//! Re-exports the backend modules used by the Tauri entrypoint and keeps the
//! testable logic available without going through the binary crate.

pub mod codegen;
pub mod commands;
pub mod parser;
pub mod part_profile;
pub mod settings;
