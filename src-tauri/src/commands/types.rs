//! Shared IPC request and response shapes for Tauri commands.
//!
//! These types define the serialized contract between the frontend webview and
//! the Rust backend. Keeping them in one place avoids coupling unrelated
//! command handlers just because they share a request payload or response type.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::codegen::generate::ClcModuleConfig;
use crate::settings;

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
pub struct RenameOverlayPackageRequest {
    pub part_number: String,
    pub old_package_name: String,
    pub new_package_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteOverlayPackageRequest {
    pub part_number: String,
    pub package_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetPackageDisplayNameRequest {
    pub part_number: String,
    pub package_name: String,
    #[serde(default)]
    pub display_name: Option<String>,
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
