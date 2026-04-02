//! End-to-end integration test: load device fixture -> assign pins -> generate code -> verify output.

use pickle_lib::codegen::fuses::generate_dynamic_fuse_pragmas;
use pickle_lib::codegen::generate::{
    generate_c_files, generated_file_names, PinAssignment, PinConfig, DEFAULT_OUTPUT_BASENAME,
};
use pickle_lib::codegen::oscillator::OscConfig;
use pickle_lib::parser::edc_parser::{DcrField, DcrFieldValue, DcrRegister, DeviceData};
use std::collections::HashMap;

fn load_fixture() -> DeviceData {
    let fixture = include_str!("../../tests/fixtures/DSPIC33CK64MP102.json");
    DeviceData::from_json(fixture).expect("Failed to parse fixture JSON")
}

#[test]
fn test_load_device_fixture() {
    let device = load_fixture();
    assert_eq!(device.part_number.to_uppercase(), "DSPIC33CK64MP102");
    assert!(!device.pads.is_empty(), "Device should have pads");
    assert!(!device.pinouts.is_empty(), "Device should have pinouts");
    assert!(
        !device.default_pinout.is_empty(),
        "Device should have a default pinout"
    );
}

#[test]
fn test_resolve_pins_for_default_package() {
    let device = load_fixture();
    let pins = device.resolve_pins(None);
    assert!(!pins.is_empty(), "Should resolve pins for default package");

    // Every resolved pin should have a position and pad name
    for pin in &pins {
        assert!(pin.position > 0, "Pin position should be > 0");
        assert!(!pin.pad_name.is_empty(), "Pad name should not be empty");
    }
}

#[test]
fn test_resolve_pins_all_packages() {
    let device = load_fixture();
    for (pkg_name, pinout) in &device.pinouts {
        let pins = device.resolve_pins(Some(pkg_name));
        assert!(!pins.is_empty(), "Package {} should resolve pins", pkg_name);
        assert_eq!(
            pins.len(),
            pinout.pin_count as usize,
            "Package {} resolved pin count should match pinout.pin_count",
            pkg_name
        );
    }
}

#[test]
fn test_generate_code_with_uart_assignment() {
    let device = load_fixture();
    let filenames = generated_file_names(DEFAULT_OUTPUT_BASENAME);
    let pins = device.resolve_pins(None);

    // Find an RP pin to assign UART
    let rp_pin = pins
        .iter()
        .find(|p| {
            p.rp_number.is_some()
                && !p
                    .functions
                    .iter()
                    .any(|f| f.contains("PGC") || f.contains("PGD"))
        })
        .expect("Should have at least one non-ICSP RP pin");

    let config = PinConfig {
        part_number: device.part_number.clone(),
        assignments: vec![PinAssignment {
            pin_position: rp_pin.position,
            rp_number: rp_pin.rp_number,
            peripheral: "U1TX".to_string(),
            direction: "out".to_string(),
            ppsval: Some(1),
            fixed: false,
        }],
        digital_pins: vec![rp_pin.position],
    };

    let sig_names: HashMap<u32, String> = HashMap::new();
    let files = generate_c_files(&device, &config, None, Some(&sig_names), None, None, None);

    assert!(
        files.contains_key(&filenames.source),
        "Should produce .c file"
    );
    assert!(
        files.contains_key(&filenames.header),
        "Should produce .h file"
    );

    let c_code = &files[&filenames.source];
    let h_code = &files[&filenames.header];

    // PPS unlock/lock sequence
    assert!(
        c_code.contains("__builtin_write_RPCON(0x0000U)"),
        "Should unlock PPS"
    );
    assert!(
        c_code.contains("__builtin_write_RPCON(0x0800U)"),
        "Should lock PPS"
    );

    // U1TX output assignment
    assert!(c_code.contains("U1TX"), "Should contain U1TX assignment");
    assert!(c_code.contains("RPOR"), "Should write to RPOR register");

    // Header should have include guard and xc.h
    assert!(
        h_code.contains("#ifndef PIN_CONFIG_H"),
        "Header should have include guard"
    );
    assert!(
        h_code.contains("#include <xc.h>"),
        "Header should include xc.h"
    );
}

#[test]
fn test_generate_code_with_oscillator_and_fuses() {
    let device = load_fixture();
    let filenames = generated_file_names(DEFAULT_OUTPUT_BASENAME);

    let config = PinConfig {
        part_number: device.part_number.clone(),
        assignments: vec![],
        digital_pins: vec![],
    };

    let osc = OscConfig {
        source: "frc_pll".to_string(),
        target_fosc_hz: 200_000_000,
        crystal_hz: 0,
        poscmd: "EC".to_string(),
    };

    // Construct minimal fuse definitions for the test (fixture JSON lacks fuse_defs)
    let test_fuse_defs = vec![
        DcrRegister {
            cname: "FICD".into(),
            desc: "ICD Configuration".into(),
            addr: 0xAF28,
            default_value: 0xFF,
            fields: vec![
                DcrField {
                    cname: "ICS".into(),
                    desc: "ICD Channel".into(),
                    mask: 0x3,
                    width: 2,
                    hidden: false,
                    values: vec![
                        DcrFieldValue {
                            cname: "PGD1".into(),
                            desc: "PGC1/PGD1".into(),
                            value: 3,
                        },
                        DcrFieldValue {
                            cname: "PGD2".into(),
                            desc: "PGC2/PGD2".into(),
                            value: 2,
                        },
                    ],
                },
                DcrField {
                    cname: "JTAGEN".into(),
                    desc: "JTAG Enable".into(),
                    mask: 0x1,
                    width: 1,
                    hidden: false,
                    values: vec![
                        DcrFieldValue {
                            cname: "OFF".into(),
                            desc: "Disabled".into(),
                            value: 0,
                        },
                        DcrFieldValue {
                            cname: "ON".into(),
                            desc: "Enabled".into(),
                            value: 1,
                        },
                    ],
                },
            ],
        },
        DcrRegister {
            cname: "FWDT".into(),
            desc: "Watchdog Timer".into(),
            addr: 0xAF20,
            default_value: 0xFF,
            fields: vec![DcrField {
                cname: "FWDTEN".into(),
                desc: "WDT Enable".into(),
                mask: 0x1,
                width: 1,
                hidden: false,
                values: vec![
                    DcrFieldValue {
                        cname: "OFF".into(),
                        desc: "Disabled".into(),
                        value: 0,
                    },
                    DcrFieldValue {
                        cname: "ON".into(),
                        desc: "Enabled".into(),
                        value: 1,
                    },
                ],
            }],
        },
    ];
    let fuse_pragmas = generate_dynamic_fuse_pragmas(&test_fuse_defs, &HashMap::new());

    let sig_names: HashMap<u32, String> = HashMap::new();
    let files = generate_c_files(
        &device,
        &config,
        None,
        Some(&sig_names),
        Some(&osc),
        Some(fuse_pragmas.as_str()),
        None,
    );

    let c_code = &files[&filenames.source];

    // Oscillator pragmas
    assert!(
        c_code.contains("#pragma config FNOSC"),
        "Should have oscillator pragma"
    );
    assert!(
        c_code.contains("configure_oscillator"),
        "Should have oscillator init function"
    );

    // Fuse pragmas
    assert!(
        c_code.contains("#pragma config ICS"),
        "Should have ICSP fuse"
    );
    assert!(
        c_code.contains("#pragma config FWDTEN"),
        "Should have watchdog fuse"
    );
}

#[test]
fn test_device_json_roundtrip_preserves_data() {
    let device = load_fixture();
    let json = device.to_json();
    let restored = DeviceData::from_json(&json).expect("Should parse serialized JSON");

    assert_eq!(device.part_number, restored.part_number);
    assert_eq!(device.pads.len(), restored.pads.len());
    assert_eq!(device.pinouts.len(), restored.pinouts.len());
    assert_eq!(device.default_pinout, restored.default_pinout);
    assert_eq!(
        device.remappable_inputs.len(),
        restored.remappable_inputs.len()
    );
    assert_eq!(
        device.remappable_outputs.len(),
        restored.remappable_outputs.len()
    );
}

#[test]
fn test_signal_name_macros() {
    let device = load_fixture();
    let filenames = generated_file_names(DEFAULT_OUTPUT_BASENAME);
    let pins = device.resolve_pins(None);

    let rp_pin = pins
        .iter()
        .find(|p| p.rp_number.is_some() && p.port.is_some())
        .expect("Need an RP pin with port info");

    let config = PinConfig {
        part_number: device.part_number.clone(),
        assignments: vec![PinAssignment {
            pin_position: rp_pin.position,
            rp_number: rp_pin.rp_number,
            peripheral: "U1TX".to_string(),
            direction: "out".to_string(),
            ppsval: Some(1),
            fixed: false,
        }],
        digital_pins: vec![rp_pin.position],
    };

    let mut sig_names: HashMap<u32, String> = HashMap::new();
    sig_names.insert(rp_pin.position, "DEBUG_TX".to_string());

    let files = generate_c_files(&device, &config, None, Some(&sig_names), None, None, None);
    let h_code = &files[&filenames.header];

    assert!(
        h_code.contains("DEBUG_TX_PORT"),
        "Should have signal PORT macro"
    );
    assert!(
        h_code.contains("DEBUG_TX_LAT"),
        "Should have signal LAT macro"
    );
    assert!(
        h_code.contains("DEBUG_TX_TRIS"),
        "Should have signal TRIS macro"
    );
}

#[test]
fn test_signal_name_macros_sanitize_non_identifier_characters() {
    let device = load_fixture();
    let filenames = generated_file_names(DEFAULT_OUTPUT_BASENAME);
    let pins = device.resolve_pins(None);

    let rp_pin = pins
        .iter()
        .find(|p| p.rp_number.is_some() && p.port.is_some())
        .expect("Need an RP pin with port info");

    let config = PinConfig {
        part_number: device.part_number.clone(),
        assignments: vec![PinAssignment {
            pin_position: rp_pin.position,
            rp_number: rp_pin.rp_number,
            peripheral: "U1TX".to_string(),
            direction: "out".to_string(),
            ppsval: Some(1),
            fixed: false,
        }],
        digital_pins: vec![rp_pin.position],
    };

    let mut sig_names: HashMap<u32, String> = HashMap::new();
    sig_names.insert(rp_pin.position, "Debug TX/UART-1".to_string());

    let files = generate_c_files(&device, &config, None, Some(&sig_names), None, None, None);
    let h_code = &files[&filenames.header];

    assert!(h_code.contains("DEBUG_TX_UART_1_PORT"));
    assert!(h_code.contains("DEBUG_TX_UART_1_LAT"));
    assert!(h_code.contains("DEBUG_TX_UART_1_TRIS"));
}

#[test]
fn test_explicit_digital_pin_override_clears_ansel_for_fixture_pin() {
    let device = load_fixture();
    let filenames = generated_file_names(DEFAULT_OUTPUT_BASENAME);
    let analog_pin = device
        .resolve_pins(None)
        .into_iter()
        .find(|pin| pin.port.is_some() && pin.port_bit.is_some() && !pin.analog_channels.is_empty())
        .expect("fixture should contain an analog-capable GPIO pin");

    let port = analog_pin.port.expect("analog pin should have a port");
    let bit = analog_pin.port_bit.expect("analog pin should have a bit");
    let config = PinConfig {
        part_number: device.part_number.clone(),
        assignments: vec![],
        digital_pins: vec![analog_pin.position],
    };

    let files = generate_c_files(&device, &config, None, None, None, None, None);
    let c_code = &files[&filenames.source];
    let ansel_write = format!("ANSEL{port}bits.ANSEL{port}{bit} = 0U;");

    assert!(
        c_code.contains(&ansel_write),
        "explicit digital selection should clear the ANSEL bit for {port}{bit}"
    );
}
