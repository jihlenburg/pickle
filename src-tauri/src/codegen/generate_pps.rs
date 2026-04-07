//! PPS code emission helpers for generated MCU init output.
//!
//! This module owns the `configure_pps()` function body so `generate.rs` can
//! stay focused on generation order and file assembly. The emitted text remains
//! byte-stable with the previous inline implementation because downstream tests
//! and compile-check fixtures rely on exact register writes and comments.

use std::collections::HashMap;

use crate::codegen::generate_support::{align_comments, push_section_comment};
use crate::codegen::generate_types::PinAssignment;
use crate::parser::edc_parser::{PPSInputMapping, PPSOutputMapping, ResolvedPin};

const PPS_UNLOCK: &str = "0x0000U";
const PPS_LOCK: &str = "0x0800U";

fn build_rp_port_labels(resolved: &[ResolvedPin]) -> HashMap<u32, String> {
    let mut labels = HashMap::new();
    for pin in resolved {
        if let (Some(rp_num), Some(port)) = (pin.rp_number, pin.port.as_deref()) {
            labels.insert(rp_num, format!("R{}{}", port, pin.port_bit.unwrap_or(0)));
        }
    }
    labels
}

fn port_label(rp_num: u32, labels: &HashMap<u32, String>) -> String {
    labels
        .get(&rp_num)
        .cloned()
        .unwrap_or_else(|| format!("RP{}", rp_num))
}

pub(crate) fn append_configure_pps_function(
    c_lines: &mut Vec<String>,
    assignments: &[PinAssignment],
    resolved: &[ResolvedPin],
    pps_input_mappings: &[PPSInputMapping],
    pps_output_mappings: &[PPSOutputMapping],
    signal_names: &HashMap<u32, String>,
) {
    let pps_in: Vec<_> = assignments
        .iter()
        .filter(|assign| !assign.fixed && assign.direction == "in")
        .collect();
    let pps_out: Vec<_> = assignments
        .iter()
        .filter(|assign| !assign.fixed && assign.direction == "out")
        .collect();

    if pps_in.is_empty() && pps_out.is_empty() {
        return;
    }

    let input_field_map: HashMap<&str, &PPSInputMapping> = pps_input_mappings
        .iter()
        .map(|mapping| (mapping.field_name.as_str(), mapping))
        .collect();
    let output_rp_map: HashMap<u32, &PPSOutputMapping> = pps_output_mappings
        .iter()
        .map(|mapping| (mapping.rp_number, mapping))
        .collect();
    let rp_port_labels = build_rp_port_labels(resolved);

    push_section_comment(
        c_lines,
        "configure_pps",
        &[
            "",
            "Configures Peripheral Pin Select (PPS) input and output mappings.",
            "The RPCON register is unlocked before writing and locked after.",
        ],
    );
    c_lines.push("void configure_pps(void)".into());
    c_lines.push("{".into());
    c_lines.push("    /* Unlock PPS registers (clear IOLOCK bit in RPCON) */".into());
    c_lines.push(format!("    __builtin_write_RPCON({});", PPS_UNLOCK));
    c_lines.push(String::new());

    if !pps_in.is_empty() {
        c_lines.push("    /* --- PPS Input Mappings ---".into());
        c_lines.push("     * Each RPINRx register field selects which RP pin drives".into());
        c_lines.push("     * the corresponding peripheral input. */".into());
        let mut pps_in_lines = Vec::new();
        for assign in &pps_in {
            let field_name = {
                // Device packs are inconsistent here: some use `U1RXR`, others `U1RX`.
                // Probe both spellings before declaring the input unmapped.
                let candidate1 = format!("{}R", assign.peripheral);
                if input_field_map.contains_key(candidate1.as_str()) {
                    Some(candidate1)
                } else if input_field_map.contains_key(assign.peripheral.as_str()) {
                    Some(assign.peripheral.clone())
                } else {
                    None
                }
            };
            let sig_label = signal_names
                .get(&assign.pin_position)
                .map(|name| format!(" [{}]", name))
                .unwrap_or_default();
            if let Some(field_name) = field_name {
                if let Some(mapping) = input_field_map.get(field_name.as_str()) {
                    let rp = assign.rp_number.unwrap_or(0);
                    let port = port_label(rp, &rp_port_labels);
                    pps_in_lines.push(format!(
                        "    {}bits.{} = {}U;  /* {} <- RP{}/{}{} */",
                        mapping.register,
                        mapping.field_name,
                        rp,
                        assign.peripheral,
                        rp,
                        port,
                        sig_label
                    ));
                }
            } else {
                pps_in_lines.push(format!(
                    "    /* WARNING: no RPINR mapping found for {} */",
                    assign.peripheral
                ));
            }
        }
        c_lines.extend(align_comments(&pps_in_lines));
        c_lines.push(String::new());
    }

    if !pps_out.is_empty() {
        c_lines.push("    /* --- PPS Output Mappings ---".into());
        c_lines.push("     * Each RPORx register field selects which peripheral output".into());
        c_lines.push("     * drives the corresponding RP pin. */".into());
        let mut pps_out_lines = Vec::new();
        for assign in &pps_out {
            let sig_label = signal_names
                .get(&assign.pin_position)
                .map(|name| format!(" [{}]", name))
                .unwrap_or_default();
            let rp = assign.rp_number.unwrap_or(0);
            if let Some(mapping) = output_rp_map.get(&rp) {
                if let Some(ppsval) = assign.ppsval {
                    let port = port_label(rp, &rp_port_labels);
                    pps_out_lines.push(format!(
                        "    {}bits.{} = {}U;  /* RP{}/{} -> {}{} */",
                        mapping.register,
                        mapping.field_name,
                        ppsval,
                        rp,
                        port,
                        assign.peripheral,
                        sig_label
                    ));
                } else {
                    pps_out_lines.push(format!(
                        "    /* WARNING: no RPOR mapping found for RP{} */",
                        rp
                    ));
                }
            } else {
                pps_out_lines.push(format!(
                    "    /* WARNING: no RPOR mapping found for RP{} */",
                    rp
                ));
            }
        }
        c_lines.extend(align_comments(&pps_out_lines));
        c_lines.push(String::new());
    }

    c_lines.push("    /* Lock PPS registers (set IOLOCK bit in RPCON) */".into());
    c_lines.push(format!("    __builtin_write_RPCON({});", PPS_LOCK));
    c_lines.push("}".into());
    c_lines.push(String::new());
}
