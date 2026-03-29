//! Tauri command handlers — replace all FastAPI REST endpoints.

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use log::{error, info};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

use crate::codegen::fuses::FuseConfig;
use crate::codegen::generate::{generate_c_files, PinAssignment, PinConfig};
use crate::codegen::oscillator::OscConfig;
use crate::parser::dfp_manager;
use crate::parser::pack_index;
use crate::parser::pinout_verifier;

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
    pub path: Option<String>,
    pub version: Option<String>,
}

#[derive(Serialize)]
pub struct CompileCheckResponse {
    pub success: bool,
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FuseRequest {
    #[serde(default = "default_ics")]
    pub ics: u32,
    #[serde(default = "default_off")]
    pub jtagen: String,
    #[serde(default = "default_off")]
    pub fwdten: String,
    #[serde(default = "default_wdtps")]
    pub wdtps: String,
    #[serde(default = "default_on")]
    pub boren: String,
    #[serde(default = "default_borv")]
    pub borv: String,
}

fn default_ics() -> u32 {
    1
}
fn default_off() -> String {
    "OFF".to_string()
}
fn default_on() -> String {
    "ON".to_string()
}
fn default_wdtps() -> String {
    "PS1024".to_string()
}
fn default_borv() -> String {
    "BOR_HIGH".to_string()
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

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_devices() -> Result<DeviceListResponse, String> {
    info!("list_devices: enumerating devices");
    let cached = dfp_manager::list_cached_devices();
    let all_devs = dfp_manager::list_all_known_devices();
    info!(
        "list_devices: {} total, {} cached",
        all_devs.len(),
        cached.len()
    );
    Ok(DeviceListResponse {
        total: all_devs.len(),
        cached_count: cached.len(),
        devices: all_devs,
        cached,
    })
}

#[tauri::command]
pub fn refresh_index() -> Result<RefreshResponse, String> {
    let index = pack_index::get_pack_index(true)?;
    Ok(RefreshResponse {
        success: true,
        device_count: index.devices.len(),
        pack_count: index.packs.len(),
        age_hours: (index.age_hours() * 10.0).round() / 10.0,
    })
}

#[tauri::command]
pub fn index_status() -> Result<IndexStatusResponse, String> {
    match pack_index::get_pack_index(false) {
        Ok(index) => Ok(IndexStatusResponse {
            available: true,
            device_count: index.devices.len(),
            pack_count: index.packs.len(),
            age_hours: Some((index.age_hours() * 10.0).round() / 10.0),
            is_stale: index.is_stale(),
        }),
        Err(_) => Ok(IndexStatusResponse {
            available: false,
            device_count: 0,
            pack_count: 0,
            age_hours: None,
            is_stale: true,
        }),
    }
}

#[tauri::command]
pub fn load_device(part_number: String, package: Option<String>) -> Result<Value, String> {
    info!("load_device: part={} package={:?}", part_number, package);
    let device = dfp_manager::load_device(&part_number).ok_or_else(|| {
        error!("load_device: device {} not found", part_number);
        format!("Device {} not found", part_number)
    })?;

    let selected_pkg = package
        .as_deref()
        .filter(|p| device.pinouts.contains_key(*p))
        .unwrap_or(&device.default_pinout);

    let resolved_pins = device.resolve_pins(Some(selected_pkg));
    let pinout = device.get_pinout(Some(selected_pkg));

    let packages: HashMap<String, Value> = device
        .pinouts
        .iter()
        .map(|(name, po)| {
            (
                name.clone(),
                serde_json::json!({
                    "pin_count": po.pin_count,
                    "source": po.source,
                }),
            )
        })
        .collect();

    info!(
        "load_device: {} loaded, package={}, {} pins",
        device.part_number, selected_pkg, pinout.pin_count
    );
    Ok(serde_json::json!({
        "part_number": device.part_number,
        "selected_package": selected_pkg,
        "packages": packages,
        "pin_count": pinout.pin_count,
        "pins": resolved_pins,
        "remappable_inputs": device.remappable_inputs,
        "remappable_outputs": device.remappable_outputs,
        "pps_input_mappings": device.pps_input_mappings,
        "pps_output_mappings": device.pps_output_mappings,
        "port_registers": device.port_registers,
    }))
}

#[tauri::command]
pub async fn open_text_file_dialog(
    app: AppHandle,
    request: OpenFileDialogRequest,
) -> Result<Option<OpenTextFileResponse>, String> {
    let mut dialog = app.dialog().file();
    if let Some(title) = request.title {
        dialog = dialog.set_title(title);
    }
    dialog = apply_dialog_filters(dialog, &request.filters);

    let Some(file_path) = dialog.blocking_pick_file() else {
        return Ok(None);
    };

    let path = resolve_dialog_path(file_path)?;
    let contents =
        fs::read_to_string(&path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;

    Ok(Some(OpenTextFileResponse {
        path: path.display().to_string(),
        contents,
    }))
}

#[tauri::command]
pub async fn open_binary_file_dialog(
    app: AppHandle,
    request: OpenFileDialogRequest,
) -> Result<Option<OpenBinaryFileResponse>, String> {
    let mut dialog = app.dialog().file();
    if let Some(title) = request.title {
        dialog = dialog.set_title(title);
    }
    dialog = apply_dialog_filters(dialog, &request.filters);

    let Some(file_path) = dialog.blocking_pick_file() else {
        return Ok(None);
    };

    let path = resolve_dialog_path(file_path)?;
    let bytes = fs::read(&path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "selected-file".to_string());

    Ok(Some(OpenBinaryFileResponse {
        path: path.display().to_string(),
        name,
        base64: base64::engine::general_purpose::STANDARD.encode(bytes),
    }))
}

#[tauri::command]
pub async fn save_text_file_dialog(
    app: AppHandle,
    request: SaveTextFileRequest,
) -> Result<Option<SavedPathResponse>, String> {
    let mut dialog = app.dialog().file().set_file_name(request.suggested_name);
    if let Some(title) = request.title {
        dialog = dialog.set_title(title);
    }
    dialog = apply_dialog_filters(dialog, &request.filters);

    let Some(file_path) = dialog.blocking_save_file() else {
        return Ok(None);
    };

    let path = resolve_dialog_path(file_path)?;
    write_text_file(&path, &request.contents)?;

    Ok(Some(SavedPathResponse {
        path: path.display().to_string(),
    }))
}

#[tauri::command]
pub async fn export_generated_files_dialog(
    app: AppHandle,
    request: ExportFilesRequest,
) -> Result<Option<ExportFilesResponse>, String> {
    if request.files.is_empty() {
        return Err("No files available to export".to_string());
    }

    let mut dialog = app.dialog().file();
    if let Some(title) = request.title {
        dialog = dialog.set_title(title);
    }

    let Some(folder_path) = dialog.blocking_pick_folder() else {
        return Ok(None);
    };

    let directory = resolve_dialog_path(folder_path)?;
    fs::create_dir_all(&directory).map_err(|e| {
        format!(
            "Cannot create export directory {}: {e}",
            directory.display()
        )
    })?;

    let mut written_files = Vec::new();
    for (filename, contents) in request.files {
        let path = directory.join(&filename);
        write_text_file(&path, &contents)?;
        written_files.push(path.display().to_string());
    }
    written_files.sort();

    Ok(Some(ExportFilesResponse {
        directory: directory.display().to_string(),
        written_files,
    }))
}

#[tauri::command]
pub fn generate_code(request: CodegenRequest) -> Result<Value, String> {
    info!(
        "generate_code: part={} assignments={}",
        request.part_number,
        request.assignments.len()
    );
    let device = dfp_manager::load_device(&request.part_number).ok_or_else(|| {
        error!("generate_code: device {} not found", request.part_number);
        format!("Device {} not found", request.part_number)
    })?;

    let pkg_name = request.package.as_deref().unwrap_or(&device.default_pinout);

    let config = PinConfig {
        part_number: request.part_number.clone(),
        assignments: request
            .assignments
            .into_iter()
            .map(|a| PinAssignment {
                pin_position: a.pin_position,
                rp_number: a.rp_number,
                peripheral: a.peripheral,
                direction: a.direction,
                ppsval: a.ppsval,
                fixed: a.fixed,
            })
            .collect(),
        digital_pins: request.digital_pins,
    };

    let sig_names: HashMap<u32, String> = request
        .signal_names
        .into_iter()
        .filter_map(|(k, v)| k.parse::<u32>().ok().map(|k| (k, v)))
        .collect();

    let osc = request
        .oscillator
        .filter(|o| !o.source.is_empty())
        .map(|o| OscConfig {
            source: o.source,
            target_fosc_hz: (o.target_fosc_mhz * 1_000_000.0) as u64,
            crystal_hz: (o.crystal_mhz * 1_000_000.0) as u64,
            poscmd: o.poscmd,
        });

    let fuse = request.fuses.map(|f| FuseConfig {
        ics: f.ics,
        jtagen: f.jtagen,
        fwdten: f.fwdten,
        wdtps: f.wdtps,
        boren: f.boren,
        borv: f.borv,
    });

    let files = generate_c_files(
        &device,
        &config,
        Some(pkg_name),
        Some(&sig_names),
        osc.as_ref(),
        fuse.as_ref(),
    );

    Ok(serde_json::json!({ "files": files }))
}

fn find_xc16_gcc() -> Option<String> {
    // Check PATH
    if let Ok(output) = Command::new("which").arg("xc16-gcc").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
    }
    // Check common install paths
    for candidate in &[
        "/Applications/microchip/xc16/v2.10/bin/xc16-gcc",
        "/opt/microchip/xc16/v2.10/bin/xc16-gcc",
    ] {
        if Path::new(candidate).is_file() {
            return Some(candidate.to_string());
        }
    }
    None
}

fn part_to_mcpu(part_number: &str) -> String {
    let p = part_number.to_uppercase();
    if let Some(rest) = p.strip_prefix("DSPIC") {
        rest.to_string()
    } else if let Some(rest) = p.strip_prefix("PIC") {
        rest.to_string()
    } else {
        p
    }
}

#[tauri::command]
pub fn compiler_info() -> Result<CompilerResponse, String> {
    match find_xc16_gcc() {
        Some(path) => {
            let version = Command::new(&path)
                .arg("--version")
                .output()
                .ok()
                .and_then(|o| {
                    String::from_utf8(o.stdout)
                        .ok()
                        .and_then(|s| s.lines().next().map(|l| l.to_string()))
                })
                .unwrap_or_else(|| "unknown".to_string());
            Ok(CompilerResponse {
                available: true,
                path: Some(path),
                version: Some(version),
            })
        }
        None => Ok(CompilerResponse {
            available: false,
            path: None,
            version: None,
        }),
    }
}

#[tauri::command]
pub fn compile_check(request: CompileCheckRequest) -> Result<CompileCheckResponse, String> {
    let gcc =
        find_xc16_gcc().ok_or_else(|| "XC16 compiler not found on this system".to_string())?;

    let mcpu = part_to_mcpu(&request.part_number);
    let tmpdir = tempfile::tempdir().map_err(|e| format!("Temp dir error: {e}"))?;

    if !request.header.is_empty() {
        let hdr = tmpdir.path().join("pin_config.h");
        std::fs::write(&hdr, &request.header).map_err(|e| format!("Write header error: {e}"))?;
    }

    let src = tmpdir.path().join("pin_config.c");
    std::fs::write(&src, &request.code).map_err(|e| format!("Write source error: {e}"))?;

    let output = Command::new(&gcc)
        .arg(format!("-mcpu={}", mcpu))
        .arg("-c")
        .arg(format!("-I{}", tmpdir.path().display()))
        .arg("-Wall")
        .arg("-Werror")
        .arg("-std=c99")
        .arg("-o")
        .arg(tmpdir.path().join("pin_config.o"))
        .arg(&src)
        .output()
        .map_err(|e| format!("Compiler execution error: {e}"))?;

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if output.status.success() {
        Ok(CompileCheckResponse {
            success: true,
            errors: String::new(),
            warnings: if stderr.is_empty() {
                String::new()
            } else {
                stderr
            },
        })
    } else {
        Ok(CompileCheckResponse {
            success: false,
            errors: if stderr.is_empty() { stdout } else { stderr },
            warnings: String::new(),
        })
    }
}

#[tauri::command]
pub fn verify_pinout(
    pdf_base64: String,
    part_number: String,
    package: Option<String>,
    api_key: Option<String>,
) -> Result<Value, String> {
    use base64::Engine;

    let pdf_bytes = base64::engine::general_purpose::STANDARD
        .decode(&pdf_base64)
        .map_err(|e| format!("Invalid base64: {e}"))?;

    let device = dfp_manager::load_device(&part_number)
        .ok_or_else(|| format!("Device {} not found", part_number))?;

    let pkg_name = package.as_deref().unwrap_or(&device.default_pinout);
    let resolved_pins = device.resolve_pins(Some(pkg_name));
    let pinout = device.get_pinout(Some(pkg_name));

    let device_dict = serde_json::json!({
        "part_number": device.part_number,
        "selected_package": pkg_name,
        "packages": device.pinouts.iter().map(|(name, po)| {
            (name.clone(), serde_json::json!({"pin_count": po.pin_count, "source": po.source}))
        }).collect::<HashMap<String, Value>>(),
        "pin_count": pinout.pin_count,
        "pins": resolved_pins,
    });

    let result = pinout_verifier::verify_pinout(&pdf_bytes, &device_dict, api_key.as_deref())?;

    serde_json::to_value(&result).map_err(|e| format!("Serialize error: {e}"))
}

#[tauri::command]
pub fn apply_overlay(request: ApplyOverlayRequest) -> Result<Value, String> {
    let mut vr = pinout_verifier::VerifyResult {
        part_number: request.part_number.clone(),
        packages: HashMap::new(),
        notes: Vec::new(),
        raw_response: String::new(),
    };

    for (pkg_name, pkg_data) in &request.packages {
        let mut pins: HashMap<u32, String> = HashMap::new();
        if let Some(pin_obj) = pkg_data.get("pins").and_then(|v| v.as_object()) {
            for (pos_str, pad) in pin_obj {
                if let (Ok(pos), Some(pad_str)) = (pos_str.parse::<u32>(), pad.as_str()) {
                    pins.insert(pos, pad_str.to_string());
                }
            }
        }
        vr.packages.insert(
            pkg_name.clone(),
            pinout_verifier::PackageResult {
                package_name: pkg_name.clone(),
                pin_count: pkg_data
                    .get("pin_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(pins.len() as u64) as u32,
                pins,
                pin_functions: HashMap::new(),
                corrections: Vec::new(),
                match_score: 0.0,
            },
        );
    }

    let path = pinout_verifier::save_overlay(&request.part_number, &vr, None)?;
    Ok(serde_json::json!({
        "success": true,
        "path": path.to_string_lossy()
    }))
}

#[tauri::command]
pub fn api_key_status() -> Result<ApiKeyStatusResponse, String> {
    match pinout_verifier::get_api_key() {
        Some(key) => {
            let hint = format!("...{}", &key[key.len().saturating_sub(4)..]);
            Ok(ApiKeyStatusResponse {
                configured: true,
                hint: Some(hint),
            })
        }
        None => Ok(ApiKeyStatusResponse {
            configured: false,
            hint: None,
        }),
    }
}
