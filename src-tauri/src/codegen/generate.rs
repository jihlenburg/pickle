//! C code generator for dsPIC33 and PIC24 PPS pin configuration.
//!
//! Generates C and header outputs for PPS-remappable and fixed-function
//! pin assignments. Outputs MISRA C:2012 compliant C99 code.
//!
//! Generation is intentionally phase-ordered:
//! 1. optional oscillator/fuse pragmas
//! 2. PPS configuration
//! 3. ANSEL/TRIS port setup
//! 4. optional analog peripheral enables
//! 5. `system_init()` calling those routines in hardware-safe order

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::codegen::oscillator::{generate_osc_code, managed_config_fields, OscConfig};
use crate::parser::edc_parser::DeviceData;

/// Regex for identifying ICSP/debug pins
const ICSP_PATTERN: &str = r"^MCLR$|^PGC\d$|^PGD\d$|^PGEC\d$|^PGED\d$";

const PPS_UNLOCK: &str = "0x0000U";
const PPS_LOCK: &str = "0x0800U";
const COMMENT_COL: usize = 40;
pub const DEFAULT_OUTPUT_BASENAME: &str = "mcu_init";

static ICSP_RE: Lazy<Regex> = Lazy::new(|| Regex::new(ICSP_PATTERN).unwrap());
static OPAMP_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^OA(\d+)(OUT|IN[+-]?)$").unwrap());
static OPAMP_NUM_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^OA(\d+)").unwrap());
static ANALOG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^AN[A-Z]?\d+$").unwrap());
static SAFE_SIGNAL_NAME_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^A-Za-z0-9_]").unwrap());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFileNames {
    pub basename: String,
    pub source: String,
    pub header: String,
}

pub fn generated_file_names(output_basename: &str) -> GeneratedFileNames {
    let basename = if output_basename.trim().is_empty() {
        DEFAULT_OUTPUT_BASENAME.to_string()
    } else {
        output_basename.trim().to_string()
    };

    GeneratedFileNames {
        source: format!("{basename}.c"),
        header: format!("{basename}.h"),
        basename,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GenerateOutputOptions<'a> {
    pub package: Option<&'a str>,
    pub output_basename: &'a str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinAssignment {
    pub pin_position: u32,
    pub rp_number: Option<u32>,
    #[serde(default)]
    pub peripheral: String,
    #[serde(default = "default_direction")]
    pub direction: String,
    pub ppsval: Option<u32>,
    #[serde(default)]
    pub fixed: bool,
}

fn default_direction() -> String {
    "in".to_string()
}

fn push_section_comment(lines: &mut Vec<String>, title: &str, body: &[&str]) {
    lines.push(
        "/* ---------------------------------------------------------------------------".into(),
    );
    lines.push(format!(" * {}", title));
    for line in body {
        lines.push(format!(" * {}", line));
    }
    lines.push(
        " * -------------------------------------------------------------------------*/".into(),
    );
}

fn extend_aligned_sections(lines: &mut Vec<String>, text: &str) {
    let mut sections: Vec<Vec<String>> = vec![vec![]];
    for line in text.lines() {
        if line.is_empty() {
            sections.push(vec![]);
        } else {
            sections
                .last_mut()
                .expect("sections always contains at least one block")
                .push(line.to_string());
        }
    }

    for (index, section) in sections.iter().enumerate() {
        if !section.is_empty() {
            lines.extend(align_comments(section));
        }
        if index + 1 < sections.len() {
            lines.push(String::new());
        }
    }
}

fn pragma_config_field(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("#pragma config ")?;
    let (field, _) = rest.split_once('=')?;
    Some(field.trim())
}

fn filter_fuse_pragmas_for_oscillator(
    fuse_text: &str,
    osc_config: Option<&OscConfig>,
) -> Option<String> {
    let trimmed = fuse_text.trim();
    if trimmed.is_empty() {
        return None;
    }

    let Some(osc) = osc_config else {
        return Some(trimmed.to_string());
    };

    let excluded_fields = managed_config_fields(osc);
    if excluded_fields.is_empty() {
        return Some(trimmed.to_string());
    }

    let kept_sections: Vec<String> = trimmed
        .split("\n\n")
        .filter_map(|section| {
            let mut kept_lines = Vec::new();
            let mut kept_pragmas = 0usize;

            for line in section.lines() {
                if let Some(field) = pragma_config_field(line) {
                    if excluded_fields.contains(field) {
                        continue;
                    }
                    kept_pragmas += 1;
                }
                kept_lines.push(line.to_string());
            }

            if kept_pragmas == 0 {
                None
            } else {
                Some(kept_lines.join("\n"))
            }
        })
        .collect();

    if kept_sections.is_empty() {
        None
    } else {
        Some(kept_sections.join("\n\n"))
    }
}

fn sanitize_signal_name(name: &str) -> String {
    SAFE_SIGNAL_NAME_RE.replace_all(name, "_").to_uppercase()
}

#[derive(Debug, Clone, Copy)]
struct ClcRegisterValues {
    conl: u16,
    conl_enable: u16,
    conh: u16,
    sel: u16,
    glsl: u16,
    glsh: u16,
}

fn compute_clc_register_values(config: &ClcModuleConfig) -> ClcRegisterValues {
    let mut conl = (config.mode & 0x7) as u16;
    if config.lcpol {
        conl |= 1 << 5;
    }
    if config.lcoe {
        conl |= 1 << 7;
    }
    if config.intn {
        conl |= 1 << 10;
    }
    if config.intp {
        conl |= 1 << 11;
    }

    let mut conh = 0;
    for gate in 0..4 {
        if config.gpol[gate] {
            conh |= 1 << gate;
        }
    }

    let sel = (config.ds[0] as u16 & 0x7)
        | ((config.ds[1] as u16 & 0x7) << 4)
        | ((config.ds[2] as u16 & 0x7) << 8)
        | ((config.ds[3] as u16 & 0x7) << 12);

    let mut glsl = 0;
    for bit in 0..8 {
        if config.gates[0][bit] {
            glsl |= 1 << bit;
        }
        if config.gates[1][bit] {
            glsl |= 1 << (bit + 8);
        }
    }

    let mut glsh = 0;
    for bit in 0..8 {
        if config.gates[2][bit] {
            glsh |= 1 << bit;
        }
        if config.gates[3][bit] {
            glsh |= 1 << (bit + 8);
        }
    }

    ClcRegisterValues {
        conl,
        conl_enable: conl | (1 << 15),
        conh,
        sel,
        glsl,
        glsh,
    }
}

/// Configuration for a single CLC module (CLC1-4).
/// Field values map directly to register bits as documented in DS70005298A.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClcModuleConfig {
    /// Data Selection MUX values (DS1-DS4), 3 bits each
    pub ds: [u8; 4],
    /// Gate source enable bits: gates[gate_idx][bit_idx]
    /// Bit order per gate: D1T, D1N, D2T, D2N, D3T, D3N, D4T, D4N
    pub gates: [[bool; 8]; 4],
    /// Gate polarity inversion (G1POL-G4POL)
    pub gpol: [bool; 4],
    /// Logic function mode (MODE<2:0>, 0-7)
    pub mode: u8,
    /// Output polarity inversion (LCPOL)
    pub lcpol: bool,
    /// Output enable to pin (LCOE)
    pub lcoe: bool,
    /// Module enable (LCEN)
    pub lcen: bool,
    /// Interrupt on positive edge (INTP)
    pub intp: bool,
    /// Interrupt on negative edge (INTN)
    pub intn: bool,
}

/// CLC mode names for generated comments
const CLC_MODE_NAMES: [&str; 8] = [
    "AND-OR",
    "OR-XOR",
    "4-input AND",
    "S-R Latch",
    "1-input D flip-flop with S/R",
    "2-input D flip-flop with R",
    "J-K flip-flop with R",
    "Transparent latch with S/R",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinConfig {
    pub part_number: String,
    #[serde(default)]
    pub assignments: Vec<PinAssignment>,
    #[serde(default)]
    pub digital_pins: Vec<u32>,
}

/// Align inline comments in a block of C statements to a consistent column.
fn align_comments(lines: &[String]) -> Vec<String> {
    let mut parsed: Vec<(String, Option<String>)> = Vec::new();

    for line in lines {
        let stripped = line.trim_start();
        if stripped.starts_with("/*") || stripped.starts_with('*') || !line.contains("/*") {
            parsed.push((line.clone(), None));
            continue;
        }
        if let Some(idx) = line.find("/*") {
            let code_part = line[..idx].trim_end().to_string();
            let comment_part = line[idx..].to_string();
            parsed.push((code_part, Some(comment_part)));
        } else {
            parsed.push((line.clone(), None));
        }
    }

    let mut max_code_len = COMMENT_COL;
    for (code, comment) in &parsed {
        if comment.is_some() {
            max_code_len = max_code_len.max(code.len() + 2);
        }
    }

    parsed
        .into_iter()
        .map(|(code, comment)| {
            if let Some(c) = comment {
                let padding = max_code_len.saturating_sub(code.len());
                format!("{}{}{}", code, " ".repeat(padding), c)
            } else {
                code
            }
        })
        .collect()
}

pub fn generate_c_files(
    device: &DeviceData,
    config: &PinConfig,
    package: Option<&str>,
    signal_names: Option<&HashMap<u32, String>>,
    osc_config: Option<&OscConfig>,
    fuse_pragmas: Option<&str>,
    clc_modules: Option<&HashMap<u32, ClcModuleConfig>>,
) -> HashMap<String, String> {
    generate_c_files_named(
        device,
        config,
        signal_names,
        osc_config,
        fuse_pragmas,
        clc_modules,
        GenerateOutputOptions {
            package,
            output_basename: DEFAULT_OUTPUT_BASENAME,
        },
    )
}

pub fn generate_c_files_named(
    device: &DeviceData,
    config: &PinConfig,
    signal_names: Option<&HashMap<u32, String>>,
    osc_config: Option<&OscConfig>,
    fuse_pragmas: Option<&str>,
    clc_modules: Option<&HashMap<u32, ClcModuleConfig>>,
    output: GenerateOutputOptions<'_>,
) -> HashMap<String, String> {
    let filenames = generated_file_names(output.output_basename);
    let pinout = device.get_pinout(output.package);
    let resolved = device.resolve_pins(output.package);
    let pin_by_pos: HashMap<u32, _> = resolved.iter().map(|p| (p.position, p)).collect();
    let empty_sig: HashMap<u32, String> = HashMap::new();
    let sig = signal_names.unwrap_or(&empty_sig);

    // PPS and fixed-function assignments feed different generated sections.
    let pps_assignments: Vec<_> = config.assignments.iter().filter(|a| !a.fixed).collect();
    let fixed_assignments: Vec<_> = config.assignments.iter().filter(|a| a.fixed).collect();

    let input_field_map: HashMap<&str, &_> = device
        .pps_input_mappings
        .iter()
        .map(|m| (m.field_name.as_str(), m))
        .collect();
    let output_rp_map: HashMap<u32, &_> = device
        .pps_output_mappings
        .iter()
        .map(|m| (m.rp_number, m))
        .collect();

    // Prefer a human-readable port/bit label in generated comments, but fall
    // back to the raw RP number for pads that lack parsed port metadata.
    let port_label = |rp_num: u32| -> String {
        for p in &resolved {
            if p.rp_number == Some(rp_num) {
                if let Some(port) = p.port.as_deref() {
                    return format!("R{}{}", port, p.port_bit.unwrap_or(0));
                }
            }
        }
        format!("RP{}", rp_num)
    };

    // Emit helper functions only when a matching feature is actually used.
    let has_pps = pps_assignments
        .iter()
        .any(|a| a.direction == "in" || a.direction == "out");
    let has_opamp = fixed_assignments
        .iter()
        .any(|a| OPAMP_RE.is_match(&a.peripheral));
    let has_clc = clc_modules.is_some_and(|m| !m.is_empty());

    let (osc_pragmas, osc_init) = if let Some(osc) = osc_config {
        generate_osc_code(osc)
    } else {
        (String::new(), String::new())
    };
    let filtered_fuse_pragmas =
        fuse_pragmas.and_then(|text| filter_fuse_pragmas_for_oscillator(text, osc_config));

    // =========================================================================
    // Generate the configured header output.
    // =========================================================================
    let mut h_lines = Vec::new();
    h_lines.push("/**".into());
    h_lines.push(format!(" * @file   {}", filenames.header));
    h_lines.push(format!(
        " * @brief  Pin configuration header for {} ({})",
        device.part_number, pinout.package
    ));
    h_lines.push(" *".into());
    h_lines.push(" * @note   Generated by pickle. MISRA C:2012 compliant.".into());
    h_lines.push(" */".into());
    h_lines.push(String::new());
    h_lines.push("#ifndef PIN_CONFIG_H".into());
    h_lines.push("#define PIN_CONFIG_H".into());
    h_lines.push(String::new());
    h_lines.push("#include <xc.h>".into());
    h_lines.push(String::new());

    // Signal name defines
    if !sig.is_empty() {
        push_section_comment(
            &mut h_lines,
            "Signal name aliases",
            &["Maps user-defined signal names to PORT/LAT/TRIS bit-fields."],
        );
        for assign in &config.assignments {
            if let Some(name) = sig.get(&assign.pin_position) {
                if name.is_empty() {
                    continue;
                }
                if let Some(pin) = pin_by_pos.get(&assign.pin_position) {
                    if let (Some(port), Some(bit)) = (&pin.port, pin.port_bit) {
                        let safe_name = sanitize_signal_name(name);
                        h_lines.push(format!(
                            "#define {}_PORT  (PORT{}bits.R{}{})",
                            safe_name, port, port, bit
                        ));
                        h_lines.push(format!(
                            "#define {}_LAT   (LAT{}bits.LAT{}{})",
                            safe_name, port, port, bit
                        ));
                        h_lines.push(format!(
                            "#define {}_TRIS  (TRIS{}bits.TRIS{}{})",
                            safe_name, port, port, bit
                        ));
                    }
                }
            }
        }
        h_lines.push(String::new());
    }

    // Function prototypes
    h_lines.push("/* Function prototypes */".into());
    if !osc_init.is_empty() {
        h_lines.push("void configure_oscillator(void);".into());
    }
    if has_pps {
        h_lines.push("void configure_pps(void);".into());
    }
    h_lines.push("void configure_ports(void);".into());
    if has_opamp {
        h_lines.push("void configure_analog(void);".into());
    }
    if has_clc {
        h_lines.push("void configure_clc(void);".into());
    }
    h_lines.push("void system_init(void);".into());
    h_lines.push(String::new());
    h_lines.push("#endif /* PIN_CONFIG_H */".into());
    h_lines.push(String::new());

    // =========================================================================
    // Generate the configured C output.
    // =========================================================================
    let mut c_lines: Vec<String> = Vec::new();

    c_lines.push("/**".into());
    c_lines.push(format!(" * @file   {}", filenames.source));
    c_lines.push(format!(
        " * @brief  Pin configuration for {} ({})",
        device.part_number, pinout.package
    ));
    c_lines.push(" *".into());
    c_lines.push(" * Configures PPS remappable pin mappings, analog/digital selection,".into());
    c_lines.push(" * and pin direction (TRIS) registers.".into());
    c_lines.push(" *".into());
    c_lines.push(" * @note   Generated by pickle. MISRA C:2012 compliant.".into());
    c_lines.push(" */".into());
    c_lines.push(String::new());
    c_lines.push(format!("#include \"{}\"", filenames.header));
    c_lines.push(String::new());

    // Oscillator pragmas are emitted first because Microchip expects config
    // pragmas near the top of the translation unit.
    if !osc_pragmas.is_empty() {
        let aligned = align_comments(
            &osc_pragmas
                .lines()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
        );
        c_lines.extend(aligned);
        c_lines.push(String::new());
    }

    // Align fuse pragmas section-by-section so unrelated pragma groups do not
    // try to share one huge comment column.
    if let Some(fuse_text) = filtered_fuse_pragmas.as_deref() {
        if !fuse_text.is_empty() {
            extend_aligned_sections(&mut c_lines, fuse_text);
            c_lines.push(String::new());
        }
    }

    // Oscillator init function
    if !osc_init.is_empty() {
        c_lines.push(osc_init.clone());
    }

    // PPS configuration
    let pps_in: Vec<_> = pps_assignments
        .iter()
        .filter(|a| a.direction == "in")
        .collect();
    let pps_out: Vec<_> = pps_assignments
        .iter()
        .filter(|a| a.direction == "out")
        .collect();

    if !pps_in.is_empty() || !pps_out.is_empty() {
        push_section_comment(
            &mut c_lines,
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
                let sig_label = sig
                    .get(&assign.pin_position)
                    .map(|s| format!(" [{}]", s))
                    .unwrap_or_default();
                if let Some(fname) = field_name {
                    if let Some(m) = input_field_map.get(fname.as_str()) {
                        let rp = assign.rp_number.unwrap_or(0);
                        let pl = port_label(rp);
                        pps_in_lines.push(format!(
                            "    {}bits.{} = {}U;  /* {} <- RP{}/{}{} */",
                            m.register, m.field_name, rp, assign.peripheral, rp, pl, sig_label
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
                let sig_label = sig
                    .get(&assign.pin_position)
                    .map(|s| format!(" [{}]", s))
                    .unwrap_or_default();
                let rp = assign.rp_number.unwrap_or(0);
                if let Some(m) = output_rp_map.get(&rp) {
                    if let Some(ppsval) = assign.ppsval {
                        let pl = port_label(rp);
                        pps_out_lines.push(format!(
                            "    {}bits.{} = {}U;  /* RP{}/{} -> {}{} */",
                            m.register, m.field_name, ppsval, rp, pl, assign.peripheral, sig_label
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

    // Port configuration runs after PPS so remappable functions are bound before
    // the pins are driven or sampled.
    push_section_comment(
        &mut c_lines,
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
    let mut port_config: BTreeMap<(String, u32), (String, String, bool)> = BTreeMap::new();
    let mut icsp_pins: Vec<(String, u32, String)> = Vec::new();

    for assign in &config.assignments {
        if let Some(pin) = pin_by_pos.get(&assign.pin_position) {
            if pin.port.is_none() {
                continue;
            }
            let port = pin.port.as_ref().unwrap().clone();
            let bit = pin.port_bit.unwrap_or(0);

            if ICSP_RE.is_match(&assign.peripheral) {
                icsp_pins.push((port, bit, assign.peripheral.clone()));
                continue;
            }
            let key = (port, bit);
            port_config.insert(
                key,
                (
                    assign.peripheral.clone(),
                    assign.direction.clone(),
                    assign.fixed,
                ),
            );
        }
    }

    for pos in &config.digital_pins {
        if let Some(pin) = pin_by_pos.get(pos) {
            if let (Some(port), Some(bit)) = (&pin.port, pin.port_bit) {
                let key = (port.clone(), bit);
                // An explicit digital override should still clear ANSEL even when
                // no peripheral assignment exists for that position.
                port_config
                    .entry(key)
                    .or_insert_with(|| ("GPIO".to_string(), "in".to_string(), true));
            }
        }
    }

    if !icsp_pins.is_empty() {
        c_lines.push(
            "    /* ICSP/debug pins — directly controlled by the debug module (FICD.ICS) */".into(),
        );
        icsp_pins.sort();
        for (port, bit, periph) in &icsp_pins {
            c_lines.push(format!(
                "    /* R{}{} reserved for {} — no ANSEL/TRIS configuration needed */",
                port, bit, periph
            ));
        }
        c_lines.push(String::new());
    }

    if !port_config.is_empty() {
        let mut analog_pins: BTreeSet<(String, u32)> = BTreeSet::new();
        let mut digital_pins: BTreeSet<(String, u32)> = BTreeSet::new();

        for (key, (periph, _, _)) in &port_config {
            // In generated code, explicit analog functions keep ANSEL enabled and
            // everything else defaults to digital behavior.
            if ANALOG_RE.is_match(periph) {
                analog_pins.insert(key.clone());
            } else {
                digital_pins.insert(key.clone());
            }
        }

        let has_ansel_bit = |port: &str, bit: u32| -> bool {
            device
                .ansel_bits
                .get(port)
                .map(|bits| bits.contains(&bit))
                .unwrap_or(false)
        };

        if !digital_pins.is_empty() {
            c_lines.push(
                "    /* Disable analog function on digital pins (0 = digital mode) */".into(),
            );
            for (port, bit) in &digital_pins {
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
        for ((port, bit), (periph, direction, _)) in &port_config {
            let tris_reg = format!("TRIS{}", port);
            if !device.port_registers.contains_key(&tris_reg) {
                continue;
            }

            match direction.as_str() {
                "out" => {
                    tris_lines.push(format!(
                        "    {}bits.TRIS{}{} = 0U;  /* {} ({}) */",
                        tris_reg, port, bit, periph, direction
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
                        tris_reg, port, bit, periph, direction
                    ));
                }
            }
        }
        c_lines.extend(align_comments(&tris_lines));
    }

    c_lines.push("}".into());
    c_lines.push(String::new());

    // Op-amp enables
    let mut opamp_nums: BTreeSet<u32> = BTreeSet::new();
    for assign in &fixed_assignments {
        if OPAMP_RE.is_match(&assign.peripheral) {
            if let Some(caps) = OPAMP_NUM_RE.captures(&assign.peripheral) {
                if let Ok(num) = caps.get(1).unwrap().as_str().parse::<u32>() {
                    opamp_nums.insert(num);
                }
            }
        }
    }

    if !opamp_nums.is_empty() {
        push_section_comment(
            &mut c_lines,
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
        for oa_num in &opamp_nums {
            opamp_lines.push(format!(
                "    AMP{}CONbits.AMPEN = 1U;  /* Enable Op-Amp {} */",
                oa_num, oa_num
            ));
        }
        c_lines.extend(align_comments(&opamp_lines));
        c_lines.push("}".into());
        c_lines.push(String::new());
    }

    // configure_clc() — CLC module initialization
    if has_clc {
        if let Some(clc_mods) = clc_modules {
            push_section_comment(
                &mut c_lines,
                "configure_clc",
                &[
                    "",
                    "Configures the Configurable Logic Cell modules. Each module is disabled",
                    "before writing its configuration registers, then enabled last.",
                ],
            );
            c_lines.push("void configure_clc(void)".into());
            c_lines.push("{".into());

            let mut sorted_keys: Vec<_> = clc_mods.keys().collect();
            sorted_keys.sort();

            for (i, idx) in sorted_keys.iter().enumerate() {
                let mod_cfg = &clc_mods[idx];
                let n = idx;
                let registers = compute_clc_register_values(mod_cfg);
                let mode_name = CLC_MODE_NAMES
                    .get(mod_cfg.mode as usize)
                    .unwrap_or(&"Unknown");

                let mut clc_lines = Vec::new();
                clc_lines.push(format!("    /* CLC{} — {} */", n, mode_name));
                clc_lines.push(format!(
                    "    CLC{}CONL = 0x0000U; /* Disable module before configuration */",
                    n
                ));
                clc_lines.push(format!(
                    "    CLC{}SEL  = 0x{:04X}U; /* DS1={}, DS2={}, DS3={}, DS4={} */",
                    n, registers.sel, mod_cfg.ds[0], mod_cfg.ds[1], mod_cfg.ds[2], mod_cfg.ds[3]
                ));
                clc_lines.push(format!(
                    "    CLC{}GLSL = 0x{:04X}U; /* Gate 1-2 source enables */",
                    n, registers.glsl
                ));
                clc_lines.push(format!(
                    "    CLC{}GLSH = 0x{:04X}U; /* Gate 3-4 source enables */",
                    n, registers.glsh
                ));
                clc_lines.push(format!(
                    "    CLC{}CONH = 0x{:04X}U; /* Gate polarity */",
                    n, registers.conh
                ));
                if mod_cfg.lcen {
                    clc_lines.push(format!(
                        "    CLC{}CONL = 0x{:04X}U; /* Enable: MODE={}, LCOE={}, LCPOL={} */",
                        n,
                        registers.conl_enable,
                        mod_cfg.mode,
                        if mod_cfg.lcoe { "on" } else { "off" },
                        if mod_cfg.lcpol { "inv" } else { "norm" }
                    ));
                } else {
                    clc_lines.push(format!(
                        "    CLC{}CONL = 0x{:04X}U; /* Module disabled */",
                        n, registers.conl
                    ));
                }

                for line in align_comments(&clc_lines) {
                    c_lines.push(line);
                }
                if i + 1 < sorted_keys.len() {
                    c_lines.push(String::new());
                }
            }

            c_lines.push("}".into());
            c_lines.push(String::new());
        }
    }

    // system_init()
    push_section_comment(
        &mut c_lines,
        "system_init",
        &[
            "",
            "Master initialization function. Calls all configuration routines in the",
            "correct order: oscillator first (clock must be stable), then PPS (requires",
            "unlock/lock), then port direction/analog, then peripheral enables.",
        ],
    );
    c_lines.push("void system_init(void)".into());
    c_lines.push("{".into());
    if !osc_init.is_empty() {
        c_lines.push("    configure_oscillator();".into());
    }
    if has_pps {
        c_lines.push("    configure_pps();".into());
    }
    c_lines.push("    configure_ports();".into());
    if has_opamp {
        c_lines.push("    configure_analog();".into());
    }
    if has_clc {
        c_lines.push("    configure_clc();".into());
    }
    c_lines.push("}".into());
    c_lines.push(String::new());

    c_lines.push(format!("/* End of {} */", filenames.source));
    c_lines.push(String::new());

    let mut result = HashMap::new();
    result.insert(filenames.header, h_lines.join("\n"));
    result.insert(filenames.source, c_lines.join("\n"));
    result
}

/// Generate a single self-contained C file (backward-compatible for compile-check).
pub fn generate_c_code(
    device: &DeviceData,
    config: &PinConfig,
    package: Option<&str>,
    signal_names: Option<&HashMap<u32, String>>,
    osc_config: Option<&OscConfig>,
    fuse_pragmas: Option<&str>,
) -> String {
    generate_c_code_named(
        device,
        config,
        package,
        signal_names,
        osc_config,
        fuse_pragmas,
        DEFAULT_OUTPUT_BASENAME,
    )
}

pub fn generate_c_code_named(
    device: &DeviceData,
    config: &PinConfig,
    package: Option<&str>,
    signal_names: Option<&HashMap<u32, String>>,
    osc_config: Option<&OscConfig>,
    fuse_pragmas: Option<&str>,
    output_basename: &str,
) -> String {
    let filenames = generated_file_names(output_basename);
    let files = generate_c_files_named(
        device,
        config,
        signal_names,
        osc_config,
        fuse_pragmas,
        None,
        GenerateOutputOptions {
            package,
            output_basename,
        },
    );
    let h_content = &files[&filenames.header];
    let c_content = &files[&filenames.source];

    let mut defines = Vec::new();
    let mut in_defines = false;
    for line in h_content.lines() {
        if line.starts_with("#define ")
            && (line.contains("_PORT") || line.contains("_LAT") || line.contains("_TRIS"))
        {
            defines.push(line.to_string());
        }
        if line.starts_with("/* ---") && line.contains("Signal name") {
            in_defines = true;
        }
        if in_defines {
            defines.push(line.to_string());
            if line.is_empty() {
                in_defines = false;
            }
        }
    }

    let mut merged = c_content.replace(
        &format!("#include \"{}\"", filenames.header),
        "#include <xc.h>",
    );

    if !defines.is_empty() {
        // Family-specific compile-checks use a single translation unit, so inline the signal-name
        // macros that would normally come from the generated header.
        let define_block = defines.join("\n");
        merged = merged.replace(
            "#include <xc.h>\n",
            &format!("#include <xc.h>\n\n{}\n", define_block),
        );
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::edc_parser::*;
    use std::collections::HashMap;

    fn make_test_device() -> DeviceData {
        let mut pads = HashMap::new();
        pads.insert(
            "RB0".to_string(),
            Pad {
                name: "RB0".to_string(),
                functions: vec!["OSCI".into(), "AN5".into(), "RP32".into(), "RB0".into()],
                rp_number: Some(32),
                port: Some("B".to_string()),
                port_bit: Some(0),
                analog_channels: vec!["AN5".to_string()],
                is_power: false,
            },
        );
        pads.insert(
            "RB1".to_string(),
            Pad {
                name: "RB1".to_string(),
                functions: vec!["OSCO".into(), "AN6".into(), "RP33".into(), "RB1".into()],
                rp_number: Some(33),
                port: Some("B".to_string()),
                port_bit: Some(1),
                analog_channels: vec!["AN6".to_string()],
                is_power: false,
            },
        );
        pads.insert(
            "RB2".to_string(),
            Pad {
                name: "RB2".to_string(),
                functions: vec!["RP34".into(), "RB2".into()],
                rp_number: Some(34),
                port: Some("B".to_string()),
                port_bit: Some(2),
                analog_channels: vec![],
                is_power: false,
            },
        );
        pads.insert(
            "RB3".to_string(),
            Pad {
                name: "RB3".to_string(),
                functions: vec!["PGD1".into(), "AN8".into(), "RP35".into(), "RB3".into()],
                rp_number: Some(35),
                port: Some("B".to_string()),
                port_bit: Some(3),
                analog_channels: vec!["AN8".to_string()],
                is_power: false,
            },
        );
        pads.insert(
            "RB4".to_string(),
            Pad {
                name: "RB4".to_string(),
                functions: vec!["PGC1".into(), "RP36".into(), "RB4".into()],
                rp_number: Some(36),
                port: Some("B".to_string()),
                port_bit: Some(4),
                analog_channels: vec![],
                is_power: false,
            },
        );
        pads.insert(
            "VDD".to_string(),
            Pad {
                name: "VDD".to_string(),
                functions: vec!["VDD".into()],
                rp_number: None,
                port: None,
                port_bit: None,
                analog_channels: vec![],
                is_power: true,
            },
        );

        let mut pins = HashMap::new();
        pins.insert(1, "RB0".to_string());
        pins.insert(2, "RB1".to_string());
        pins.insert(3, "RB2".to_string());
        pins.insert(4, "RB3".to_string());
        pins.insert(5, "RB4".to_string());
        pins.insert(6, "VDD".to_string());

        let mut pinouts = HashMap::new();
        pinouts.insert(
            "6-pin TEST".to_string(),
            Pinout {
                package: "6-pin TEST".to_string(),
                pin_count: 6,
                source: "edc".to_string(),
                pins,
            },
        );

        let pps_input_mappings = vec![PPSInputMapping {
            peripheral: "U1RXR".to_string(),
            register: "RPINR18".to_string(),
            register_addr: 3368,
            field_name: "U1RXR".to_string(),
            field_mask: 255,
            field_offset: 0,
        }];

        let pps_output_mappings = vec![
            PPSOutputMapping {
                rp_number: 32,
                register: "RPOR0".to_string(),
                register_addr: 3456,
                field_name: "RP32R".to_string(),
                field_mask: 63,
                field_offset: 0,
            },
            PPSOutputMapping {
                rp_number: 33,
                register: "RPOR0".to_string(),
                register_addr: 3456,
                field_name: "RP33R".to_string(),
                field_mask: 63,
                field_offset: 8,
            },
            PPSOutputMapping {
                rp_number: 34,
                register: "RPOR1".to_string(),
                register_addr: 3458,
                field_name: "RP34R".to_string(),
                field_mask: 63,
                field_offset: 0,
            },
        ];

        let mut port_registers = HashMap::new();
        port_registers.insert("TRISB".to_string(), 3614);
        port_registers.insert("ANSELB".to_string(), 3612);

        let mut ansel_bits = HashMap::new();
        ansel_bits.insert("B".to_string(), vec![0, 1, 2, 3, 4]);

        DeviceData {
            part_number: "DSPIC33CK64MP102".to_string(),
            pads,
            pinouts,
            default_pinout: "6-pin TEST".to_string(),
            remappable_inputs: vec![RemappablePeripheral {
                name: "U1RX".to_string(),
                direction: "in".to_string(),
                ppsval: None,
            }],
            remappable_outputs: vec![RemappablePeripheral {
                name: "U1TX".to_string(),
                direction: "out".to_string(),
                ppsval: Some(1),
            }],
            pps_input_mappings,
            pps_output_mappings,
            port_registers,
            ansel_bits,
            fuse_defs: vec![],
            clc_module_id: None,
            clc_input_sources: None,
        }
    }

    #[test]
    fn test_generates_header_and_source() {
        let device = make_test_device();
        let filenames = generated_file_names(DEFAULT_OUTPUT_BASENAME);
        let config = PinConfig {
            part_number: "DSPIC33CK64MP102".to_string(),
            assignments: vec![PinAssignment {
                pin_position: 1,
                rp_number: Some(32),
                peripheral: "U1RX".to_string(),
                direction: "in".to_string(),
                ppsval: None,
                fixed: false,
            }],
            digital_pins: vec![],
        };
        let files = generate_c_files(&device, &config, None, None, None, None, None);
        assert!(files.contains_key(&filenames.header));
        assert!(files.contains_key(&filenames.source));
        assert!(files[&filenames.header].contains("#ifndef PIN_CONFIG_H"));
        assert!(files[&filenames.source].contains(&format!("#include \"{}\"", filenames.header)));
    }

    #[test]
    fn test_icsp_pins_no_ansel_tris() {
        let device = make_test_device();
        let config = PinConfig {
            part_number: "DSPIC33CK64MP102".to_string(),
            assignments: vec![
                PinAssignment {
                    pin_position: 4,
                    rp_number: Some(35),
                    peripheral: "PGD1".to_string(),
                    direction: "in".to_string(),
                    ppsval: None,
                    fixed: true,
                },
                PinAssignment {
                    pin_position: 5,
                    rp_number: Some(36),
                    peripheral: "PGC1".to_string(),
                    direction: "in".to_string(),
                    ppsval: None,
                    fixed: true,
                },
            ],
            digital_pins: vec![],
        };
        let files = generate_c_files(&device, &config, None, None, None, None, None);
        let c = &files[&generated_file_names(DEFAULT_OUTPUT_BASENAME).source];
        assert!(c.contains("reserved for PGD1"));
        assert!(c.contains("reserved for PGC1"));
        assert!(!c.contains("ANSELB3") || !c.contains("= 0U"));
    }

    #[test]
    fn test_system_init_order() {
        let device = make_test_device();
        let osc = OscConfig {
            source: "frc_pll".to_string(),
            target_fosc_hz: 200_000_000,
            crystal_hz: 0,
            poscmd: "EC".to_string(),
        };
        let config = PinConfig {
            part_number: "DSPIC33CK64MP102".to_string(),
            assignments: vec![PinAssignment {
                pin_position: 1,
                rp_number: Some(32),
                peripheral: "U1RX".to_string(),
                direction: "in".to_string(),
                ppsval: None,
                fixed: false,
            }],
            digital_pins: vec![],
        };
        let files = generate_c_files(&device, &config, None, None, Some(&osc), None, None);
        let c = &files[&generated_file_names(DEFAULT_OUTPUT_BASENAME).source];

        // system_init order: oscillator -> pps -> ports
        // Find positions within system_init function
        let sys_init_pos = c.find("void system_init(void)").unwrap();
        let osc_call = c[sys_init_pos..].find("configure_oscillator();").unwrap();
        let pps_call = c[sys_init_pos..].find("configure_pps();").unwrap();
        let ports_call = c[sys_init_pos..].find("configure_ports();").unwrap();

        assert!(osc_call < pps_call);
        assert!(pps_call < ports_call);
    }

    #[test]
    fn test_pps_unlock_lock() {
        let device = make_test_device();
        let config = PinConfig {
            part_number: "DSPIC33CK64MP102".to_string(),
            assignments: vec![PinAssignment {
                pin_position: 2,
                rp_number: Some(33),
                peripheral: "U1TX".to_string(),
                direction: "out".to_string(),
                ppsval: Some(1),
                fixed: false,
            }],
            digital_pins: vec![],
        };
        let files = generate_c_files(&device, &config, None, None, None, None, None);
        let c = &files[&generated_file_names(DEFAULT_OUTPUT_BASENAME).source];

        assert!(c.contains("0x0000U"));
        assert!(c.contains("0x0800U"));
        let unlock_pos = c.find("0x0000U").unwrap();
        let lock_pos = c.find("0x0800U").unwrap();
        assert!(unlock_pos < lock_pos);
    }

    #[test]
    fn test_explicit_digital_override_clears_ansel_for_analog_capable_pin() {
        let device = make_test_device();
        let config = PinConfig {
            part_number: "DSPIC33CK64MP102".to_string(),
            assignments: vec![],
            digital_pins: vec![1],
        };

        let files = generate_c_files(&device, &config, None, None, None, None, None);
        let c = &files[&generated_file_names(DEFAULT_OUTPUT_BASENAME).source];

        assert!(
            c.contains("ANSELBbits.ANSELB0 = 0U;"),
            "explicit digital pins should force ANSEL off on analog-capable pads"
        );
        assert!(
            c.contains("TRISBbits.TRISB0 = 1U;"),
            "unassigned digital pins should still default to input direction"
        );
    }

    #[test]
    fn test_signal_name_aliases_are_sanitized_for_c_macros() {
        let device = make_test_device();
        let config = PinConfig {
            part_number: "DSPIC33CK64MP102".to_string(),
            assignments: vec![PinAssignment {
                pin_position: 1,
                rp_number: Some(32),
                peripheral: "U1RX".to_string(),
                direction: "in".to_string(),
                ppsval: None,
                fixed: false,
            }],
            digital_pins: vec![],
        };

        let mut signal_names = HashMap::new();
        signal_names.insert(1, "Debug TX/UART-1".to_string());

        let files = generate_c_files(
            &device,
            &config,
            None,
            Some(&signal_names),
            None,
            None,
            None,
        );
        let h = &files[&generated_file_names(DEFAULT_OUTPUT_BASENAME).header];

        assert!(h.contains("DEBUG_TX_UART_1_PORT"));
        assert!(h.contains("DEBUG_TX_UART_1_LAT"));
        assert!(h.contains("DEBUG_TX_UART_1_TRIS"));
    }

    #[test]
    fn test_oscillator_owned_fuse_pragmas_are_filtered_from_dynamic_fuse_sections() {
        let device = make_test_device();
        let config = PinConfig {
            part_number: "DSPIC33CK64MP102".to_string(),
            assignments: vec![PinAssignment {
                pin_position: 1,
                rp_number: Some(32),
                peripheral: "U1RX".to_string(),
                direction: "in".to_string(),
                ppsval: None,
                fixed: false,
            }],
            digital_pins: vec![],
        };
        let osc = OscConfig {
            source: "frc_pll".to_string(),
            target_fosc_hz: 200_000_000,
            crystal_hz: 0,
            poscmd: "EC".to_string(),
        };
        let fuse_pragmas = r#"/* FOSC */
#pragma config FNOSC = FRCDIVN    /* conflicting oscillator source */
#pragma config IESO = ON          /* conflicting oscillator startup */
#pragma config POSCMD = NONE      /* conflicting primary oscillator mode */
#pragma config FCKSM = CSECME     /* conflicting clock switching policy */
#pragma config PLLKEN = OFF       /* conflicting PLL lock behavior */

/* FICD */
#pragma config ICS = PGD1         /* keep unrelated debug channel */"#;

        let files = generate_c_files(
            &device,
            &config,
            None,
            None,
            Some(&osc),
            Some(fuse_pragmas),
            None,
        );
        let c = &files[&generated_file_names(DEFAULT_OUTPUT_BASENAME).source];

        assert_eq!(c.matches("#pragma config FNOSC").count(), 1);
        assert_eq!(c.matches("#pragma config IESO").count(), 1);
        assert_eq!(c.matches("#pragma config POSCMD").count(), 1);
        assert_eq!(c.matches("#pragma config FCKSM").count(), 1);
        assert_eq!(c.matches("#pragma config PLLKEN").count(), 1);
        assert!(c.contains("#pragma config FNOSC = FRCPLL"));
        assert!(c.contains("#pragma config PLLKEN = ON"));
        assert!(!c.contains("#pragma config FNOSC = FRCDIVN"));
        assert!(!c.contains("#pragma config PLLKEN = OFF"));
        assert!(c.contains("#pragma config ICS = PGD1"));
    }

    #[test]
    fn test_fuse_only_generation_preserves_oscillator_fields_without_oscillator_config() {
        let device = make_test_device();
        let config = PinConfig {
            part_number: "DSPIC33CK64MP102".to_string(),
            assignments: vec![],
            digital_pins: vec![],
        };
        let fuse_pragmas = r#"/* FOSC */
#pragma config FNOSC = FRCDIVN    /* fuse-managed oscillator source */"#;

        let files = generate_c_files(&device, &config, None, None, None, Some(fuse_pragmas), None);
        let c = &files[&generated_file_names(DEFAULT_OUTPUT_BASENAME).source];

        assert_eq!(c.matches("#pragma config FNOSC").count(), 1);
        assert!(c.contains("#pragma config FNOSC = FRCDIVN"));
    }
}
