//! Shared DFP/cache filesystem path policy.
//!
//! This module centralizes root discovery and write-location rules so pack
//! caches, overlay files, and datasheet artifacts all follow the same read/write
//! precedence without repeating path assembly logic across parser modules.

use std::fs;
use std::path::{Path, PathBuf};

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn read_roots() -> Vec<PathBuf> {
    let mut roots = vec![project_root()];

    let app_data = base_dir();
    if !roots.contains(&app_data) {
        roots.push(app_data);
    }

    roots
}

pub fn base_dir() -> PathBuf {
    // Use platform-standard app data directory:
    //   macOS:   ~/Library/Application Support/pickle
    //   Linux:   ~/.local/share/pickle
    //   Windows: %APPDATA%/pickle
    let dir = dirs::data_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("pickle");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn preferred_write_root(subdir: &str) -> PathBuf {
    // Keep writes colocated with the data the app is already reading. Otherwise
    // the user can end up reading stale overlays from one root and writing new
    // ones into another.
    for root in [project_root()] {
        if root.as_os_str().is_empty() {
            continue;
        }
        if root.join(subdir).exists() {
            return root;
        }
    }
    base_dir()
}

pub fn devices_dir() -> PathBuf {
    preferred_write_root("devices").join("devices")
}

pub fn dfp_cache_dir() -> PathBuf {
    preferred_write_root("dfp_cache").join("dfp_cache")
}

pub fn datasheets_dir() -> PathBuf {
    let dir = dfp_cache_dir().join("datasheets");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub(crate) fn datasheet_pdf_cache_dir() -> PathBuf {
    let dir = datasheets_dir().join("pdf");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub(crate) fn datasheet_pdf_cache_path(part_number: &str) -> PathBuf {
    datasheet_pdf_cache_dir().join(format!("{}.pdf", part_number.trim().to_uppercase()))
}

pub fn pinouts_dir() -> PathBuf {
    preferred_write_root("pinouts").join("pinouts")
}

pub fn clc_sources_dir() -> PathBuf {
    preferred_write_root("clc_sources").join("clc_sources")
}
