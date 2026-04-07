//! Datasheet-package comparison helpers for verification.
//!
//! The provider pass now returns extracted package tables only. This module
//! turns that extraction into deterministic local diffs against the loaded
//! device/package data, filters out incompatible family branches, and keeps the
//! correction model independent of provider-written prose.

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};

use crate::part_profile::PartProfile;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinCorrection {
    pub pin_position: u32,
    pub current_pad: String,
    pub datasheet_pad: String,
    pub current_functions: Vec<String>,
    pub datasheet_functions: Vec<String>,
    pub correction_type: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageResult {
    pub package_name: String,
    pub pin_count: u32,
    pub pins: HashMap<u32, String>,
    pub pin_functions: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub corrections: Vec<PinCorrection>,
    #[serde(default)]
    pub match_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtractedPackage {
    pub pin_count: u32,
    #[serde(default)]
    pub pins: HashMap<u32, String>,
    #[serde(default)]
    pub pin_functions: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VerifierExtraction {
    #[serde(default)]
    pub packages: HashMap<String, ExtractedPackage>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clc_input_sources: Option<Vec<Vec<String>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyResult {
    pub part_number: String,
    pub packages: HashMap<String, PackageResult>,
    #[serde(default)]
    pub notes: Vec<String>,
    /// CLC input source MUX mapping extracted from the CLCxSEL register chapter:
    /// 4 groups (DS1–DS4) of 8 source labels. `None` when the datasheet lacks
    /// a CLC chapter or the provider couldn't locate it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clc_input_sources: Option<Vec<Vec<String>>>,
    #[serde(skip)]
    pub raw_response: String,
}

impl VerifyResult {
    pub fn to_overlay_json(&self) -> Value {
        let mut packages = serde_json::Map::new();
        for (name, pkg) in &self.packages {
            let mut pins_map = serde_json::Map::new();
            let mut sorted_pins: Vec<_> = pkg.pins.iter().collect();
            sorted_pins.sort_by_key(|(k, _)| **k);
            for (pos, pad) in sorted_pins {
                pins_map.insert(pos.to_string(), Value::String(pad.clone()));
            }
            let mut pkg_obj = serde_json::Map::new();
            pkg_obj.insert("source".into(), Value::String("overlay".into()));
            pkg_obj.insert("pin_count".into(), Value::Number(pkg.pin_count.into()));
            pkg_obj.insert("pins".into(), Value::Object(pins_map));
            packages.insert(name.clone(), Value::Object(pkg_obj));
        }
        let mut root = serde_json::Map::new();
        root.insert("packages".into(), Value::Object(packages));
        Value::Object(root)
    }
}

/// Collapse package-specific rail aliases like `VDD_2` back to the canonical
/// pad name so overlay/datasheet comparisons ignore duplicated suffixes.
fn normalize_pad(name: &str) -> String {
    let re = Regex::new(r"_\d+$").unwrap();
    let upper = name.to_uppercase();
    re.replace(upper.trim(), "").to_string()
}

fn package_explicit_branch_markers(package_name: &str, family_prefix: &str) -> Vec<String> {
    let token_re = Regex::new(r"(?i)(?:DSPIC|PIC)[A-Z0-9X]+").unwrap();
    let package_upper = package_name.to_uppercase();
    let mut markers: Vec<String> = token_re
        .find_iter(&package_upper)
        .filter_map(|capture| {
            let token = capture.as_str();
            if !token.starts_with(family_prefix) {
                return None;
            }
            let suffix = token.trim_start_matches(family_prefix);
            let normalized = suffix.trim_start_matches(|ch: char| ch.is_ascii_digit() || ch == 'X');
            PartProfile::from_part_number(normalized)
                .branch()
                .map(ToOwned::to_owned)
        })
        .collect();
    markers.sort();
    markers.dedup();
    markers
}

pub(crate) fn package_matches_selected_device_branch(
    package_name: &str,
    part_number: &str,
) -> bool {
    let profile = PartProfile::from_part_number(part_number);
    let Some(selected_branch) = profile.branch() else {
        return true;
    };
    let Some(family_prefix) = profile.family_prefix() else {
        return true;
    };
    let markers = package_explicit_branch_markers(package_name, family_prefix);
    markers.is_empty() || markers.iter().any(|marker| marker == selected_branch)
}

fn normalize_function_name(name: &str) -> String {
    let compact = name
        .trim()
        .to_uppercase()
        .replace([' ', '\t', '\n', '\r'], "");
    if compact.is_empty() {
        return compact;
    }

    let cmp_re = Regex::new(r"^CMP[A-Z]*(\d+)$").unwrap();
    if let Some(caps) = cmp_re.captures(&compact) {
        return format!("CMP{}", &caps[1]);
    }
    if compact.starts_with("DACOUT") {
        return "DACOUT".to_string();
    }

    compact
}

fn is_pps_like_function(name: &str) -> bool {
    Regex::new(
        r"(?i)^(PWM\d+[HL]?|U\d+(TX|RX|CTS|RTS)|SPI\d+(SDI|SDO|SCK|SS)|SDA\d*|SCL\d*|SCK\d*|SDI\d*|SDO\d*|SS\d*|CAN\d*(TX|RX)|REFCLKO?\d*|QE[AB]\d*|QEI(HOME|INDX|CMP)?\d*|INDX\d*|HOME\d*|FLT\d*|SENT\d+|RP\d+)$"
    )
    .unwrap()
    .is_match(name)
}

fn correction_note_from_functions(
    prefix: &str,
    pin_position: u32,
    pad_name: &str,
    functions: &[String],
) -> String {
    format!(
        "Pin {pin_position}: {pad_name} {prefix} {}.",
        functions.join(", ")
    )
}

fn pin_is_powerish(pin: Option<&Value>, datasheet_pad: &str, current_pad: &str) -> bool {
    let is_power = pin
        .and_then(|value| value.get("is_power"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if is_power {
        return true;
    }

    let normalized_pads = [normalize_pad(datasheet_pad), normalize_pad(current_pad)];
    normalized_pads.iter().any(|pad| {
        matches!(
            pad.as_str(),
            "VDD" | "VSS" | "AVDD" | "AVSS" | "VCAP" | "VDDCORE" | "SWVDD" | "SWVSS" | "LX"
        )
    })
}

fn lookup_package_functions(pkg: &PackageResult, pad_name: &str) -> Vec<String> {
    let normalized_pad = normalize_pad(pad_name);
    pkg.pin_functions
        .iter()
        .find(|(pad, _)| normalize_pad(pad) == normalized_pad)
        .map(|(_, functions)| functions.clone())
        .unwrap_or_default()
}

fn normalized_function_set(functions: &[String], rp_capable: bool) -> BTreeSet<String> {
    functions
        .iter()
        .map(|name| normalize_function_name(name))
        .filter(|name| !name.is_empty())
        .filter(|name| !(rp_capable && is_pps_like_function(name)))
        .collect()
}

fn current_pin_function_list(pin: &Value) -> Vec<String> {
    pin.get("functions")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(|entry| entry.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn build_local_corrections(
    pkg: &PackageResult,
    current_pins: &HashMap<u64, &Value>,
) -> Vec<PinCorrection> {
    let mut corrections = Vec::new();
    let mut all_positions: Vec<u32> = pkg
        .pins
        .keys()
        .copied()
        .chain(current_pins.keys().map(|pos| *pos as u32))
        .collect();
    all_positions.sort_unstable();
    all_positions.dedup();

    for position in all_positions {
        let current = current_pins.get(&(position as u64)).copied();
        let current_pad = current
            .and_then(|pin| pin.get("pad_name").or_else(|| pin.get("pad")))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        let datasheet_pad = pkg.pins.get(&position).cloned().unwrap_or_default();

        if current.is_none() && !datasheet_pad.is_empty() {
            corrections.push(PinCorrection {
                pin_position: position,
                current_pad: String::new(),
                datasheet_pad: datasheet_pad.clone(),
                current_functions: Vec::new(),
                datasheet_functions: lookup_package_functions(pkg, &datasheet_pad),
                correction_type: "missing_pin".to_string(),
                note: format!(
                    "Pin {position}: datasheet package contains {datasheet_pad}, but the loaded package data has no pin at this position."
                ),
            });
            continue;
        }

        if current.is_some() && datasheet_pad.is_empty() {
            corrections.push(PinCorrection {
                pin_position: position,
                current_pad: current_pad.clone(),
                datasheet_pad: String::new(),
                current_functions: current
                    .map(current_pin_function_list)
                    .unwrap_or_default(),
                datasheet_functions: Vec::new(),
                correction_type: "extra_pin".to_string(),
                note: format!(
                    "Pin {position}: loaded package data contains {current_pad}, but the datasheet package has no pin at this position."
                ),
            });
            continue;
        }

        if current.is_none() {
            continue;
        }

        if normalize_pad(&datasheet_pad) != normalize_pad(&current_pad) {
            corrections.push(PinCorrection {
                pin_position: position,
                current_pad: current_pad.clone(),
                datasheet_pad: datasheet_pad.clone(),
                current_functions: current
                    .map(current_pin_function_list)
                    .unwrap_or_default(),
                datasheet_functions: lookup_package_functions(pkg, &datasheet_pad),
                correction_type: "wrong_pad".to_string(),
                note: format!(
                    "Pin {position}: current parsed pad is {current_pad}, but the datasheet lists {datasheet_pad}."
                ),
            });
        }

        if pin_is_powerish(current, &datasheet_pad, &current_pad) {
            continue;
        }

        let current_functions = current.map(current_pin_function_list).unwrap_or_default();
        let datasheet_functions = lookup_package_functions(pkg, &datasheet_pad);
        let rp_capable = current
            .and_then(|pin| pin.get("rp_number"))
            .and_then(|value| value.as_u64())
            .is_some();
        let current_set = normalized_function_set(&current_functions, rp_capable);
        let datasheet_set = normalized_function_set(&datasheet_functions, rp_capable);

        let missing: Vec<String> = datasheet_set.difference(&current_set).cloned().collect();
        if !missing.is_empty() {
            corrections.push(PinCorrection {
                pin_position: position,
                current_pad: current_pad.clone(),
                datasheet_pad: datasheet_pad.clone(),
                current_functions: current_functions.clone(),
                datasheet_functions: datasheet_functions.clone(),
                correction_type: "missing_functions".to_string(),
                note: correction_note_from_functions(
                    "is missing fixed functions",
                    position,
                    &datasheet_pad,
                    &missing,
                ),
            });
        }

        let extra: Vec<String> = current_set.difference(&datasheet_set).cloned().collect();
        if !extra.is_empty() {
            corrections.push(PinCorrection {
                pin_position: position,
                current_pad: current_pad.clone(),
                datasheet_pad: datasheet_pad.clone(),
                current_functions,
                datasheet_functions,
                correction_type: "extra_functions".to_string(),
                note: correction_note_from_functions(
                    "has extra fixed functions",
                    position,
                    &current_pad,
                    &extra,
                ),
            });
        }
    }

    corrections
}

fn strip_verifier_json_fencing(raw: &str) -> String {
    let json_str = raw.trim();
    if json_str.starts_with("```") {
        if let (Some(start), Some(end)) = (json_str.find('{'), json_str.rfind('}')) {
            if start <= end {
                return json_str[start..=end].to_string();
            }
        }
    }
    json_str.to_string()
}

pub fn extract_verifier_json_value(raw: &str) -> Result<Value, String> {
    let json_str = strip_verifier_json_fencing(raw);
    serde_json::from_str(&json_str).map_err(|error| error.to_string())
}

pub fn verifier_extraction_from_value(data: &Value) -> VerifierExtraction {
    let mut extraction = VerifierExtraction::default();

    if let Some(packages) = data.get("packages").and_then(|value| value.as_object()) {
        for (pkg_name, pkg_data) in packages {
            let mut pins: HashMap<u32, String> = HashMap::new();
            if let Some(pin_obj) = pkg_data.get("pins").and_then(|value| value.as_object()) {
                for (pos_str, pad) in pin_obj {
                    if let (Ok(pos), Some(pad_str)) = (pos_str.parse::<u32>(), pad.as_str()) {
                        pins.insert(pos, pad_str.to_string());
                    }
                }
            }

            let pin_functions: HashMap<String, Vec<String>> = pkg_data
                .get("pin_functions")
                .and_then(|value| value.as_object())
                .map(|obj| {
                    obj.iter()
                        .map(|(pad, functions)| {
                            let extracted = functions
                                .as_array()
                                .map(|values| {
                                    values
                                        .iter()
                                        .filter_map(|value| {
                                            value.as_str().map(|entry| entry.to_string())
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();
                            (pad.clone(), extracted)
                        })
                        .collect()
                })
                .unwrap_or_default();

            let pin_count = pkg_data
                .get("pin_count")
                .and_then(|value| value.as_u64())
                .unwrap_or(pins.len() as u64) as u32;

            extraction.packages.insert(
                pkg_name.clone(),
                ExtractedPackage {
                    pin_count,
                    pins,
                    pin_functions,
                },
            );
        }
    }

    if let Some(notes) = data.get("notes").and_then(|value| value.as_array()) {
        extraction.notes = notes
            .iter()
            .filter_map(|value| value.as_str().map(|note| note.to_string()))
            .collect();
    }

    if let Some(clc_arr) = data
        .get("clc_input_sources")
        .and_then(|value| value.as_array())
    {
        let sources: Vec<Vec<String>> = clc_arr
            .iter()
            .filter_map(|group| {
                group.as_array().map(|entries| {
                    entries
                        .iter()
                        .filter_map(|entry| entry.as_str().map(|value| value.to_string()))
                        .collect()
                })
            })
            .collect();
        if !sources.is_empty() {
            extraction.clc_input_sources = Some(sources);
        }
    }

    extraction
}

pub fn parse_verifier_extraction(raw: &str) -> Result<VerifierExtraction, String> {
    extract_verifier_json_value(raw).map(|value| verifier_extraction_from_value(&value))
}

pub fn build_verify_result(
    extraction: &VerifierExtraction,
    device_data: &Value,
    raw_response: &str,
) -> VerifyResult {
    let part = device_data
        .get("part_number")
        .and_then(|value| value.as_str())
        .unwrap_or("UNKNOWN")
        .to_string();

    let mut result = VerifyResult {
        part_number: part.clone(),
        packages: HashMap::new(),
        notes: Vec::new(),
        clc_input_sources: None,
        raw_response: raw_response.to_string(),
    };

    let current_pins: HashMap<u64, &Value> = device_data
        .get("pins")
        .and_then(|value| value.as_array())
        .map(|pins| {
            pins.iter()
                .filter_map(|pin| {
                    pin.get("position")
                        .and_then(|value| value.as_u64())
                        .map(|position| (position, pin))
                })
                .collect()
        })
        .unwrap_or_default();

    let target_pin_count = device_data
        .get("pin_count")
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as u32;
    let mut excluded_branch_packages: Vec<String> = Vec::new();
    let mut excluded_pin_count_packages: Vec<String> = Vec::new();

    for (pkg_name, pkg_data) in &extraction.packages {
        if !package_matches_selected_device_branch(pkg_name, &part) {
            excluded_branch_packages.push(pkg_name.clone());
            continue;
        }

        let pin_count = pkg_data.pin_count;
        if target_pin_count > 0 && pin_count != target_pin_count {
            excluded_pin_count_packages.push(pkg_name.clone());
            continue;
        }

        let mut matches = 0u32;
        let mut total = 0u32;
        for (pos, pad) in &pkg_data.pins {
            if let Some(current) = current_pins.get(&(*pos as u64)) {
                total += 1;
                let current_pad = current
                    .get("pad_name")
                    .or_else(|| current.get("pad"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                if normalize_pad(pad) == normalize_pad(current_pad) {
                    matches += 1;
                }
            }
        }
        let match_score = if total > 0 {
            matches as f64 / total as f64
        } else {
            0.0
        };

        let mut package_result = PackageResult {
            package_name: pkg_name.clone(),
            pin_count,
            pins: pkg_data.pins.clone(),
            pin_functions: pkg_data.pin_functions.clone(),
            corrections: Vec::new(),
            match_score,
        };
        package_result.corrections = build_local_corrections(&package_result, &current_pins);
        result.packages.insert(pkg_name.clone(), package_result);
    }

    result.notes = extraction.notes.clone();
    if !excluded_branch_packages.is_empty() {
        excluded_branch_packages.sort();
        result.notes.push(format!(
            "Ignored {} extracted package table{} because they explicitly target a different device branch than {}: {}.",
            excluded_branch_packages.len(),
            if excluded_branch_packages.len() == 1 { "" } else { "s" },
            part,
            excluded_branch_packages.join(", ")
        ));
    }
    if !excluded_pin_count_packages.is_empty() {
        excluded_pin_count_packages.sort();
        result.notes.push(format!(
            "Ignored {} extracted package table{} because their pin count does not match the selected {}-pin device: {}.",
            excluded_pin_count_packages.len(),
            if excluded_pin_count_packages.len() == 1 { "" } else { "s" },
            target_pin_count,
            excluded_pin_count_packages.join(", ")
        ));
    }

    if let Some(ref sources) = extraction.clc_input_sources {
        if sources.len() == 4 && sources.iter().all(|group| group.len() == 8) {
            result.clc_input_sources = Some(sources.clone());
        }
    }

    result
}

pub fn parse_verifier_response(raw: &str, device_data: &Value) -> VerifyResult {
    match parse_verifier_extraction(raw) {
        Ok(extraction) => build_verify_result(&extraction, device_data, raw),
        Err(error) => VerifyResult {
            part_number: device_data
                .get("part_number")
                .and_then(|value| value.as_str())
                .unwrap_or("UNKNOWN")
                .to_string(),
            packages: HashMap::new(),
            notes: vec![format!(
                "Failed to parse verifier response as JSON: {error}"
            )],
            clc_input_sources: None,
            raw_response: raw.to_string(),
        },
    }
}
