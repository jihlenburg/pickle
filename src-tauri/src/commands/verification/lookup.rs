//! Datasheet discovery command helpers.
//!
//! `find_datasheet` is intentionally separate from the verification run
//! commands because it mixes local cache probing, Microchip resolution, and
//! frontend-facing progress copy without touching the provider runtime.

use std::fs;

use log::info;
use serde_json::Value;
use tauri::AppHandle;

use crate::commands::verification_support;
use crate::commands::{encode_base64, file_name_or};
use crate::parser::{datasheet_fetcher, dfp_manager, pinout_verifier};

/// Try to find a datasheet: local cache -> ~/Downloads -> auto-resolve from Microchip.
/// Returns `{ path, name, base64, source }` or `null`.
/// Emits `verify-progress` events so the frontend can show status updates.
#[tauri::command]
pub async fn find_datasheet(app: AppHandle, part_number: String) -> Result<Option<Value>, String> {
    let pn = part_number.clone();
    let app2 = app.clone();

    tokio::task::spawn_blocking(move || {
        let emit = |update: pinout_verifier::VerifyProgressUpdate| {
            verification_support::emit_progress(&app2, "verify-progress", update);
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
