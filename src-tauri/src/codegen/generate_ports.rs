//! Port and analog-peripheral code emission for generated MCU init output.
//!
//! This module owns `configure_ports()` and `configure_analog()` so the
//! top-level generator can focus on file layout and phase ordering while these
//! helpers keep GPIO-mode, debugger reservations, and family-specific op-amp
//! enable rules together.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::codegen::generate_support::{align_comments, push_section_comment};
use crate::codegen::generate_types::PinAssignment;
use crate::parser::edc_parser::{DeviceData, ResolvedPin};
use crate::part_profile::PartProfile;

const ICSP_PATTERN: &str = r"^MCLR$|^PGC\d$|^PGD\d$|^PGEC\d$|^PGED\d$";

static ICSP_RE: Lazy<Regex> = Lazy::new(|| Regex::new(ICSP_PATTERN).unwrap());
static OPAMP_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^OA(\d+)(OUT|IN[+-]?)$").unwrap());
static OPAMP_NUM_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^OA(\d+)").unwrap());
static ANALOG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^AN[A-Z]?\d+$").unwrap());

#[derive(Debug, Clone)]
struct EffectivePortConfig {
    peripheral: String,
    direction: String,
}

pub(crate) fn collect_opamp_numbers(assignments: &[PinAssignment]) -> BTreeSet<u32> {
    let mut opamp_nums = BTreeSet::new();
    for assign in assignments.iter().filter(|assign| assign.fixed) {
        if OPAMP_RE.is_match(&assign.peripheral) {
            if let Some(caps) = OPAMP_NUM_RE.captures(&assign.peripheral) {
                if let Ok(num) = caps.get(1).unwrap().as_str().parse::<u32>() {
                    opamp_nums.insert(num);
                }
            }
        }
    }
    opamp_nums
}

pub(crate) fn append_configure_ports_function(
    c_lines: &mut Vec<String>,
    device: &DeviceData,
    assignments: &[PinAssignment],
    digital_pins: &[u32],
    resolved: &[ResolvedPin],
) {
    let pin_by_pos: HashMap<u32, &ResolvedPin> =
        resolved.iter().map(|pin| (pin.position, pin)).collect();

    // Port configuration runs after PPS so remappable functions are bound before
    // the pins are driven or sampled.
    push_section_comment(
        c_lines,
        "configure_ports",
        &[
            "",
            "Configures ANSELx (analog/digital), and TRISx (direction) registers",
            "for all assigned pins.",
        ],
    );
    c_lines.push("void configure_ports(void)".into());
    c_lines.push("{".into());

    // Build one effective configuration per physical port bit. ICSP/debug pins are
    // split out because firmware should not fight the debugger for ownership.
    let mut port_config: BTreeMap<(String, u32), EffectivePortConfig> = BTreeMap::new();
    let mut icsp_pins: Vec<(String, u32, String)> = Vec::new();

    for assign in assignments {
        if let Some(pin) = pin_by_pos.get(&assign.pin_position) {
            let Some(port) = pin.port.as_ref() else {
                continue;
            };
            let bit = pin.port_bit.unwrap_or(0);

            if ICSP_RE.is_match(&assign.peripheral) {
                icsp_pins.push((port.clone(), bit, assign.peripheral.clone()));
                continue;
            }

            port_config.insert(
                (port.clone(), bit),
                EffectivePortConfig {
                    peripheral: assign.peripheral.clone(),
                    direction: assign.direction.clone(),
                },
            );
        }
    }

    for pos in digital_pins {
        if let Some(pin) = pin_by_pos.get(pos) {
            if let (Some(port), Some(bit)) = (&pin.port, pin.port_bit) {
                // An explicit digital override should still clear ANSEL even when
                // no peripheral assignment exists for that position.
                port_config
                    .entry((port.clone(), bit))
                    .or_insert_with(|| EffectivePortConfig {
                        peripheral: "GPIO".to_string(),
                        direction: "in".to_string(),
                    });
            }
        }
    }

    if !icsp_pins.is_empty() {
        c_lines.push(
            "    /* ICSP/debug pins — directly controlled by the debug module (FICD.ICS) */".into(),
        );
        icsp_pins.sort();
        for (port, bit, peripheral) in &icsp_pins {
            c_lines.push(format!(
                "    /* R{}{} reserved for {} — no ANSEL/TRIS configuration needed */",
                port, bit, peripheral
            ));
        }
        c_lines.push(String::new());
    }

    if !port_config.is_empty() {
        let mut analog_pins: BTreeSet<(String, u32)> = BTreeSet::new();
        let mut digital_pin_keys: BTreeSet<(String, u32)> = BTreeSet::new();

        for (key, entry) in &port_config {
            // In generated code, explicit analog functions keep ANSEL enabled and
            // everything else defaults to digital behavior.
            if ANALOG_RE.is_match(&entry.peripheral) {
                analog_pins.insert(key.clone());
            } else {
                digital_pin_keys.insert(key.clone());
            }
        }

        let has_ansel_bit = |port: &str, bit: u32| -> bool {
            device
                .ansel_bits
                .get(port)
                .map(|bits| bits.contains(&bit))
                .unwrap_or(false)
        };

        if !digital_pin_keys.is_empty() {
            c_lines.push(
                "    /* Disable analog function on digital pins (0 = digital mode) */".into(),
            );
            for (port, bit) in &digital_pin_keys {
                if has_ansel_bit(port, *bit) {
                    c_lines.push(format!("    ANSEL{}bits.ANSEL{}{} = 0U;", port, port, bit));
                }
            }
            c_lines.push(String::new());
        }

        if !analog_pins.is_empty() {
            c_lines
                .push("    /* Enable analog function on analog pins (1 = analog mode) */".into());
            for (port, bit) in &analog_pins {
                if has_ansel_bit(port, *bit) {
                    c_lines.push(format!("    ANSEL{}bits.ANSEL{}{} = 1U;", port, port, bit));
                }
            }
            c_lines.push(String::new());
        }

        // TRIS direction registers
        c_lines.push("    /* Configure pin direction: TRISx (0 = output, 1 = input) */".into());
        let mut tris_lines = Vec::new();
        for ((port, bit), entry) in &port_config {
            let tris_reg = format!("TRIS{}", port);
            if !device.port_registers.contains_key(&tris_reg) {
                continue;
            }

            match entry.direction.as_str() {
                "out" => {
                    tris_lines.push(format!(
                        "    {}bits.TRIS{}{} = 0U;  /* {} ({}) */",
                        tris_reg, port, bit, entry.peripheral, entry.direction
                    ));
                }
                "io" => {
                    tris_lines.push(format!(
                        "    {}bits.TRIS{}{} = 1U;  /* R{}{} = in/out (modify direction as needed) */",
                        tris_reg, port, bit, port, bit
                    ));
                }
                _ => {
                    tris_lines.push(format!(
                        "    {}bits.TRIS{}{} = 1U;  /* {} ({}) */",
                        tris_reg, port, bit, entry.peripheral, entry.direction
                    ));
                }
            }
        }
        c_lines.extend(align_comments(&tris_lines));
    }

    c_lines.push("}".into());
    c_lines.push(String::new());
}

pub(crate) fn append_configure_analog_function(
    c_lines: &mut Vec<String>,
    device: &DeviceData,
    opamp_nums: &BTreeSet<u32>,
) {
    if opamp_nums.is_empty() {
        return;
    }

    push_section_comment(
        c_lines,
        "configure_analog",
        &[
            "",
            "Enables on-chip op-amp modules. Gain and mode settings should be",
            "configured separately according to the application requirements.",
        ],
    );
    c_lines.push("void configure_analog(void)".into());
    c_lines.push("{".into());
    let mut opamp_lines = Vec::new();
    let profile = PartProfile::from_part_number(&device.part_number);
    let dspic33ak = profile.is_dspic33ak();
    for oa_num in opamp_nums {
        if dspic33ak {
            opamp_lines.push(format!(
                "    AMP{}CON1bits.AMPEN = 1U;  /* Enable Op-Amp {} */",
                oa_num, oa_num
            ));
        } else {
            opamp_lines.push(format!(
                "    AMPCON1Lbits.AMPEN{} = 1U;  /* Enable Op-Amp {} */",
                oa_num, oa_num
            ));
        }
    }
    c_lines.extend(align_comments(&opamp_lines));
    c_lines.push("}".into());
    c_lines.push(String::new());
}
