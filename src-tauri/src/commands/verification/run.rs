//! Provider-backed verification commands.
//!
//! These commands prepare the selected datasheet/device context, forward the
//! request into the verifier runner, and translate the result back into the
//! JSON contract the frontend already consumes.

use std::collections::HashMap;

use log::info;
use serde_json::Value;
use tauri::AppHandle;

use crate::commands::verification_support;
use crate::parser::{pinout_verifier, verify_compare, verify_overlay};

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
            verification_support::emit_progress(&app2, "verify-progress", update);
        };

        let pdf = verification_support::decode_datasheet_pdf(
            &emit,
            &part_number,
            pdf_base64.as_deref(),
            "Decoding the datasheet PDF",
            "The PDF is prepared locally before pickle trims it to the relevant sections.",
            "verify_pinout",
        )?;
        verification_support::require_datasheet_pdf(
            &pdf,
            datasheet_text.as_deref(),
            "No datasheet input available for verification",
            "Verification requires a datasheet PDF. Text-only datasheet fallback is disabled.",
        )?;
        verification_support::validate_and_cache_datasheet_pdf(&pdf, &part_number, "verify_pinout")?;

        emit(
            pinout_verifier::VerifyProgressUpdate::new(
                "device.load",
                0.3,
                "Loading the selected device and package data",
            )
            .detail("pickle compares the datasheet against the currently loaded package pin map."),
        );
        let context =
            verification_support::load_pinout_device_context(&part_number, package.as_deref())?;

        emit(
            pinout_verifier::VerifyProgressUpdate::new(
                "datasheet.ready",
                0.34,
                format!("Prepared {:.1} MB datasheet for verification", pdf.size_mb),
            )
            .detail("pickle will reduce the family datasheet before it uploads anything to the provider."),
        );
        info!("verify_pinout: calling LLM API...");
        let app_progress = app2.clone();
        let progress = move |update: pinout_verifier::VerifyProgressUpdate| {
            verification_support::emit_progress(&app_progress, "verify-progress", update);
        };
        let result = pinout_verifier::verify_pinout(
            &pdf.bytes,
            datasheet_text.as_deref(),
            &context.device_dict,
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
            verification_support::emit_progress(&app2, "verify-clc-progress", update);
        };

        let pdf = verification_support::decode_datasheet_pdf(
            &emit,
            &part_number,
            pdf_base64.as_deref(),
            "Decoding the datasheet PDF",
            "The PDF is prepared locally before pickle trims it to the CLC chapter.",
            "verify_clc",
        )?;
        verification_support::require_datasheet_pdf(
            &pdf,
            datasheet_text.as_deref(),
            "No datasheet input available for CLC verification",
            "CLC verification requires a datasheet PDF. Text-only datasheet fallback is disabled.",
        )?;
        verification_support::validate_and_cache_datasheet_pdf(&pdf, &part_number, "verify_clc")?;

        emit(
            pinout_verifier::VerifyProgressUpdate::new(
                "device.load",
                0.3,
                "Loading the selected device data",
            )
            .detail("pickle needs the device context so it can store any extracted CLC sources under the right part number."),
        );
        let context =
            verification_support::load_clc_device_context(&part_number, package.as_deref())?;

        if !context.device.has_clc() {
            let result = verify_compare::VerifyResult {
                part_number: context.device.part_number.clone(),
                packages: HashMap::new(),
                notes: vec![
                    "This device has no CLC peripheral, so background CLC verification was skipped."
                        .to_string(),
                ],
                clc_input_sources: None,
                raw_response: String::new(),
            };
            return serde_json::to_value(&result).map_err(|e| format!("Serialize error: {e}"));
        }

        emit(
            pinout_verifier::VerifyProgressUpdate::new(
                "datasheet.ready",
                0.34,
                format!(
                    "Prepared {:.1} MB datasheet for background CLC verification",
                    pdf.size_mb
                ),
            )
            .detail("pickle will now trim the datasheet to the CLC chapter and run a second provider pass in the background."),
        );
        info!("verify_clc: calling LLM API...");
        let app_progress = app2.clone();
        let progress = move |update: pinout_verifier::VerifyProgressUpdate| {
            verification_support::emit_progress(&app_progress, "verify-clc-progress", update);
        };
        let result = pinout_verifier::verify_clc(
            &pdf.bytes,
            datasheet_text.as_deref(),
            &context.device_dict,
            api_key.as_deref(),
            Some(&progress),
        )?;

        if let Some(ref clc_sources) = result.clc_input_sources {
            let _ = verify_overlay::save_clc_sources(&part_number, clc_sources)?;
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
