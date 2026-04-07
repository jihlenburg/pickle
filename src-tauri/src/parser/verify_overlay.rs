//! Verification overlay persistence helpers.
//!
//! Verified package tables and display-name overrides are stored separately
//! from provider orchestration so the verifier can stay focused on extraction
//! while this module owns JSON merge, rename, delete, cleanup, and canonical
//! package-collapse policy for identical pin maps.

use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::parser::dfp_manager::{clc_sources_dir, pinouts_dir};
use crate::parser::dfp_store::equivalent_existing_pinout_key;
use crate::parser::edc_parser::DeviceData;
use crate::parser::verify_compare::VerifyResult;

pub struct SaveOverlayOutcome {
    pub path: PathBuf,
    pub package_names: Vec<String>,
}

pub fn save_overlay(
    part_number: &str,
    verify_result: &VerifyResult,
    selected_packages: Option<&[String]>,
    existing_device: Option<&DeviceData>,
) -> Result<SaveOverlayOutcome, String> {
    let dir = pinouts_dir();
    fs::create_dir_all(&dir).map_err(|error| format!("Cannot create pinouts dir: {error}"))?;
    let overlay_path = dir.join(format!("{}.json", part_number.to_uppercase()));

    let mut existing: Value = if overlay_path.exists() {
        let text = fs::read_to_string(&overlay_path).unwrap_or_else(|_| "{}".to_string());
        serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if existing.get("packages").is_none() {
        existing["packages"] = serde_json::json!({});
    }

    let overlay_data = verify_result.to_overlay_json();
    let mut package_names = Vec::new();
    if let Some(new_packages) = overlay_data
        .get("packages")
        .and_then(|value| value.as_object())
    {
        for (pkg_name, pkg_data) in new_packages {
            if let Some(selected) = selected_packages {
                if !selected.contains(pkg_name) {
                    continue;
                }
            }
            let canonical_name = merge_verified_package_into_overlay_json(
                &mut existing,
                existing_device,
                pkg_name,
                pkg_data,
            )?;
            package_names.push(canonical_name);
        }
    }

    let json = serde_json::to_string_pretty(&existing)
        .map_err(|error| format!("JSON serialize error: {error}"))?;
    fs::write(&overlay_path, json).map_err(|error| format!("Write error: {error}"))?;

    if let Some(ref clc_sources) = verify_result.clc_input_sources {
        let _ = save_clc_sources(part_number, clc_sources);
    }

    Ok(SaveOverlayOutcome {
        path: overlay_path,
        package_names,
    })
}

fn overlay_path_for_part(part_number: &str) -> PathBuf {
    pinouts_dir().join(format!("{}.json", part_number.to_uppercase()))
}

fn load_overlay_json(path: &PathBuf) -> Result<Value, String> {
    if !path.exists() {
        return Err(format!("Overlay file not found: {}", path.display()));
    }

    let text = fs::read_to_string(path).map_err(|error| format!("Read error: {error}"))?;
    serde_json::from_str(&text).map_err(|error| format!("JSON parse error: {error}"))
}

fn load_overlay_json_or_default(path: &PathBuf) -> Result<Value, String> {
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    load_overlay_json(path)
}

fn matching_overlay_package_key(
    packages: &serde_json::Map<String, Value>,
    package_name: &str,
) -> Option<String> {
    packages
        .keys()
        .find(|name| name.eq_ignore_ascii_case(package_name))
        .cloned()
}

fn ensure_overlay_object_map<'a>(
    overlay: &'a mut Value,
    field_name: &str,
) -> Result<&'a mut serde_json::Map<String, Value>, String> {
    if !overlay.is_object() {
        *overlay = serde_json::json!({});
    }

    let Some(root) = overlay.as_object_mut() else {
        return Err("Overlay root must be a JSON object".to_string());
    };
    if !root.get(field_name).is_some_and(Value::is_object) {
        root.insert(
            field_name.to_string(),
            Value::Object(serde_json::Map::new()),
        );
    }
    root.get_mut(field_name)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("Overlay field {field_name} is not a JSON object"))
}

fn move_display_name_override_in_json(
    overlay: &mut Value,
    old_package_name: &str,
    new_package_name: &str,
) {
    let Some(display_names) = overlay
        .get_mut("display_names")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let Some(existing_key) = matching_overlay_package_key(display_names, old_package_name) else {
        return;
    };
    let Some(display_name) = display_names.remove(&existing_key) else {
        return;
    };
    display_names.insert(new_package_name.to_string(), display_name);
}

fn clear_display_name_override_in_json(overlay: &mut Value, package_name: &str) {
    let Some(display_names) = overlay
        .get_mut("display_names")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    if let Some(existing_key) = matching_overlay_package_key(display_names, package_name) {
        display_names.remove(&existing_key);
    }
}

fn package_data_pin_map(package_data: &Value) -> HashMap<u32, String> {
    package_data
        .get("pins")
        .and_then(Value::as_object)
        .map(|pins| {
            pins.iter()
                .filter_map(|(position, pad_name)| {
                    position
                        .parse::<u32>()
                        .ok()
                        .zip(pad_name.as_str().map(ToOwned::to_owned))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn merge_verified_package_into_overlay_json(
    overlay: &mut Value,
    existing_device: Option<&DeviceData>,
    package_name: &str,
    package_data: &Value,
) -> Result<String, String> {
    let pin_count = package_data
        .get("pin_count")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| package_data_pin_map(package_data).len() as u64)
        as u32;
    let pin_map = package_data_pin_map(package_data);

    if let Some(device) = existing_device {
        if let Some(existing_key) = equivalent_existing_pinout_key(device, pin_count, &pin_map) {
            if !existing_key.eq_ignore_ascii_case(package_name) {
                if let Some(packages) = overlay.get_mut("packages").and_then(Value::as_object_mut) {
                    if let Some(alias_key) = matching_overlay_package_key(packages, package_name) {
                        packages.remove(&alias_key);
                    }
                }
                clear_display_name_override_in_json(overlay, package_name);
                let _ =
                    set_package_display_name_in_json(overlay, &existing_key, Some(package_name))?;
                return Ok(existing_key);
            }
        }
    }

    let packages = ensure_overlay_object_map(overlay, "packages")?;
    if let Some(existing_key) = matching_overlay_package_key(packages, package_name) {
        packages.remove(&existing_key);
    }
    packages.insert(package_name.to_string(), package_data.clone());
    Ok(package_name.to_string())
}

fn overlay_effectively_empty(overlay: &Value) -> bool {
    let packages_empty = overlay
        .get("packages")
        .and_then(Value::as_object)
        .map(|packages| packages.is_empty())
        .unwrap_or(true);
    let display_names_empty = overlay
        .get("display_names")
        .and_then(Value::as_object)
        .map(|display_names| display_names.is_empty())
        .unwrap_or(true);
    packages_empty && display_names_empty
}

fn prune_empty_overlay_sections(overlay: &mut Value) {
    let Some(root) = overlay.as_object_mut() else {
        return;
    };
    for field_name in ["packages", "display_names"] {
        if root
            .get(field_name)
            .and_then(Value::as_object)
            .is_some_and(|entries| entries.is_empty())
        {
            root.remove(field_name);
        }
    }
}

fn write_overlay_json(path: &PathBuf, overlay: &mut Value) -> Result<Option<PathBuf>, String> {
    prune_empty_overlay_sections(overlay);
    if overlay_effectively_empty(overlay) {
        if path.exists() {
            fs::remove_file(path).map_err(|error| format!("Delete error: {error}"))?;
        }
        return Ok(None);
    }

    let json = serde_json::to_string_pretty(overlay)
        .map_err(|error| format!("JSON serialize error: {error}"))?;
    fs::write(path, json).map_err(|error| format!("Write error: {error}"))?;
    Ok(Some(path.clone()))
}

pub(crate) fn rename_overlay_package_in_json(
    overlay: &mut Value,
    old_package_name: &str,
    new_package_name: &str,
) -> Result<(), String> {
    let Some(packages) = overlay.get_mut("packages").and_then(Value::as_object_mut) else {
        return Err("Overlay file does not contain any package entries".to_string());
    };

    let Some(existing_key) = matching_overlay_package_key(packages, old_package_name) else {
        return Err(format!(
            "Overlay package {} was not found",
            old_package_name
        ));
    };

    if existing_key == new_package_name {
        return Ok(());
    }

    if let Some(conflict_key) = matching_overlay_package_key(packages, new_package_name) {
        if !conflict_key.eq_ignore_ascii_case(&existing_key) {
            return Err(format!(
                "Overlay package {} already exists",
                new_package_name
            ));
        }
    }

    let package_value = packages
        .remove(&existing_key)
        .ok_or_else(|| format!("Overlay package {} could not be removed", existing_key))?;
    packages.insert(new_package_name.to_string(), package_value);
    move_display_name_override_in_json(overlay, &existing_key, new_package_name);
    Ok(())
}

pub(crate) fn delete_overlay_package_from_json(
    overlay: &mut Value,
    package_name: &str,
) -> Result<bool, String> {
    let Some(packages) = overlay.get_mut("packages").and_then(Value::as_object_mut) else {
        return Err("Overlay file does not contain any package entries".to_string());
    };

    let Some(existing_key) = matching_overlay_package_key(packages, package_name) else {
        return Err(format!("Overlay package {} was not found", package_name));
    };

    packages.remove(&existing_key);
    clear_display_name_override_in_json(overlay, &existing_key);
    Ok(overlay_effectively_empty(overlay))
}

pub(crate) fn set_package_display_name_in_json(
    overlay: &mut Value,
    package_name: &str,
    display_name: Option<&str>,
) -> Result<bool, String> {
    match display_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(display_name) => {
            let display_names = ensure_overlay_object_map(overlay, "display_names")?;
            if let Some(existing_key) = matching_overlay_package_key(display_names, package_name) {
                display_names.remove(&existing_key);
            }
            display_names.insert(
                package_name.to_string(),
                Value::String(display_name.to_string()),
            );
        }
        None => {
            clear_display_name_override_in_json(overlay, package_name);
        }
    }

    Ok(overlay_effectively_empty(overlay))
}

pub fn rename_overlay_package(
    part_number: &str,
    old_package_name: &str,
    new_package_name: &str,
) -> Result<PathBuf, String> {
    let overlay_path = overlay_path_for_part(part_number);
    let mut overlay = load_overlay_json(&overlay_path)?;
    rename_overlay_package_in_json(&mut overlay, old_package_name, new_package_name)?;

    let _ = write_overlay_json(&overlay_path, &mut overlay)?;
    Ok(overlay_path)
}

pub fn delete_overlay_package(
    part_number: &str,
    package_name: &str,
) -> Result<Option<PathBuf>, String> {
    let overlay_path = overlay_path_for_part(part_number);
    let mut overlay = load_overlay_json(&overlay_path)?;
    let _ = delete_overlay_package_from_json(&mut overlay, package_name)?;

    write_overlay_json(&overlay_path, &mut overlay)
}

pub fn set_package_display_name(
    part_number: &str,
    package_name: &str,
    display_name: Option<&str>,
) -> Result<Option<PathBuf>, String> {
    let overlay_path = overlay_path_for_part(part_number);
    let mut overlay = load_overlay_json_or_default(&overlay_path)?;
    let _ = set_package_display_name_in_json(&mut overlay, package_name, display_name)?;
    write_overlay_json(&overlay_path, &mut overlay)
}

pub fn save_clc_sources(part_number: &str, clc_sources: &[Vec<String>]) -> Result<PathBuf, String> {
    let clc_dir = clc_sources_dir();
    fs::create_dir_all(&clc_dir)
        .map_err(|error| format!("Cannot create clc_sources dir: {error}"))?;
    let clc_path = clc_dir.join(format!("{}.json", part_number.to_uppercase()));
    let clc_json = serde_json::to_string_pretty(clc_sources)
        .map_err(|error| format!("CLC JSON serialize error: {error}"))?;
    fs::write(&clc_path, clc_json).map_err(|error| format!("CLC write error: {error}"))?;
    Ok(clc_path)
}

#[cfg(test)]
mod tests {
    use super::{merge_verified_package_into_overlay_json, set_package_display_name_in_json};
    use crate::parser::edc_parser::{DeviceData, DeviceInfo, Pinout};
    use serde_json::json;
    use std::collections::HashMap;

    fn pinout(package: &str, source: &str, pins: &[(u32, &str)]) -> Pinout {
        Pinout {
            package: package.to_string(),
            display_name: None,
            pin_count: pins.len() as u32,
            source: source.to_string(),
            pins: pins
                .iter()
                .map(|(position, pad_name)| (*position, (*pad_name).to_string()))
                .collect(),
        }
    }

    fn test_device(pinouts: Vec<(&str, Pinout)>) -> DeviceData {
        let mut pinout_map = HashMap::new();
        for (name, pinout) in pinouts {
            pinout_map.insert(name.to_string(), pinout);
        }

        DeviceData {
            part_number: "DSPIC33AK256MPS205".to_string(),
            pads: HashMap::new(),
            pinouts: pinout_map,
            default_pinout: "STX32 (48-pin VQFN)".to_string(),
            remappable_inputs: Vec::new(),
            remappable_outputs: Vec::new(),
            pps_input_mappings: Vec::new(),
            pps_output_mappings: Vec::new(),
            port_registers: HashMap::new(),
            ansel_bits: HashMap::new(),
            fuse_defs: Vec::new(),
            clc_module_id: None,
            clc_input_sources: None,
            device_info: DeviceInfo::default(),
        }
    }

    #[test]
    fn merge_verified_package_canonicalizes_identical_builtin_pinouts() {
        let device = test_device(vec![(
            "STX32 (48-pin VQFN)",
            pinout(
                "STX32 (48-pin VQFN)",
                "edc",
                &[(1, "RA0"), (2, "VDD"), (3, "VSS"), (4, "RB1")],
            ),
        )]);
        let mut overlay = json!({
            "packages": {},
        });
        let package_data = json!({
            "source": "overlay",
            "pin_count": 4,
            "pins": {
                "1": "RA0",
                "2": "VDD_2",
                "3": "VSS_2",
                "4": "RB1"
            }
        });

        let canonical = merge_verified_package_into_overlay_json(
            &mut overlay,
            Some(&device),
            "48-PIN VQFN/TQFP (dsPIC33AKXXXMPS505/dsPIC33AKXXXMPS205)",
            &package_data,
        )
        .expect("identical built-in package should collapse to the device-pack key");

        assert_eq!(canonical, "STX32 (48-pin VQFN)");
        assert!(overlay["packages"]
            .as_object()
            .expect("packages object")
            .is_empty());
        assert_eq!(
            overlay["display_names"]["STX32 (48-pin VQFN)"],
            "48-PIN VQFN/TQFP (dsPIC33AKXXXMPS505/dsPIC33AKXXXMPS205)"
        );
    }

    #[test]
    fn merge_verified_package_keeps_distinct_overlay_pinouts() {
        let device = test_device(vec![(
            "STX32 (48-pin VQFN)",
            pinout(
                "STX32 (48-pin VQFN)",
                "edc",
                &[(1, "RA0"), (2, "VDD"), (3, "VSS"), (4, "RB1")],
            ),
        )]);
        let mut overlay = json!({});
        let package_data = json!({
            "source": "overlay",
            "pin_count": 4,
            "pins": {
                "1": "RA0",
                "2": "VDD",
                "3": "VSS",
                "4": "RB2"
            }
        });

        let canonical = merge_verified_package_into_overlay_json(
            &mut overlay,
            Some(&device),
            "48-PIN TQFP",
            &package_data,
        )
        .expect("distinct package should stay as its own overlay entry");

        assert_eq!(canonical, "48-PIN TQFP");
        assert_eq!(overlay["packages"]["48-PIN TQFP"]["pins"]["4"], "RB2");
        assert!(overlay.get("display_names").is_none());
    }

    #[test]
    fn explicit_display_names_can_still_be_written_after_collapse() {
        let mut overlay = json!({});

        let should_delete = set_package_display_name_in_json(
            &mut overlay,
            "STX32 (48-pin VQFN)",
            Some("48-PIN VQFN/TQFP"),
        )
        .expect("display-name overrides should remain writable for canonicalized packages");

        assert!(!should_delete);
        assert_eq!(
            overlay["display_names"]["STX32 (48-pin VQFN)"],
            "48-PIN VQFN/TQFP"
        );
    }
}
