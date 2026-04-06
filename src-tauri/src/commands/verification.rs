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
        let emit = |update: pinout_verifier::VerifyProgressUpdate| {
            let _ = app2.emit("verify-progress", update);
        };

        emit(
            pinout_verifier::VerifyProgressUpdate::new(
                "datasheet.search",
                0.06,
                "Checking local datasheet cache",
            )
            .detail("pickle looks in its cache and nearby local files before downloading anything."),
        );
        if let Some(path) = dfp_manager::find_local_datasheet(&pn) {
            let bytes =
                fs::read(&path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
            let name = file_name_or(&path, "datasheet.pdf");
            emit(
                pinout_verifier::VerifyProgressUpdate::new(
                    "datasheet.search",
                    0.16,
                    format!("Found cached datasheet: {name}"),
                )
                .detail("Using the local PDF avoids a download step."),
            );
            return Ok(Some(serde_json::json!({
                "path": path.display().to_string(),
                "name": name,
                "base64": encode_base64(&bytes),
                "source": "local",
            })));
        }

        emit(
            pinout_verifier::VerifyProgressUpdate::new(
                "datasheet.resolve",
                0.1,
                "Resolving the datasheet on Microchip",
            )
            .detail("If no cached PDF is available, pickle looks up the right family datasheet revision."),
        );
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

        let sibling_note = if let Some(ref sibling) = ds_ref.sibling_source {
            format!(
                " (using sibling {} family datasheet — no dedicated datasheet found for {})",
                sibling, pn
            )
        } else {
            String::new()
        };

        emit(
            pinout_verifier::VerifyProgressUpdate::new(
                "datasheet.download",
                0.16,
                format!("Downloading datasheet {}{}", ds_ref.datasheet_revision, sibling_note),
            )
            .detail(if ds_ref.sibling_source.is_some() {
                "No product-specific datasheet was found. Using a sibling device's family datasheet with the same pin-number suffix."
            } else {
                "The PDF is cached locally after download so later runs can skip this step."
            }),
        );
        info!("find_datasheet: downloading PDF from {}", ds_ref.pdf_url);
        match datasheet_fetcher::get_or_download_pdf(&pn, &ds_ref.pdf_url) {
            Ok(bytes) => {
                let name = format!("{}-{}.pdf", pn.to_uppercase(), ds_ref.datasheet_revision);
                emit(
                    pinout_verifier::VerifyProgressUpdate::new(
                        "datasheet.download",
                        0.2,
                        format!("Downloaded {name}{}", sibling_note),
                    )
                    .detail("Verification will use this cached PDF on later runs."),
                );
                return Ok(Some(serde_json::json!({
                    "name": name,
                    "base64": encode_base64(&bytes),
                    "source": "downloaded",
                    "revision": ds_ref.datasheet_revision,
                    "sibling_source": ds_ref.sibling_source,
                    "datasheet_title": ds_ref.datasheet_title,
                })));
            }
            Err(error) => {
                info!(
                    "find_datasheet: PDF download failed, trying text fallback: {}",
                    error
                );
            }
        }

        emit(
            pinout_verifier::VerifyProgressUpdate::new(
                "datasheet.resolve",
                0.18,
                "Trying text extraction fallback",
            )
            .detail("Text-only fallback is kept for lookup diagnostics, but verification still requires the PDF."),
        );
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
    pdf_base64: Option<String>,
    datasheet_text: Option<String>,
    part_number: String,
    package: Option<String>,
    api_key: Option<String>,
) -> Result<Value, String> {
    let app2 = app.clone();

    tokio::task::spawn_blocking(move || {
        let emit = |update: pinout_verifier::VerifyProgressUpdate| {
            let _ = app2.emit("verify-progress", update);
        };

        let pdf_bytes = if let Some(pdf_base64) = pdf_base64.as_deref() {
            emit(
                pinout_verifier::VerifyProgressUpdate::new(
                    "datasheet.decode",
                    0.24,
                    "Decoding the datasheet PDF",
                )
                .detail("The PDF is prepared locally before pickle trims it to the relevant sections."),
            );
            info!("verify_pinout: decoding PDF for {}", part_number);
            base64::engine::general_purpose::STANDARD
                .decode(pdf_base64)
                .map_err(|e| format!("Invalid base64: {e}"))?
        } else {
            Vec::new()
        };

        if pdf_bytes.is_empty() && datasheet_text.as_deref().unwrap_or("").is_empty() {
            return Err("No datasheet input available for verification".to_string());
        }
        if pdf_bytes.is_empty() {
            return Err(
                "Verification requires a datasheet PDF. Text-only datasheet fallback is disabled."
                    .to_string(),
            );
        }

        let size_mb = pdf_bytes.len() as f64 / 1_048_576.0;
        info!("verify_pinout: PDF size = {:.1} MB", size_mb);

        if let Some(reason) = dfp_manager::datasheet_pdf_mismatch_reason(&pdf_bytes, &part_number) {
            return Err(reason);
        }

        if !pdf_bytes.is_empty() {
            dfp_manager::cache_datasheet(&part_number, &pdf_bytes);
        }

        emit(
            pinout_verifier::VerifyProgressUpdate::new(
                "device.load",
                0.3,
                "Loading the selected device and package data",
            )
            .detail("pickle compares the datasheet against the currently loaded package pin map."),
        );
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

        if !pdf_bytes.is_empty() {
            emit(
                pinout_verifier::VerifyProgressUpdate::new(
                    "datasheet.ready",
                    0.34,
                    format!("Prepared {:.1} MB datasheet for verification", size_mb),
                )
                .detail("pickle will reduce the family datasheet before it uploads anything to the provider."),
            );
        } else {
            emit(
                pinout_verifier::VerifyProgressUpdate::new(
                    "datasheet.ready",
                    0.34,
                    "Prepared extracted datasheet text",
                ),
            );
        }
        info!("verify_pinout: calling LLM API...");
        let app_progress = app2.clone();
        let progress = move |update: pinout_verifier::VerifyProgressUpdate| {
            let _ = app_progress.emit("verify-progress", update);
        };
        let result = pinout_verifier::verify_pinout(
            &pdf_bytes,
            datasheet_text.as_deref(),
            &device_dict,
            api_key.as_deref(),
            Some(&progress),
        )?;
        info!(
            "verify_pinout: LLM response received, {} packages found",
            result.packages.len()
        );

        emit(
            pinout_verifier::VerifyProgressUpdate::new(
                "result.done",
                1.0,
                "Verification complete",
            )
            .detail("The extracted package data and any discrepancies are ready to review."),
        );
        serde_json::to_value(&result).map_err(|e| format!("Serialize error: {e}"))
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

#[tauri::command]
pub async fn verify_clc(
    app: AppHandle,
    pdf_base64: Option<String>,
    datasheet_text: Option<String>,
    part_number: String,
    package: Option<String>,
    api_key: Option<String>,
) -> Result<Value, String> {
    let app2 = app.clone();

    tokio::task::spawn_blocking(move || {
        let emit = |update: pinout_verifier::VerifyProgressUpdate| {
            let _ = app2.emit("verify-clc-progress", update);
        };

        let pdf_bytes = if let Some(pdf_base64) = pdf_base64.as_deref() {
            emit(
                pinout_verifier::VerifyProgressUpdate::new(
                    "datasheet.decode",
                    0.24,
                    "Decoding the datasheet PDF",
                )
                .detail("The PDF is prepared locally before pickle trims it to the CLC chapter."),
            );
            info!("verify_clc: decoding PDF for {}", part_number);
            base64::engine::general_purpose::STANDARD
                .decode(pdf_base64)
                .map_err(|e| format!("Invalid base64: {e}"))?
        } else {
            Vec::new()
        };

        if pdf_bytes.is_empty() && datasheet_text.as_deref().unwrap_or("").is_empty() {
            return Err("No datasheet input available for CLC verification".to_string());
        }
        if pdf_bytes.is_empty() {
            return Err(
                "CLC verification requires a datasheet PDF. Text-only datasheet fallback is disabled."
                    .to_string(),
            );
        }

        let size_mb = pdf_bytes.len() as f64 / 1_048_576.0;
        info!("verify_clc: PDF size = {:.1} MB", size_mb);

        if let Some(reason) = dfp_manager::datasheet_pdf_mismatch_reason(&pdf_bytes, &part_number) {
            return Err(reason);
        }

        if !pdf_bytes.is_empty() {
            dfp_manager::cache_datasheet(&part_number, &pdf_bytes);
        }

        emit(
            pinout_verifier::VerifyProgressUpdate::new(
                "device.load",
                0.3,
                "Loading the selected device data",
            )
            .detail("pickle needs the device context so it can store any extracted CLC sources under the right part number."),
        );
        let device = dfp_manager::load_device(&part_number)
            .ok_or_else(|| format!("Device {} not found", part_number))?;

        if device.clc_module_id.is_none() {
            let result = pinout_verifier::VerifyResult {
                part_number: device.part_number.clone(),
                packages: HashMap::new(),
                notes: vec![
                    "This device has no CLC peripheral, so background CLC verification was skipped."
                        .to_string(),
                ],
                clc_input_sources: None,
                raw_response: String::new(),
            };
            return serde_json::to_value(&result)
                .map_err(|e| format!("Serialize error: {e}"));
        }

        let package_name = package.as_deref().unwrap_or(&device.default_pinout);
        let pinout = device.get_pinout(Some(package_name));

        let device_dict = serde_json::json!({
            "part_number": device.part_number,
            "selected_package": package_name,
            "packages": device_packages(&device),
            "pin_count": pinout.pin_count,
            "pins": [],
        });

        emit(
            pinout_verifier::VerifyProgressUpdate::new(
                "datasheet.ready",
                0.34,
                format!("Prepared {:.1} MB datasheet for background CLC verification", size_mb),
            )
            .detail("pickle will now trim the datasheet to the CLC chapter and run a second provider pass in the background."),
        );
        info!("verify_clc: calling LLM API...");
        let app_progress = app2.clone();
        let progress = move |update: pinout_verifier::VerifyProgressUpdate| {
            let _ = app_progress.emit("verify-clc-progress", update);
        };
        let result = pinout_verifier::verify_clc(
            &pdf_bytes,
            datasheet_text.as_deref(),
            &device_dict,
            api_key.as_deref(),
            Some(&progress),
        )?;

        if let Some(ref clc_sources) = result.clc_input_sources {
            let _ = pinout_verifier::save_clc_sources(&part_number, clc_sources)?;
        }

        emit(
            pinout_verifier::VerifyProgressUpdate::new(
                "result.done",
                1.0,
                "Background CLC verification complete",
            )
            .detail("Any extracted CLC source mapping has been cached for this device."),
        );
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
