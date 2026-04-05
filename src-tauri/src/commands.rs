//! Tauri command handlers — replace all FastAPI REST endpoints.

pub mod catalog;
pub mod devices;
pub mod dialogs;
pub mod keychain;
pub mod settings_state;
pub mod toolchain;
pub mod verification;

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::codegen::generate::ClcModuleConfig;
use crate::parser::edc_parser::DeviceData;
use crate::settings;

pub use catalog::{index_status, list_devices, refresh_index};
pub use devices::{generate_code, load_device};
pub use dialogs::{
    delete_file_path, export_generated_files_dialog, open_binary_file_dialog,
    open_text_file_dialog, save_text_file_dialog, write_text_file_path,
};
pub use settings_state::{load_app_settings, remember_last_used_device, set_theme_mode};
pub use toolchain::{compile_check, compiler_info};
pub use keychain::{api_key_details, delete_api_key, save_api_key};
pub use verification::{api_key_status, apply_overlay, find_datasheet, verify_pinout};

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct DeviceListResponse {
    pub devices: Vec<String>,
    pub cached: Vec<String>,
    pub total: usize,
    pub cached_count: usize,
}

#[derive(Serialize)]
pub struct RefreshResponse {
    pub success: bool,
    pub device_count: usize,
    pub pack_count: usize,
    pub age_hours: f64,
}

#[derive(Serialize)]
pub struct IndexStatusResponse {
    pub available: bool,
    pub device_count: usize,
    pub pack_count: usize,
    pub age_hours: Option<f64>,
    pub is_stale: bool,
}

#[derive(Serialize)]
pub struct CompilerResponse {
    pub available: bool,
    pub command: String,
    pub device_family: String,
    pub path: Option<String>,
    pub version: Option<String>,
}

#[derive(Serialize)]
pub struct CompileCheckResponse {
    pub success: bool,
    pub command: String,
    pub device_family: String,
    pub errors: String,
    pub warnings: String,
}

#[derive(Serialize)]
pub struct ApiKeyStatusResponse {
    pub configured: bool,
    pub hint: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsResponse {
    pub path: String,
    pub settings: settings::AppSettings,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenTextFileResponse {
    pub path: String,
    pub contents: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenBinaryFileResponse {
    pub path: String,
    pub name: String,
    pub base64: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedPathResponse {
    pub path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportFilesResponse {
    pub directory: String,
    pub written_files: Vec<String>,
}

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignmentRequest {
    pub pin_position: u32,
    pub rp_number: Option<u32>,
    pub peripheral: String,
    pub direction: String,
    pub ppsval: Option<u32>,
    #[serde(default)]
    pub fixed: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OscRequest {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub target_fosc_mhz: f64,
    #[serde(default)]
    pub crystal_mhz: f64,
    #[serde(default = "default_poscmd")]
    pub poscmd: String,
}

fn default_poscmd() -> String {
    "EC".to_string()
}

/// Dynamic fuse selections: `{ register_cname: { field_cname: value_cname } }`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FuseRequest {
    #[serde(default)]
    pub selections: HashMap<String, HashMap<String, String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodegenRequest {
    pub part_number: String,
    pub package: Option<String>,
    pub assignments: Vec<AssignmentRequest>,
    #[serde(default)]
    pub signal_names: HashMap<String, String>,
    #[serde(default)]
    pub digital_pins: Vec<u32>,
    pub oscillator: Option<OscRequest>,
    pub fuses: Option<FuseRequest>,
    pub clc: Option<HashMap<String, ClcModuleConfig>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileCheckRequest {
    pub code: String,
    #[serde(default)]
    pub header: String,
    pub part_number: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOverlayRequest {
    pub part_number: String,
    pub packages: HashMap<String, Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogFilterRequest {
    pub name: String,
    #[serde(default)]
    pub extensions: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenFileDialogRequest {
    pub title: Option<String>,
    #[serde(default)]
    pub filters: Vec<DialogFilterRequest>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveTextFileRequest {
    pub title: Option<String>,
    pub suggested_name: String,
    pub contents: String,
    #[serde(default)]
    pub filters: Vec<DialogFilterRequest>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportFilesRequest {
    pub title: Option<String>,
    pub files: HashMap<String, String>,
}

fn apply_dialog_filters<R: tauri::Runtime>(
    mut dialog: tauri_plugin_dialog::FileDialogBuilder<R>,
    filters: &[DialogFilterRequest],
) -> tauri_plugin_dialog::FileDialogBuilder<R> {
    for filter in filters {
        let extensions: Vec<&str> = filter.extensions.iter().map(String::as_str).collect();
        dialog = dialog.add_filter(filter.name.clone(), &extensions);
    }
    dialog
}

fn resolve_dialog_path(file_path: tauri_plugin_dialog::FilePath) -> Result<PathBuf, String> {
    file_path
        .into_path()
        .map_err(|e| format!("Invalid file path returned by dialog: {e}"))
}

fn write_text_file(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Cannot create directory: {e}"))?;
    }
    fs::write(path, contents).map_err(|e| format!("Cannot write file: {e}"))
}

fn round_tenths(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn parse_u32_keyed_map<T>(source: HashMap<String, T>) -> HashMap<u32, T> {
    source
        .into_iter()
        .filter_map(|(key, value)| key.parse::<u32>().ok().map(|parsed| (parsed, value)))
        .collect()
}

fn is_synthetic_package_name(name: &str) -> bool {
    name.trim().eq_ignore_ascii_case("default")
}

fn synthetic_package_replacement<'a>(device: &'a DeviceData, requested: Option<&str>) -> Option<&'a str> {
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

fn selected_package<'a>(device: &'a DeviceData, package: Option<&'a str>) -> &'a str {
    synthetic_package_replacement(device, package)
        .or_else(|| {
            package
        .filter(|pkg_name| device.pinouts.contains_key(*pkg_name))
        })
        .unwrap_or(&device.default_pinout)
}

fn device_packages(device: &DeviceData) -> HashMap<String, Value> {
    device
        .pinouts
        .iter()
        .map(|(name, pinout)| {
            (
                name.clone(),
                serde_json::json!({
                    "pin_count": pinout.pin_count,
                    "source": pinout.source,
                }),
            )
        })
        .collect()
}

fn encode_base64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn file_name_or(path: &Path, fallback: &str) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| fallback.to_string())
}
