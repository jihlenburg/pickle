//! Overlay mutation and verification-status commands.
//!
//! These commands manage user-authored verification output after the provider
//! pass is complete: saving overlays, renaming/deleting overlay packages,
//! storing display-name overrides, and reporting provider-key status.

use std::collections::HashMap;

use serde_json::Value;

use crate::commands::{
    ApiKeyStatusResponse, ApplyOverlayRequest, DeleteOverlayPackageRequest,
    RenameOverlayPackageRequest, SetPackageDisplayNameRequest,
};
use crate::parser::{dfp_manager, pinout_verifier, verify_compare, verify_overlay};

#[tauri::command]
pub fn apply_overlay(request: ApplyOverlayRequest) -> Result<Value, String> {
    let device = dfp_manager::load_device(&request.part_number)
        .ok_or_else(|| format!("Device {} not found", request.part_number))?;
    let mut verify_result = verify_compare::VerifyResult {
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
            verify_compare::PackageResult {
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

    let saved =
        verify_overlay::save_overlay(&request.part_number, &verify_result, None, Some(&device))?;
    Ok(serde_json::json!({
        "success": true,
        "path": saved.path.to_string_lossy(),
        "packageName": saved.package_names.into_iter().next(),
    }))
}

#[tauri::command]
pub fn rename_overlay_package(request: RenameOverlayPackageRequest) -> Result<Value, String> {
    let part_number = request.part_number.trim().to_uppercase();
    let old_package_name = request.old_package_name.trim();
    let new_package_name = request.new_package_name.trim();

    if old_package_name.is_empty() || new_package_name.is_empty() {
        return Err("Package names cannot be empty".to_string());
    }

    let device = dfp_manager::load_device(&part_number)
        .ok_or_else(|| format!("Device {} not found", part_number))?;

    if device.pinouts.keys().any(|name| {
        name.eq_ignore_ascii_case(new_package_name) && !name.eq_ignore_ascii_case(old_package_name)
    }) {
        return Err(format!(
            "Package {} already exists for {}",
            new_package_name, part_number
        ));
    }

    let path =
        verify_overlay::rename_overlay_package(&part_number, old_package_name, new_package_name)?;
    Ok(serde_json::json!({
        "success": true,
        "path": path.to_string_lossy(),
        "packageName": new_package_name
    }))
}

#[tauri::command]
pub fn delete_overlay_package(request: DeleteOverlayPackageRequest) -> Result<Value, String> {
    let part_number = request.part_number.trim().to_uppercase();
    let package_name = request.package_name.trim();

    if package_name.is_empty() {
        return Err("Package name cannot be empty".to_string());
    }

    let path = verify_overlay::delete_overlay_package(&part_number, package_name)?;
    Ok(serde_json::json!({
        "success": true,
        "path": path.map(|value| value.to_string_lossy().to_string()),
        "packageName": package_name
    }))
}

#[tauri::command]
pub fn set_package_display_name(request: SetPackageDisplayNameRequest) -> Result<Value, String> {
    let part_number = request.part_number.trim().to_uppercase();
    let package_name = request.package_name.trim();
    let display_name = request
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    if package_name.is_empty() {
        return Err("Package name cannot be empty".to_string());
    }

    let device = dfp_manager::load_device(&part_number)
        .ok_or_else(|| format!("Device {} not found", part_number))?;
    let canonical_package_name = device
        .pinouts
        .keys()
        .find(|name| name.eq_ignore_ascii_case(package_name))
        .cloned()
        .ok_or_else(|| format!("Package {} was not found for {}", package_name, part_number))?;

    let path = verify_overlay::set_package_display_name(
        &part_number,
        &canonical_package_name,
        display_name.as_deref(),
    )?;

    Ok(serde_json::json!({
        "success": true,
        "path": path.map(|value| value.to_string_lossy().to_string()),
        "packageName": canonical_package_name,
        "displayName": display_name,
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
