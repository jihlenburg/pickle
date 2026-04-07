//! Local device-cache, overlay, and CLC-source store helpers.
//!
//! The DFP manager delegates JSON-backed package overlays, cached parsed
//! devices, and per-device CLC source overrides to this module so the main pack
//! loader can focus on locating and extracting pack data.

use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::parser::dfp_paths::{clc_sources_dir, devices_dir, read_roots};
use crate::parser::edc_parser::{DeviceData, Pad, Pinout};

fn is_synthetic_package_name(name: &str) -> bool {
    name.trim().eq_ignore_ascii_case("default")
}

pub(crate) fn replace_redundant_default_pinout(device: &mut DeviceData) {
    let default_name = device.default_pinout.clone();
    if !is_synthetic_package_name(&default_name) {
        return;
    }

    let Some(default_pinout) = device.pinouts.get(&default_name).cloned() else {
        return;
    };

    let mut replacement_names: Vec<String> = device
        .pinouts
        .iter()
        .filter_map(|(name, pinout)| {
            if name.eq_ignore_ascii_case(&default_name) || is_synthetic_package_name(name) {
                return None;
            }
            if pinout.pin_count != default_pinout.pin_count {
                return None;
            }
            if pinout.pins != default_pinout.pins {
                return None;
            }
            Some(name.clone())
        })
        .collect();
    replacement_names.sort();

    if let Some(replacement_name) = replacement_names.into_iter().next() {
        device.pinouts.remove(&default_name);
        device.default_pinout = replacement_name;
    }
}

/// Extract the dsPIC33 performance-family prefix (e.g. "CDV", "CDVL", "CK")
/// from a part number string. Returns `None` for non-dsPIC33 parts.
fn dspic33_family(part: &str) -> Option<&str> {
    let upper = part
        .strip_prefix("DSPIC33")
        .or_else(|| part.strip_prefix("dsPIC33"))?;
    for family in &["CDVL", "CDV", "EDV", "AK", "CH", "CK", "EV", "EP"] {
        if upper.starts_with(family) {
            return Some(&upper[..family.len()]);
        }
    }
    None
}

fn package_display_name_overrides(overlay: &serde_json::Value) -> HashMap<String, String> {
    overlay
        .get("display_names")
        .and_then(|value| value.as_object())
        .map(|display_names| {
            display_names
                .iter()
                .filter_map(|(package_name, display_name)| {
                    display_name
                        .as_str()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(|value| (package_name.clone(), value.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_pinout_pad_name(pad_name: &str) -> String {
    let upper = pad_name.trim().to_uppercase();
    Regex::new(r"_\d+$")
        .unwrap()
        .replace(&upper, "")
        .to_string()
}

fn pin_maps_equivalent(left: &HashMap<u32, String>, right: &HashMap<u32, String>) -> bool {
    if left.len() != right.len() {
        return false;
    }

    left.iter().all(|(position, left_pad)| {
        right
            .get(position)
            .map(|right_pad| {
                normalize_pinout_pad_name(left_pad) == normalize_pinout_pad_name(right_pad)
            })
            .unwrap_or(false)
    })
}

/// Find an existing package key whose pin map is effectively identical to the
/// verified overlay package after rail aliases such as `VDD_2` are normalized.
/// Built-in EDC package variants win over overlay-backed ones so datasheet
/// aliases collapse onto the stable device-pack key whenever possible.
pub(crate) fn equivalent_existing_pinout_key(
    device: &DeviceData,
    pin_count: u32,
    pins: &HashMap<u32, String>,
) -> Option<String> {
    let mut candidates: Vec<(u8, u8, String)> = device
        .pinouts
        .iter()
        .filter_map(|(name, pinout)| {
            if pinout.pin_count != pin_count || !pin_maps_equivalent(&pinout.pins, pins) {
                return None;
            }

            Some((
                if pinout.source.eq_ignore_ascii_case("overlay") {
                    1
                } else {
                    0
                },
                if is_synthetic_package_name(name) {
                    1
                } else {
                    0
                },
                name.clone(),
            ))
        })
        .collect();
    candidates.sort();
    candidates.into_iter().next().map(|(_, _, name)| name)
}

pub(crate) fn apply_package_display_name_overrides(
    device: &mut DeviceData,
    display_name_overrides: &HashMap<String, String>,
) {
    for (package_name, display_name) in display_name_overrides {
        let matching_key = device
            .pinouts
            .keys()
            .find(|key| key.eq_ignore_ascii_case(package_name))
            .cloned();
        if let Some(package_key) = matching_key {
            if let Some(pinout) = device.pinouts.get_mut(&package_key) {
                pinout.display_name = Some(display_name.clone());
            }
        }
    }
}

pub(crate) fn load_pinout_overlays(device: &mut DeviceData) {
    let re_suffix = Regex::new(r"_\d+$").unwrap();
    let re_paren_device = Regex::new(r"\(([^)]+)\)").unwrap();
    let device_family = dspic33_family(&device.part_number).map(|value| value.to_string());

    for root in read_roots() {
        let overlay_path = root
            .join("pinouts")
            .join(format!("{}.json", device.part_number.to_uppercase()));
        if !overlay_path.exists() {
            continue;
        }

        let text = match fs::read_to_string(&overlay_path) {
            Ok(text) => text,
            Err(_) => continue,
        };

        let overlay: serde_json::Value = match serde_json::from_str(&text) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let display_name_overrides = package_display_name_overrides(&overlay);
        let mut collapsed_display_name_overrides: HashMap<String, String> = HashMap::new();

        if let Some(packages) = overlay.get("packages").and_then(|value| value.as_object()) {
            for (package_name, package_data) in packages {
                let already_exists = device
                    .pinouts
                    .keys()
                    .any(|key| key.eq_ignore_ascii_case(package_name));
                if already_exists {
                    continue;
                }
                if package_data.get("source").and_then(|value| value.as_str()) != Some("overlay") {
                    continue;
                }

                if let Some(ref device_family) = device_family {
                    if let Some(caps) = re_paren_device.captures(package_name) {
                        let inner = caps.get(1).unwrap().as_str();
                        if let Some(package_family) = dspic33_family(inner) {
                            if package_family != device_family.as_str() {
                                continue;
                            }
                        }
                    }
                }

                let mut pin_map: HashMap<u32, String> = HashMap::new();
                if let Some(pins) = package_data.get("pins").and_then(|value| value.as_object()) {
                    for (position_str, pad_value) in pins {
                        if let (Ok(position), Some(pad_name)) =
                            (position_str.parse::<u32>(), pad_value.as_str())
                        {
                            pin_map.insert(position, pad_name.to_string());

                            if !device.pads.contains_key(pad_name) {
                                let base = re_suffix.replace(pad_name, "").to_string();
                                if let Some(src) = device.pads.get(&base).cloned() {
                                    device.pads.insert(
                                        pad_name.to_string(),
                                        Pad {
                                            name: pad_name.to_string(),
                                            functions: src.functions,
                                            rp_number: src.rp_number,
                                            port: src.port,
                                            port_bit: src.port_bit,
                                            analog_channels: src.analog_channels,
                                            is_power: src.is_power,
                                        },
                                    );
                                }
                            }
                        }
                    }
                }

                let pin_count = package_data
                    .get("pin_count")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(pin_map.len() as u64) as u32;

                if let Some(existing_key) =
                    equivalent_existing_pinout_key(device, pin_count, &pin_map)
                {
                    if !existing_key.eq_ignore_ascii_case(package_name) {
                        if let Some(display_name) = {
                            let cleaned = package_name.trim();
                            (!cleaned.is_empty()).then(|| cleaned.to_string())
                        } {
                            collapsed_display_name_overrides
                                .entry(existing_key)
                                .or_insert(display_name);
                        }
                        continue;
                    }
                }

                device.pinouts.insert(
                    package_name.clone(),
                    Pinout {
                        package: package_name.clone(),
                        display_name: None,
                        pin_count,
                        source: "overlay".to_string(),
                        pins: pin_map,
                    },
                );
            }
        }

        apply_package_display_name_overrides(device, &collapsed_display_name_overrides);
        apply_package_display_name_overrides(device, &display_name_overrides);
        replace_redundant_default_pinout(device);
    }
}

pub(crate) fn get_cached_device(part_number: &str) -> Option<(DeviceData, bool, bool)> {
    let filename = format!("{}.json", part_number.to_uppercase());

    for root in read_roots() {
        let dir = root.join("devices");
        let json_path = dir.join(&filename);
        if json_path.exists() {
            if let Ok(text) = fs::read_to_string(&json_path) {
                let has_clc_key = text.contains("\"clc_module_id\"");
                let has_device_info = text.contains("\"device_info\"");
                if let Ok(device) = DeviceData::from_json(&text) {
                    return Some((device, has_clc_key, has_device_info));
                }
            }
        }
    }

    None
}

pub(crate) fn save_cached_device(device: &DeviceData) -> Option<PathBuf> {
    let dir = devices_dir();
    fs::create_dir_all(&dir).ok()?;
    let json_path = dir.join(format!("{}.json", device.part_number.to_uppercase()));
    fs::write(&json_path, device.to_json()).ok()?;
    Some(json_path)
}

pub(crate) fn list_cached_devices() -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();

    for root in read_roots() {
        let dir = root.join("devices");
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|ext| ext == "json").unwrap_or(false) {
                    if let Some(stem) = path.file_stem() {
                        names.insert(stem.to_string_lossy().to_uppercase());
                    }
                }
            }
        }

        let edc_dir = root.join("dfp_cache").join("edc");
        if edc_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&edc_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map(|ext| ext == "PIC").unwrap_or(false) {
                        if let Some(stem) = path.file_stem() {
                            names.insert(stem.to_string_lossy().to_uppercase());
                        }
                    }
                }
            }
        }

        let pinouts = crate::parser::dfp_paths::pinouts_dir();
        if pinouts.is_dir() {
            if let Ok(entries) = fs::read_dir(&pinouts) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map(|ext| ext == "json").unwrap_or(false) {
                        if let Some(stem) = path.file_stem() {
                            names.insert(stem.to_string_lossy().to_uppercase());
                        }
                    }
                }
            }
        }
    }

    names.into_iter().collect()
}

fn known_clc_sources(module_id: &str) -> Option<Vec<Vec<String>>> {
    match module_id {
        "DOS-01577_cla_clc_upb_v1.Module" => Some(vec![
            vec![
                "CLCINA".into(),
                "Fcy".into(),
                "CLC3OUT".into(),
                "LPRC".into(),
                "REFCLKO".into(),
                "Reserved".into(),
                "SCCP2 Aux".into(),
                "SCCP4 Aux".into(),
            ],
            vec![
                "CLCINB".into(),
                "Reserved".into(),
                "CMP1".into(),
                "UART1 TX".into(),
                "Reserved".into(),
                "Reserved".into(),
                "SCCP1 OC".into(),
                "SCCP2 OC".into(),
            ],
            vec![
                "CLCINC".into(),
                "CLC1OUT".into(),
                "CMP2".into(),
                "SPI1 SDO".into(),
                "UART1 RX".into(),
                "CLC4OUT".into(),
                "SCCP3 CEF".into(),
                "SCCP4 CEF".into(),
            ],
            vec![
                "PWM Event A".into(),
                "CLC2OUT".into(),
                "CMP3".into(),
                "SPI1 SDI".into(),
                "Reserved".into(),
                "CLCIND".into(),
                "SCCP1 Aux".into(),
                "SCCP3 Aux".into(),
            ],
        ]),
        _ => None,
    }
}

pub(crate) fn load_clc_sources(device: &mut DeviceData) {
    let part_upper = device.part_number.to_uppercase();
    for root in read_roots() {
        let path = root
            .join("clc_sources")
            .join(format!("{}.json", part_upper));
        if path.exists() {
            if let Ok(text) = fs::read_to_string(&path) {
                if let Ok(sources) = serde_json::from_str::<Vec<Vec<String>>>(&text) {
                    if sources.len() == 4 && sources.iter().all(|group| group.len() == 8) {
                        device.clc_input_sources = Some(sources);
                        return;
                    }
                }
            }
        }
    }

    if let Some(ref module_id) = device.clc_module_id {
        if let Some(sources) = known_clc_sources(module_id) {
            device.clc_input_sources = Some(sources);
        }
    }
}

pub fn clc_sources_dir_public() -> PathBuf {
    clc_sources_dir()
}

#[cfg(test)]
mod tests {
    use super::{equivalent_existing_pinout_key, pin_maps_equivalent};
    use crate::parser::edc_parser::{DeviceData, DeviceInfo, Pinout};
    use std::collections::HashMap;

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

    #[test]
    fn pin_map_equivalence_ignores_duplicate_rail_suffixes() {
        let left = HashMap::from([
            (1, "VDD".to_string()),
            (2, "VSS".to_string()),
            (3, "RA0".to_string()),
        ]);
        let right = HashMap::from([
            (1, "VDD_2".to_string()),
            (2, "VSS_3".to_string()),
            (3, "RA0".to_string()),
        ]);

        assert!(pin_maps_equivalent(&left, &right));
    }

    #[test]
    fn equivalent_pinout_prefers_builtin_package_over_overlay_alias() {
        let device = test_device(vec![
            (
                "STX32 (48-pin VQFN)",
                pinout(
                    "STX32 (48-pin VQFN)",
                    "edc",
                    &[(1, "RA0"), (2, "VDD"), (3, "VSS"), (4, "RB1")],
                ),
            ),
            (
                "48-PIN VQFN",
                pinout(
                    "48-PIN VQFN",
                    "overlay",
                    &[(1, "RA0"), (2, "VDD_2"), (3, "VSS_2"), (4, "RB1")],
                ),
            ),
        ]);

        let new_pins = HashMap::from([
            (1, "RA0".to_string()),
            (2, "VDD_3".to_string()),
            (3, "VSS_4".to_string()),
            (4, "RB1".to_string()),
        ]);

        assert_eq!(
            equivalent_existing_pinout_key(&device, 4, &new_pins),
            Some("STX32 (48-pin VQFN)".to_string())
        );
    }
}
