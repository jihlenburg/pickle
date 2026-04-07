//! Small cross-command helpers.
//!
//! These helpers are shared by multiple command handlers but do not belong to
//! any one command domain. Keeping them out of `commands.rs` lets the root stay
//! focused on module wiring and re-exports.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use serde_json::Value;

use crate::parser::edc_parser::DeviceData;

use super::DialogFilterRequest;

pub(crate) fn apply_dialog_filters<R: tauri::Runtime>(
    mut dialog: tauri_plugin_dialog::FileDialogBuilder<R>,
    filters: &[DialogFilterRequest],
) -> tauri_plugin_dialog::FileDialogBuilder<R> {
    for filter in filters {
        let extensions: Vec<&str> = filter.extensions.iter().map(String::as_str).collect();
        dialog = dialog.add_filter(filter.name.clone(), &extensions);
    }
    dialog
}

pub(crate) fn resolve_dialog_path(
    file_path: tauri_plugin_dialog::FilePath,
) -> Result<PathBuf, String> {
    file_path
        .into_path()
        .map_err(|e| format!("Invalid file path returned by dialog: {e}"))
}

pub(crate) fn write_text_file(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Cannot create directory: {e}"))?;
    }
    fs::write(path, contents).map_err(|e| format!("Cannot write file: {e}"))
}

pub(crate) fn round_tenths(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

pub(crate) fn parse_u32_keyed_map<T>(source: HashMap<String, T>) -> HashMap<u32, T> {
    source
        .into_iter()
        .filter_map(|(key, value)| key.parse::<u32>().ok().map(|parsed| (parsed, value)))
        .collect()
}

fn is_synthetic_package_name(name: &str) -> bool {
    name.trim().eq_ignore_ascii_case("default")
}

fn synthetic_package_replacement<'a>(
    device: &'a DeviceData,
    requested: Option<&str>,
) -> Option<&'a str> {
    let requested_name = requested.unwrap_or(&device.default_pinout);
    if !is_synthetic_package_name(requested_name) {
        return None;
    }

    let synthetic = device.pinouts.get(requested_name)?;
    let mut candidates: Vec<&str> = device
        .pinouts
        .iter()
        .filter_map(|(name, pinout)| {
            if is_synthetic_package_name(name) || pinout.pin_count != synthetic.pin_count {
                return None;
            }
            Some(name.as_str())
        })
        .collect();
    candidates.sort_unstable();

    if candidates.len() == 1 {
        return candidates.into_iter().next();
    }

    None
}

pub(crate) fn selected_package<'a>(device: &'a DeviceData, package: Option<&'a str>) -> &'a str {
    synthetic_package_replacement(device, package)
        .or_else(|| package.filter(|pkg_name| device.pinouts.contains_key(*pkg_name)))
        .unwrap_or(&device.default_pinout)
}

pub(crate) fn device_packages(device: &DeviceData) -> HashMap<String, Value> {
    device
        .pinouts
        .iter()
        .map(|(name, pinout)| {
            (
                name.clone(),
                serde_json::json!({
                    "pin_count": pinout.pin_count,
                    "source": pinout.source,
                    "display_name": pinout.display_name,
                }),
            )
        })
        .collect()
}

pub(crate) fn encode_base64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub(crate) fn file_name_or(path: &Path, fallback: &str) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| fallback.to_string())
}
