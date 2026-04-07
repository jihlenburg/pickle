//! Verification command facade.
//!
//! The verification command surface is split into focused submodules so
//! datasheet lookup, provider-backed verification runs, and overlay/status
//! maintenance can evolve independently while preserving the public Tauri
//! command paths under `commands::verification::*`.

pub mod lookup;
pub mod overlay;
pub mod run;

pub use lookup::find_datasheet;
pub use overlay::{
    api_key_status, apply_overlay, delete_overlay_package, rename_overlay_package,
    set_package_display_name,
};
pub use run::{verify_clc, verify_pinout};
