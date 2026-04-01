//! Parser for Microchip EDC (.PIC) XML files from Device Family Packs.
//! Extracts pin multiplexing, PPS register mappings, peripheral data, and
//! device configuration register (DCR/fuse) definitions.
//!
//! Parsing is split into three logical passes:
//! 1. walk `PinList` to recover package/pad/function data
//! 2. walk `SFRDataSector` to recover register addresses and PPS bitfields
//! 3. walk `WORMHoleSector` to recover DCR fuse definitions
//!
//! Keeping those passes separate mirrors the EDC layout and keeps the cached JSON
//! stable for both frontend rendering and code generation.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const EDC_NS: &str = "http://crownking/edc";

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pad {
    pub name: String,
    pub functions: Vec<String>,
    pub rp_number: Option<u32>,
    pub port: Option<String>,
    pub port_bit: Option<u32>,
    pub analog_channels: Vec<String>,
    pub is_power: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pinout {
    pub package: String,
    pub pin_count: u32,
    pub source: String,
    /// Pin position -> pad name. Serialized with string keys for JSON compat.
    #[serde(
        serialize_with = "serialize_u32_map",
        deserialize_with = "deserialize_u32_map"
    )]
    pub pins: HashMap<u32, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemappablePeripheral {
    pub name: String,
    pub direction: String,
    pub ppsval: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PPSInputMapping {
    pub peripheral: String,
    pub register: String,
    pub register_addr: u32,
    pub field_name: String,
    pub field_mask: u32,
    pub field_offset: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PPSOutputMapping {
    pub rp_number: u32,
    pub register: String,
    pub register_addr: u32,
    pub field_name: String,
    pub field_mask: u32,
    pub field_offset: u32,
}

/// A single allowed value for a DCR field, parsed from `DCRFieldSemantic`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DcrFieldValue {
    pub cname: String,
    pub desc: String,
    pub value: u32,
}

/// A bit-field within a configuration register, parsed from `DCRFieldDef`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DcrField {
    pub cname: String,
    pub desc: String,
    pub mask: u32,
    pub width: u32,
    pub hidden: bool,
    pub values: Vec<DcrFieldValue>,
}

/// A device configuration register definition, parsed from `DCRDef`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DcrRegister {
    pub cname: String,
    pub desc: String,
    pub addr: u32,
    pub default_value: u32,
    pub fields: Vec<DcrField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedPin {
    pub position: u32,
    pub pad_name: String,
    pub functions: Vec<String>,
    pub rp_number: Option<u32>,
    pub port: Option<String>,
    pub port_bit: Option<u32>,
    pub analog_channels: Vec<String>,
    pub is_power: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceData {
    pub part_number: String,
    pub pads: HashMap<String, Pad>,
    pub pinouts: HashMap<String, Pinout>,
    pub default_pinout: String,
    pub remappable_inputs: Vec<RemappablePeripheral>,
    pub remappable_outputs: Vec<RemappablePeripheral>,
    pub pps_input_mappings: Vec<PPSInputMapping>,
    pub pps_output_mappings: Vec<PPSOutputMapping>,
    #[serde(default)]
    pub port_registers: HashMap<String, u32>,
    #[serde(default)]
    pub ansel_bits: HashMap<String, Vec<u32>>,
    #[serde(default)]
    pub fuse_defs: Vec<DcrRegister>,
}

impl DeviceData {
    pub fn get_pinout(&self, package: Option<&str>) -> &Pinout {
        if let Some(pkg) = package {
            if let Some(p) = self.pinouts.get(pkg) {
                return p;
            }
        }
        &self.pinouts[&self.default_pinout]
    }

    pub fn resolve_pins(&self, package: Option<&str>) -> Vec<ResolvedPin> {
        let pinout = self.get_pinout(package);
        let mut positions: Vec<u32> = pinout.pins.keys().copied().collect();
        positions.sort();

        // Overlay packages can reference duplicated rails as `VDD_2`, `VSS_3`, etc.
        // Resolve those aliases back to their canonical base pad when needed.
        let re_suffix = Regex::new(r"_\d+$").unwrap();

        positions
            .into_iter()
            .map(|pos| {
                let pad_name = &pinout.pins[&pos];
                if let Some(pad) = self.pads.get(pad_name) {
                    ResolvedPin {
                        position: pos,
                        pad_name: pad.name.clone(),
                        functions: pad.functions.clone(),
                        rp_number: pad.rp_number,
                        port: pad.port.clone(),
                        port_bit: pad.port_bit,
                        analog_channels: pad.analog_channels.clone(),
                        is_power: pad.is_power,
                    }
                } else {
                    let base = re_suffix.replace(pad_name, "").to_string();
                    if let Some(base_pad) = self.pads.get(&base) {
                        // Keep the package-specific display name while borrowing the
                        // electrical metadata from the canonical base pad.
                        ResolvedPin {
                            position: pos,
                            pad_name: pad_name.clone(),
                            functions: base_pad.functions.clone(),
                            rp_number: base_pad.rp_number,
                            port: base_pad.port.clone(),
                            port_bit: base_pad.port_bit,
                            analog_channels: base_pad.analog_channels.clone(),
                            is_power: base_pad.is_power,
                        }
                    } else {
                        // Last-resort fallback for pads that only exist in the package map.
                        // Treat them as fixed/power-like pins so the UI can still render.
                        ResolvedPin {
                            position: pos,
                            pad_name: pad_name.clone(),
                            functions: vec![pad_name.clone()],
                            rp_number: None,
                            port: None,
                            port_bit: None,
                            analog_channels: vec![],
                            is_power: true,
                        }
                    }
                }
            })
            .collect()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap()
    }

    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }
}

// ---------------------------------------------------------------------------
// Custom serde for HashMap<u32, String> <-> JSON object with string keys
// ---------------------------------------------------------------------------

fn serialize_u32_map<S>(map: &HashMap<u32, String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeMap;
    let mut m = serializer.serialize_map(Some(map.len()))?;
    for (k, v) in map {
        m.serialize_entry(&k.to_string(), v)?;
    }
    m.end()
}

fn deserialize_u32_map<'de, D>(deserializer: D) -> Result<HashMap<u32, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let string_map: HashMap<String, String> = HashMap::deserialize(deserializer)?;
    let mut result = HashMap::new();
    for (k, v) in string_map {
        let key: u32 = k.parse().map_err(serde::de::Error::custom)?;
        result.insert(key, v);
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// EDC XML parsing
// ---------------------------------------------------------------------------

fn parse_int(s: &str) -> u32 {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).unwrap_or(0)
    } else {
        s.parse().unwrap_or(0)
    }
}

/// Extract the numeric value from a DCRFieldSemantic `when` expression.
/// Typical format: `(field & 0x3) == 0x3` → returns 3.
fn parse_when_value(when: &str) -> u32 {
    let re = Regex::new(r"==\s*0x([0-9a-fA-F]+)").unwrap();
    if let Some(caps) = re.captures(when) {
        u32::from_str_radix(caps.get(1).unwrap().as_str(), 16).unwrap_or(0)
    } else {
        let re_dec = Regex::new(r"==\s*(\d+)").unwrap();
        re_dec
            .captures(when)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0)
    }
}

fn extract_port_info(name: &str) -> (Option<String>, Option<u32>) {
    let re = Regex::new(r"^R([A-Z])(\d+)$").unwrap();
    if let Some(caps) = re.captures(name) {
        let port = caps.get(1).unwrap().as_str().to_string();
        let bit: u32 = caps.get(2).unwrap().as_str().parse().unwrap_or(0);
        (Some(port), Some(bit))
    } else {
        (None, None)
    }
}

fn extract_rp_number(name: &str) -> Option<u32> {
    let re = Regex::new(r"^RP(\d+)$").unwrap();
    re.captures(name)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok())
}

fn pad_canonical_name(func_names: &[String]) -> String {
    let re = Regex::new(r"^R[A-E]\d+$").unwrap();
    for name in func_names {
        if re.is_match(name) {
            return name.clone();
        }
    }
    func_names
        .first()
        .cloned()
        .unwrap_or_else(|| "UNKNOWN".to_string())
}

fn get_edc_attr<'a>(node: &'a roxmltree::Node, attr: &str) -> Option<&'a str> {
    node.attribute((EDC_NS, attr))
        .or_else(|| node.attribute(attr))
}

pub fn parse_edc_file(filepath: &Path) -> Result<DeviceData, String> {
    let xml_str = fs::read_to_string(filepath).map_err(|e| format!("Read error: {e}"))?;
    let doc = roxmltree::Document::parse(&xml_str).map_err(|e| format!("XML parse error: {e}"))?;
    let root = doc.root_element();

    let part_number = get_edc_attr(&root, "name").unwrap_or("UNKNOWN").to_string();

    let mut pads: HashMap<String, Pad> = HashMap::new();
    let mut pinout_map: HashMap<u32, String> = HashMap::new();
    let mut remappable_inputs: Vec<RemappablePeripheral> = Vec::new();
    let mut remappable_outputs: Vec<RemappablePeripheral> = Vec::new();
    let mut pkg_name = String::from("default");

    // First pass: recover physical pin order, pad aliases, and remappable
    // peripheral declarations from the package-oriented `PinList`.
    let pinlist = root
        .descendants()
        .find(|n| n.tag_name().name() == "PinList");

    if let Some(pinlist) = pinlist {
        if let Some(desc) = get_edc_attr(&pinlist, "desc") {
            pkg_name = desc.trim().to_string();
        }

        let mut pin_position: u32 = 0;
        // These name patterns are stable across all pins in the package, so keep
        // the regex compilation outside the hot inner loop.
        let re_an = Regex::new(r"^AN\d+$").unwrap();
        let re_ana = Regex::new(r"^ANA\d+$").unwrap();

        for child in pinlist.children().filter(|n| n.is_element()) {
            let tag = child.tag_name().name();

            if tag == "Pin" {
                pin_position += 1;

                let vpins: Vec<_> = child
                    .children()
                    .filter(|n| n.is_element() && n.tag_name().name() == "VirtualPin")
                    .collect();

                let func_names: Vec<String> = vpins
                    .iter()
                    .filter_map(|vp| get_edc_attr(vp, "name").map(|s| s.to_string()))
                    .collect();

                let mut rp_num: Option<u32> = None;
                let mut port: Option<String> = None;
                let mut port_bit: Option<u32> = None;
                let mut analog = Vec::new();
                let mut is_power = false;

                for name in &func_names {
                    if matches!(name.as_str(), "VDD" | "VSS" | "AVDD" | "AVSS" | "MCLR") {
                        is_power = true;
                    }
                    if rp_num.is_none() {
                        rp_num = extract_rp_number(name);
                    }
                    let (p, b) = extract_port_info(name);
                    if p.is_some() {
                        port = p;
                        port_bit = b;
                    }
                    if re_an.is_match(name) || re_ana.is_match(name) {
                        analog.push(name.clone());
                    }
                }

                let pad_name = pad_canonical_name(&func_names);

                if pads.contains_key(&pad_name) && is_power {
                    // Power rails often appear multiple times with the same logical name.
                    // Suffix duplicates so each package position stays addressable.
                    let count = pads.keys().filter(|k| k.starts_with(&pad_name)).count();
                    let unique_name = format!("{}_{}", pad_name, count + 1);
                    pinout_map.insert(pin_position, unique_name.clone());
                    pads.insert(
                        unique_name.clone(),
                        Pad {
                            name: unique_name,
                            functions: func_names,
                            rp_number: rp_num,
                            port,
                            port_bit,
                            analog_channels: analog,
                            is_power,
                        },
                    );
                } else {
                    pinout_map.insert(pin_position, pad_name.clone());
                    pads.insert(
                        pad_name.clone(),
                        Pad {
                            name: pad_name,
                            functions: func_names,
                            rp_number: rp_num,
                            port,
                            port_bit,
                            analog_channels: analog,
                            is_power,
                        },
                    );
                }
            } else if tag == "RemappablePin" {
                let direction = get_edc_attr(&child, "direction").unwrap_or("").to_string();
                let vp = child
                    .children()
                    .find(|n| n.is_element() && n.tag_name().name() == "VirtualPin");
                if let Some(vp) = vp {
                    let name = get_edc_attr(&vp, "name").unwrap_or("").to_string();
                    let ppsval = get_edc_attr(&vp, "ppsval").and_then(|s| s.parse().ok());
                    let rp = RemappablePeripheral {
                        name,
                        direction: direction.clone(),
                        ppsval,
                    };
                    // Inputs and outputs are written through different register families
                    // (`RPINR*` vs `RPOR*`), so split them immediately.
                    if direction == "in" {
                        remappable_inputs.push(rp);
                    } else {
                        remappable_outputs.push(rp);
                    }
                }
            }
        }
    }

    // Second pass: parse SFR metadata to discover register addresses, ANSEL bit
    // availability, and PPS field encodings needed for code generation.
    let mut pps_input_mappings: Vec<PPSInputMapping> = Vec::new();
    let mut pps_output_mappings: Vec<PPSOutputMapping> = Vec::new();
    let mut port_registers: HashMap<String, u32> = HashMap::new();
    let mut ansel_bits: HashMap<String, Vec<u32>> = HashMap::new();

    let re_port_reg = Regex::new(r"^(TRIS|ANSEL|LAT|PORT)[A-Z]$").unwrap();
    let re_ansel = Regex::new(r"^ANSEL([A-Z])$").unwrap();
    let re_rp_field = Regex::new(r"^RP(\d+)R$").unwrap();

    let sfr_sector = root
        .descendants()
        .find(|n| n.tag_name().name() == "SFRDataSector");

    if let Some(sfr_sector) = sfr_sector {
        for sfr in sfr_sector
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "SFRDef")
        {
            let cname = get_edc_attr(&sfr, "cname").unwrap_or("");
            let addr = parse_int(get_edc_attr(&sfr, "_addr").unwrap_or("0"));

            if re_port_reg.is_match(cname) {
                port_registers.insert(cname.to_string(), addr);
            }

            if let Some(caps) = re_ansel.captures(cname) {
                let port_letter = caps.get(1).unwrap().as_str().to_string();
                let mut bits = Vec::new();
                let re_ansel_bit = Regex::new(&format!(r"^ANSEL{}(\d+)$", port_letter)).unwrap();
                for fld in sfr
                    .descendants()
                    .filter(|n| n.is_element() && n.tag_name().name() == "SFRFieldDef")
                {
                    let fname = get_edc_attr(&fld, "cname").unwrap_or("");
                    if let Some(caps) = re_ansel_bit.captures(fname) {
                        if let Ok(bit) = caps.get(1).unwrap().as_str().parse::<u32>() {
                            bits.push(bit);
                        }
                    }
                }
                bits.sort();
                ansel_bits.insert(port_letter, bits);
            }

            if cname.starts_with("RPINR") {
                let mut bit_offset: u32 = 0;
                for mode_child in sfr.descendants().filter(|n| n.is_element()) {
                    let mtag = mode_child.tag_name().name();
                    if mtag == "AdjustPoint" {
                        // Some EDC register descriptions advance the active bit cursor with
                        // explicit adjust markers instead of contiguous field definitions.
                        bit_offset += parse_int(get_edc_attr(&mode_child, "offset").unwrap_or("0"));
                    } else if mtag == "SFRFieldDef" {
                        let field_name =
                            get_edc_attr(&mode_child, "cname").unwrap_or("").to_string();
                        let mask = parse_int(get_edc_attr(&mode_child, "mask").unwrap_or("0"));
                        pps_input_mappings.push(PPSInputMapping {
                            peripheral: field_name.clone(),
                            register: cname.to_string(),
                            register_addr: addr,
                            field_name,
                            field_mask: mask,
                            field_offset: bit_offset,
                        });
                        bit_offset +=
                            parse_int(get_edc_attr(&mode_child, "nzwidth").unwrap_or("0"));
                    }
                }
            } else if let Some(suffix) = cname.strip_prefix("RPOR") {
                // Match RPOR0, RPOR1, ... but not RPOR-something-else.
                if suffix.chars().all(|c| c.is_ascii_digit()) {
                    let mut bit_offset: u32 = 0;
                    for mode_child in sfr.descendants().filter(|n| n.is_element()) {
                        let mtag = mode_child.tag_name().name();
                        if mtag == "AdjustPoint" {
                            // RPOR field offsets use the same running bit-offset model as
                            // RPINR parsing above.
                            bit_offset +=
                                parse_int(get_edc_attr(&mode_child, "offset").unwrap_or("0"));
                        } else if mtag == "SFRFieldDef" {
                            let field_name =
                                get_edc_attr(&mode_child, "cname").unwrap_or("").to_string();
                            let mask = parse_int(get_edc_attr(&mode_child, "mask").unwrap_or("0"));
                            let rp_num = re_rp_field
                                .captures(&field_name)
                                .and_then(|c| c.get(1))
                                .and_then(|m| m.as_str().parse().ok())
                                .unwrap_or(0);
                            pps_output_mappings.push(PPSOutputMapping {
                                rp_number: rp_num,
                                register: cname.to_string(),
                                register_addr: addr,
                                field_name,
                                field_mask: mask,
                                field_offset: bit_offset,
                            });
                            bit_offset +=
                                parse_int(get_edc_attr(&mode_child, "nzwidth").unwrap_or("0"));
                        }
                    }
                }
            }
        }
    }

    // Third pass: parse DCR (Device Configuration Register) definitions from
    // `WORMHoleSector` to discover fuse registers, fields, and valid values.
    let mut fuse_defs: Vec<DcrRegister> = Vec::new();

    for sector in root.descendants().filter(|n| {
        n.tag_name().name() == "WORMHoleSector"
            && get_edc_attr(n, "regionid")
                .map(|id| id.contains("cfgmem") || id.contains("config"))
                .unwrap_or(false)
    }) {
        for dcr_def in sector
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "DCRDef")
        {
            let cname = get_edc_attr(&dcr_def, "cname").unwrap_or("").to_string();
            let desc = get_edc_attr(&dcr_def, "desc").unwrap_or("").to_string();
            let addr = parse_int(get_edc_attr(&dcr_def, "_addr").unwrap_or("0"));
            let default_value = parse_int(get_edc_attr(&dcr_def, "default").unwrap_or("0"));
            let reg_hidden = get_edc_attr(&dcr_def, "ishidden")
                .map(|v| v == "true")
                .unwrap_or(false);

            if cname.is_empty() || reg_hidden {
                continue;
            }

            let mut fields: Vec<DcrField> = Vec::new();

            // Find the first DCRMode (standard operating mode DS.0)
            let dcr_mode = dcr_def
                .descendants()
                .find(|n| n.is_element() && n.tag_name().name() == "DCRMode");

            if let Some(mode) = dcr_mode {
                for child in mode.children().filter(|n| n.is_element()) {
                    if child.tag_name().name() != "DCRFieldDef" {
                        continue;
                    }

                    let field_cname = get_edc_attr(&child, "cname").unwrap_or("").to_string();
                    let field_desc = get_edc_attr(&child, "desc").unwrap_or("").to_string();
                    let field_mask = parse_int(get_edc_attr(&child, "mask").unwrap_or("0"));
                    let field_width = parse_int(get_edc_attr(&child, "nzwidth").unwrap_or("0"));
                    let hidden = get_edc_attr(&child, "ishidden")
                        .map(|v| v == "true")
                        .unwrap_or(false);

                    if field_cname.is_empty() || field_mask == 0 {
                        continue;
                    }

                    let mut values: Vec<DcrFieldValue> = Vec::new();
                    for sem in child
                        .children()
                        .filter(|n| n.is_element() && n.tag_name().name() == "DCRFieldSemantic")
                    {
                        let val_cname = get_edc_attr(&sem, "cname").unwrap_or("").to_string();
                        let val_desc = get_edc_attr(&sem, "desc").unwrap_or("").to_string();
                        let when = get_edc_attr(&sem, "when").unwrap_or("");
                        let value = parse_when_value(when);

                        if !val_cname.is_empty() {
                            values.push(DcrFieldValue {
                                cname: val_cname,
                                desc: val_desc,
                                value,
                            });
                        }
                    }

                    // Skip range-only fields (e.g. DMT interval/count) that have
                    // no enumerated values — they can't be presented as a dropdown.
                    if !values.is_empty() {
                        fields.push(DcrField {
                            cname: field_cname,
                            desc: field_desc,
                            mask: field_mask,
                            width: field_width,
                            hidden,
                            values,
                        });
                    }
                }
            }

            // Only include registers that have at least one selectable field.
            if !fields.is_empty() {
                fuse_defs.push(DcrRegister {
                    cname,
                    desc,
                    addr,
                    default_value,
                    fields,
                });
            }
        }
    }

    let default_pinout = Pinout {
        package: pkg_name.clone(),
        pin_count: pinout_map.len() as u32,
        source: "edc".to_string(),
        pins: pinout_map,
    };

    let mut pinouts = HashMap::new();
    pinouts.insert(pkg_name.clone(), default_pinout);

    Ok(DeviceData {
        part_number,
        pads,
        pinouts,
        default_pinout: pkg_name,
        remappable_inputs,
        remappable_outputs,
        pps_input_mappings,
        pps_output_mappings,
        port_registers,
        ansel_bits,
        fuse_defs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_int() {
        assert_eq!(parse_int("0x0D04"), 3332);
        assert_eq!(parse_int("255"), 255);
        assert_eq!(parse_int("0X1F"), 31);
    }

    #[test]
    fn test_extract_port_info() {
        assert_eq!(extract_port_info("RB5"), (Some("B".to_string()), Some(5)));
        assert_eq!(extract_port_info("MCLR"), (None, None));
    }

    #[test]
    fn test_extract_rp_number() {
        assert_eq!(extract_rp_number("RP32"), Some(32));
        assert_eq!(extract_rp_number("RB5"), None);
    }

    #[test]
    fn test_pad_canonical_name() {
        let funcs = vec!["OA1IN-".to_string(), "ANA1".to_string(), "RA1".to_string()];
        assert_eq!(pad_canonical_name(&funcs), "RA1");

        let power = vec!["VDD".to_string()];
        assert_eq!(pad_canonical_name(&power), "VDD");
    }

    #[test]
    fn test_device_data_json_roundtrip() {
        // Load the test fixture
        let fixture_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../tests/fixtures/DSPIC33CK64MP102.json"
        );
        if !Path::new(fixture_path).exists() {
            return; // Skip if fixture not available
        }
        let json_str = fs::read_to_string(fixture_path).unwrap();
        let device = DeviceData::from_json(&json_str).unwrap();

        assert_eq!(device.part_number, "DSPIC33CK64MP102");
        assert!(device.pads.contains_key("RB0"));
        assert!(device.pads.contains_key("MCLR"));

        // Round-trip
        let json2 = device.to_json();
        let device2 = DeviceData::from_json(&json2).unwrap();
        assert_eq!(device.part_number, device2.part_number);
        assert_eq!(device.pads.len(), device2.pads.len());
        assert_eq!(device.pinouts.len(), device2.pinouts.len());
    }

    #[test]
    fn test_resolve_pins() {
        let fixture_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../tests/fixtures/DSPIC33CK64MP102.json"
        );
        if !Path::new(fixture_path).exists() {
            return;
        }
        let json_str = fs::read_to_string(fixture_path).unwrap();
        let device = DeviceData::from_json(&json_str).unwrap();

        let pins = device.resolve_pins(None);
        assert_eq!(pins.len(), 28);
        assert_eq!(pins[0].position, 1);
        assert_eq!(pins[0].pad_name, "RA1");
    }
}
