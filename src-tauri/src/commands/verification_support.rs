//! Shared helpers for datasheet verification commands.
//!
//! The Tauri pinout and CLC commands both need the same setup steps: decode and
//! validate the selected PDF, cache it locally, load the selected device, and
//! build the JSON shape expected by the verifier runner. Centralizing those
//! steps keeps the command handlers thin and aligned.

use base64::Engine;
use log::info;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Runtime};

use crate::commands::{device_packages, selected_package};
use crate::parser::edc_parser::DeviceData;
use crate::parser::{dfp_manager, pinout_verifier};

/// Prepared datasheet bytes plus a precomputed size used for progress text.
pub(crate) struct PreparedDatasheetPdf {
    pub bytes: Vec<u8>,
    pub size_mb: f64,
}

/// Loaded device/package context ready for a verifier provider call.
pub(crate) struct VerificationDeviceContext {
    pub device: DeviceData,
    pub device_dict: Value,
}

pub(crate) fn emit_progress<R: Runtime>(
    app: &AppHandle<R>,
    event: &str,
    update: pinout_verifier::VerifyProgressUpdate,
) {
    let _ = app.emit(event, update);
}

pub(crate) fn decode_datasheet_pdf(
    emit: &impl Fn(pinout_verifier::VerifyProgressUpdate),
    part_number: &str,
    pdf_base64: Option<&str>,
    decode_label: &str,
    decode_detail: &str,
    log_prefix: &str,
) -> Result<PreparedDatasheetPdf, String> {
    let bytes = if let Some(pdf_base64) = pdf_base64 {
        emit(
            pinout_verifier::VerifyProgressUpdate::new("datasheet.decode", 0.24, decode_label)
                .detail(decode_detail),
        );
        info!("{log_prefix}: decoding PDF for {}", part_number);
        base64::engine::general_purpose::STANDARD
            .decode(pdf_base64)
            .map_err(|error| format!("Invalid base64: {error}"))?
    } else {
        Vec::new()
    };

    Ok(PreparedDatasheetPdf {
        size_mb: bytes.len() as f64 / 1_048_576.0,
        bytes,
    })
}

pub(crate) fn require_datasheet_pdf(
    pdf: &PreparedDatasheetPdf,
    datasheet_text: Option<&str>,
    empty_error: &str,
    pdf_required_error: &str,
) -> Result<(), String> {
    if pdf.bytes.is_empty() && datasheet_text.unwrap_or_default().is_empty() {
        return Err(empty_error.to_string());
    }
    if pdf.bytes.is_empty() {
        return Err(pdf_required_error.to_string());
    }
    Ok(())
}

pub(crate) fn validate_and_cache_datasheet_pdf(
    pdf: &PreparedDatasheetPdf,
    part_number: &str,
    log_prefix: &str,
) -> Result<(), String> {
    if pdf.bytes.is_empty() {
        return Ok(());
    }

    info!("{log_prefix}: PDF size = {:.1} MB", pdf.size_mb);

    if let Some(reason) = dfp_manager::datasheet_pdf_mismatch_reason(&pdf.bytes, part_number) {
        return Err(reason);
    }

    dfp_manager::cache_datasheet(part_number, &pdf.bytes);
    Ok(())
}

fn load_device(part_number: &str) -> Result<DeviceData, String> {
    dfp_manager::load_device(part_number).ok_or_else(|| format!("Device {} not found", part_number))
}

pub(crate) fn load_pinout_device_context(
    part_number: &str,
    package: Option<&str>,
) -> Result<VerificationDeviceContext, String> {
    let device = load_device(part_number)?;
    let package_name = selected_package(&device, package).to_string();
    let resolved_pins = device.resolve_pins(Some(&package_name));
    let pinout = device.get_pinout(Some(&package_name));
    let device_dict = serde_json::json!({
        "part_number": device.part_number,
        "selected_package": package_name,
        "packages": device_packages(&device),
        "pin_count": pinout.pin_count,
        "pins": resolved_pins,
    });

    Ok(VerificationDeviceContext {
        device,
        device_dict,
    })
}

pub(crate) fn load_clc_device_context(
    part_number: &str,
    package: Option<&str>,
) -> Result<VerificationDeviceContext, String> {
    let device = load_device(part_number)?;
    let package_name = selected_package(&device, package).to_string();
    let pinout = device.get_pinout(Some(&package_name));
    let device_dict = serde_json::json!({
        "part_number": device.part_number,
        "selected_package": package_name,
        "packages": device_packages(&device),
        "pin_count": pinout.pin_count,
        "pins": [],
    });

    Ok(VerificationDeviceContext {
        device,
        device_dict,
    })
}
