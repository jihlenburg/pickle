//! Unit tests for the public generator facade in `generate.rs`.
//!
//! Keeping the tests in a dedicated submodule keeps the production generator
//! file focused on runtime logic while still exercising the public API from the
//! module boundary.

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
            display_name: None,
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
        device_info: DeviceInfo::default(),
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

#[test]
fn test_opamp_enable_registers_follow_device_family() {
    let mut ck_device = make_test_device();
    ck_device.part_number = "DSPIC33CK64MP102".to_string();
    let ck_config = PinConfig {
        part_number: ck_device.part_number.clone(),
        assignments: vec![PinAssignment {
            pin_position: 1,
            rp_number: Some(32),
            peripheral: "OA1OUT".to_string(),
            direction: "out".to_string(),
            ppsval: None,
            fixed: true,
        }],
        digital_pins: vec![],
    };
    let ck_files = generate_c_files(&ck_device, &ck_config, None, None, None, None, None);
    let ck_c = &ck_files[&generated_file_names(DEFAULT_OUTPUT_BASENAME).source];
    assert!(ck_c.contains("AMPCON1Lbits.AMPEN1 = 1U;"));

    let mut ak_device = make_test_device();
    ak_device.part_number = "DSPIC33AK64MC105".to_string();
    let ak_config = PinConfig {
        part_number: ak_device.part_number.clone(),
        assignments: vec![PinAssignment {
            pin_position: 1,
            rp_number: Some(32),
            peripheral: "OA1OUT".to_string(),
            direction: "out".to_string(),
            ppsval: None,
            fixed: true,
        }],
        digital_pins: vec![],
    };
    let ak_files = generate_c_files(&ak_device, &ak_config, None, None, None, None, None);
    let ak_c = &ak_files[&generated_file_names(DEFAULT_OUTPUT_BASENAME).source];
    assert!(ak_c.contains("AMP1CON1bits.AMPEN = 1U;"));
}

#[test]
fn test_dspic33ak_clock_and_clc_generation_use_runtime_registers() {
    let mut device = make_test_device();
    device.part_number = "DSPIC33AK64MC105".to_string();
    let osc = OscConfig {
        source: "frc_pll".to_string(),
        target_fosc_hz: 200_000_000,
        crystal_hz: 0,
        poscmd: "EC".to_string(),
    };
    let clc_modules = HashMap::from([(
        1_u32,
        ClcModuleConfig {
            ds: [0, 0, 0, 0],
            gates: [[false; 8]; 4],
            gpol: [false; 4],
            mode: 0,
            lcpol: false,
            lcoe: true,
            lcen: true,
            intp: false,
            intn: false,
        },
    )]);
    let config = PinConfig {
        part_number: device.part_number.clone(),
        assignments: vec![],
        digital_pins: vec![],
    };

    let files = generate_c_files(
        &device,
        &config,
        None,
        None,
        Some(&osc),
        Some("#pragma config FNOSC = FRC"),
        Some(&clc_modules),
    );
    let c = &files[&generated_file_names(DEFAULT_OUTPUT_BASENAME).source];

    assert!(c.contains("dsPIC33AK clock configuration"));
    assert!(!c.contains("#pragma config FNOSC ="));
    assert!(c.contains("PLL1CONbits.ON = 1U;"));
    assert!(c.contains("CLK1CONbits.NOSC = 5U;"));
    assert!(c.contains("OSCCTRLbits.PLL1RDY == 0U"));
    assert!(c.contains("CLC1CON = 0x00000000U;"));
    assert!(c.contains("CLC1SEL = 0x00000000U;"));
    assert!(c.contains("CLC1GLS = 0x00000000U;"));
    assert!(c.contains("CLC1CON = 0x00008080U;"));
    assert!(!c.contains("CLC1CONL ="));
}
