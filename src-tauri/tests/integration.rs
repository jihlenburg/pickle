//! End-to-end integration test: load device fixture -> assign pins -> generate code -> verify output.

use pickle_lib::codegen::fuses::FuseConfig;
use pickle_lib::codegen::generate::{generate_c_files, PinAssignment, PinConfig};
use pickle_lib::codegen::oscillator::OscConfig;
use pickle_lib::parser::edc_parser::DeviceData;
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
    let files = generate_c_files(&device, &config, None, Some(&sig_names), None, None);

    assert!(files.contains_key("pin_config.c"), "Should produce .c file");
    assert!(files.contains_key("pin_config.h"), "Should produce .h file");

    let c_code = &files["pin_config.c"];
    let h_code = &files["pin_config.h"];

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

    let fuse = FuseConfig::default();

    let sig_names: HashMap<u32, String> = HashMap::new();
    let files = generate_c_files(
        &device,
        &config,
        None,
        Some(&sig_names),
        Some(&osc),
        Some(&fuse),
    );

    let c_code = &files["pin_config.c"];

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

    let files = generate_c_files(&device, &config, None, Some(&sig_names), None, None);
    let h_code = &files["pin_config.h"];

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
