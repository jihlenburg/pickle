//! Datasheet verification, provider status, and overlay persistence commands.

use std::collections::HashMap;
use std::fs;

use base64::Engine;
use log::info;
use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::parser::{datasheet_fetcher, dfp_manager, pinout_verifier};

use super::{
    device_packages, encode_base64, file_name_or, ApiKeyStatusResponse, ApplyOverlayRequest,
};

/// Try to find a datasheet: local cache -> ~/Downloads -> auto-resolve from Microchip.
/// Returns `{ path, name, base64, source }` or `null`.
/// Emits `verify-progress` events so the frontend can show status updates.
#[tauri::command]
pub async fn find_datasheet(app: AppHandle, part_number: String) -> Result<Option<Value>, String> {
    let pn = part_number.clone();
    let app2 = app.clone();

    tokio::task::spawn_blocking(move || {
        let emit = |msg: &str| {
            let _ = app2.emit("verify-progress", msg);
        };

        emit("Checking local files...");
        if let Some(path) = dfp_manager::find_local_datasheet(&pn) {
            let bytes =
                fs::read(&path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
            let name = file_name_or(&path, "datasheet.pdf");
            emit(&format!("Found local: {name}"));
            return Ok(Some(serde_json::json!({
                "path": path.display().to_string(),
                "name": name,
                "base64": encode_base64(&bytes),
                "source": "local",
            })));
        }

        emit("Resolving datasheet from Microchip...");
        info!(
            "find_datasheet: resolving {} from Microchip product page...",
            pn
        );
        let ds_ref = match datasheet_fetcher::resolve(&pn) {
            Ok(reference) => reference,
            Err(error) => {
                info!("find_datasheet: resolve failed for {}: {}", pn, error);
                return Ok(None);
            }
        };

        emit(&format!("Downloading {}...", ds_ref.datasheet_revision));
        info!("find_datasheet: downloading PDF from {}", ds_ref.pdf_url);
        match datasheet_fetcher::get_or_download_pdf(&pn, &ds_ref.pdf_url) {
            Ok(bytes) => {
                let name = format!("{}-{}.pdf", pn.to_uppercase(), ds_ref.datasheet_revision);
                emit(&format!("Downloaded {name}"));
                return Ok(Some(serde_json::json!({
                    "name": name,
                    "base64": encode_base64(&bytes),
                    "source": "downloaded",
                    "revision": ds_ref.datasheet_revision,
                })));
            }
            Err(error) => {
                info!(
                    "find_datasheet: PDF download failed, trying text fallback: {}",
                    error
                );
            }
        }

        emit("Trying text extraction fallback...");
        match datasheet_fetcher::get_or_fetch_text(&pn, &ds_ref.pdf_url) {
            Ok(text) => {
                let name = format!("{}-{}.md", pn.to_uppercase(), ds_ref.datasheet_revision);
                Ok(Some(serde_json::json!({
                    "name": name,
                    "text": text,
                    "source": "text_proxy",
                    "revision": ds_ref.datasheet_revision,
                    "pdf_url": ds_ref.pdf_url,
                })))
            }
            Err(error) => {
                info!("find_datasheet: text fallback also failed: {}", error);
                Ok(None)
            }
        }
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

#[tauri::command]
pub async fn verify_pinout(
    app: AppHandle,
    pdf_base64: String,
    part_number: String,
    package: Option<String>,
    api_key: Option<String>,
) -> Result<Value, String> {
    let app2 = app.clone();

    tokio::task::spawn_blocking(move || {
        let emit = |msg: &str| {
            let _ = app2.emit("verify-progress", msg);
        };

        emit("Decoding PDF...");
        info!("verify_pinout: decoding PDF for {}", part_number);
        let pdf_bytes = base64::engine::general_purpose::STANDARD
            .decode(&pdf_base64)
            .map_err(|e| format!("Invalid base64: {e}"))?;
        let size_mb = pdf_bytes.len() as f64 / 1_048_576.0;
        info!("verify_pinout: PDF size = {:.1} MB", size_mb);

        dfp_manager::cache_datasheet(&part_number, &pdf_bytes);

        emit("Loading device data...");
        let device = dfp_manager::load_device(&part_number)
            .ok_or_else(|| format!("Device {} not found", part_number))?;

        let package_name = package.as_deref().unwrap_or(&device.default_pinout);
        let resolved_pins = device.resolve_pins(Some(package_name));
        let pinout = device.get_pinout(Some(package_name));

        let device_dict = serde_json::json!({
            "part_number": device.part_number,
            "selected_package": package_name,
            "packages": device_packages(&device),
            "pin_count": pinout.pin_count,
            "pins": resolved_pins,
        });

        emit(&format!(
            "Sending {:.1} MB PDF to LLM — this takes 30–90s...",
            size_mb
        ));
        info!("verify_pinout: calling LLM API...");
        let result = pinout_verifier::verify_pinout(&pdf_bytes, &device_dict, api_key.as_deref())?;
        info!(
            "verify_pinout: LLM response received, {} packages found",
            result.packages.len()
        );

        emit("Processing results...");
        serde_json::to_value(&result).map_err(|e| format!("Serialize error: {e}"))
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

#[tauri::command]
pub fn apply_overlay(request: ApplyOverlayRequest) -> Result<Value, String> {
    let mut verify_result = pinout_verifier::VerifyResult {
        part_number: request.part_number.clone(),
        packages: HashMap::new(),
        notes: Vec::new(),
        clc_input_sources: None,
        raw_response: String::new(),
    };

    for (pkg_name, pkg_data) in &request.packages {
        let mut pins: HashMap<u32, String> = HashMap::new();
        if let Some(pin_obj) = pkg_data.get("pins").and_then(|value| value.as_object()) {
            for (position, pad) in pin_obj {
                if let (Ok(position), Some(pad_name)) = (position.parse::<u32>(), pad.as_str()) {
                    pins.insert(position, pad_name.to_string());
                }
            }
        }
        verify_result.packages.insert(
            pkg_name.clone(),
            pinout_verifier::PackageResult {
                package_name: pkg_name.clone(),
                pin_count: pkg_data
                    .get("pin_count")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(pins.len() as u64) as u32,
                pins,
                pin_functions: HashMap::new(),
                corrections: Vec::new(),
                match_score: 0.0,
            },
        );
    }

    let path = pinout_verifier::save_overlay(&request.part_number, &verify_result, None)?;
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
