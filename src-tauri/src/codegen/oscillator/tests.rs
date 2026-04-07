//! Unit tests for the public oscillator facade.
//!
//! These tests intentionally stay at the facade boundary so internal module
//! splits do not leak into the rest of the backend or change the public API.

use super::*;

#[test]
fn test_frc_to_200mhz() {
    let result = calculate_pll(8_000_000, 200_000_000).unwrap();
    assert_eq!(result.fosc, 200_000_000);
    assert_eq!(result.error_ppm, 0);
}

#[test]
fn test_frc_to_100mhz() {
    let result = calculate_pll(8_000_000, 100_000_000).unwrap();
    assert_eq!(result.fosc, 100_000_000);
    assert_eq!(result.error_ppm, 0);
    assert_eq!(result.fcy, 50_000_000);
}

#[test]
fn test_frc_to_140mhz() {
    let result = calculate_pll(8_000_000, 140_000_000).unwrap();
    assert_eq!(result.fosc, 140_000_000);
    assert_eq!(result.error_ppm, 0);
}

#[test]
fn test_crystal_10mhz_to_200mhz() {
    let result = calculate_pll(10_000_000, 200_000_000).unwrap();
    assert_eq!(result.fosc, 200_000_000);
    assert_eq!(result.error_ppm, 0);
}

#[test]
fn test_crystal_12mhz_to_100mhz() {
    let result = calculate_pll(12_000_000, 100_000_000).unwrap();
    assert_eq!(result.fosc, 100_000_000);
}

#[test]
fn test_vco_range() {
    let result = calculate_pll(8_000_000, 200_000_000).unwrap();
    assert!(result.fvco >= 400_000_000);
    assert!(result.fvco <= 1_600_000_000);
}

#[test]
fn test_fpfd_minimum() {
    let result = calculate_pll(8_000_000, 200_000_000).unwrap();
    let fpfd = result.fplli / result.n1;
    assert!(fpfd >= 8_000_000);
}

#[test]
fn test_divider_ranges() {
    let result = calculate_pll(8_000_000, 200_000_000).unwrap();
    assert!((1..=8).contains(&result.n1));
    assert!((16..=200).contains(&result.m));
    assert!((1..=7).contains(&result.n2));
    assert!((1..=7).contains(&result.n3));
}

#[test]
fn test_fosc_formula() {
    let result = calculate_pll(8_000_000, 200_000_000).unwrap();
    let expected = result.fplli * result.m / (result.n1 * result.n2 * result.n3);
    assert_eq!(result.fosc, expected);
}

#[test]
fn test_unreachable_frequency() {
    let result = calculate_pll(8_000_000, 5_000_000_000);
    if let Some(result) = &result {
        assert!(
            result.error_ppm > 500_000,
            "error_ppm={} should be huge",
            result.error_ppm
        );
    }
}

#[test]
fn test_generate_frc_osc() {
    let osc = OscConfig {
        source: "frc".to_string(),
        target_fosc_hz: 0,
        crystal_hz: 0,
        poscmd: "EC".to_string(),
    };
    let (pragmas, init) = generate_osc_code(&osc);
    assert!(pragmas.contains("FNOSC = FRC"));
    assert!(pragmas.contains("POSCMD = NONE"));
    assert!(init.is_empty());
}

#[test]
fn test_instruction_cycle_matches_fosc_on_dspic33ak() {
    assert_eq!(
        instruction_cycle_hz("DSPIC33AK64MC105", 200_000_000),
        200_000_000
    );
    assert_eq!(
        instruction_cycle_hz("DSPIC33CK64MP102", 200_000_000),
        100_000_000
    );
}

#[test]
fn test_generate_frc_pll_osc() {
    let osc = OscConfig {
        source: "frc_pll".to_string(),
        target_fosc_hz: 200_000_000,
        crystal_hz: 0,
        poscmd: "EC".to_string(),
    };
    let (pragmas, init) = generate_osc_code(&osc);
    assert!(pragmas.contains("FNOSC = FRCPLL"));
    assert!(pragmas.contains("200.000 MHz"));
    assert!(init.contains("configure_oscillator"));
    assert!(init.contains("CLKDIVbits.PLLPRE"));
    assert!(init.contains("PLLFBDbits.PLLFBDIV"));
    assert!(init.contains("OSCCONbits.LOCK"));
}

#[test]
fn test_dspic33ak_generates_runtime_clock_sequence_and_no_ck_fuse_fields() {
    let osc = OscConfig {
        source: "frc_pll".to_string(),
        target_fosc_hz: 200_000_000,
        crystal_hz: 0,
        poscmd: "EC".to_string(),
    };
    let (pragmas, init) = generate_osc_code_for_device("DSPIC33AK64MC105", &osc);
    assert!(pragmas.contains("dsPIC33AK clock configuration"));
    assert!(!pragmas.contains("FNOSC"));
    assert!(init.contains("PLL1DIVbits.PLLFBDIV = "));
    assert!(init.contains("PLL1CONbits.PLLSWEN = 1U;"));
    assert!(init.contains("PLL1CONbits.NOSC = 1U;"));
    assert!(init.contains("CLK1CONbits.NOSC = 5U;"));
    assert!(init.contains("OSCCTRLbits.PLL1RDY == 0U"));
    assert!(managed_config_fields_for_device("DSPIC33AK64MC105", &osc).contains("FNOSC"));
}

#[test]
fn test_dspic33ak_primary_clock_switch_uses_osccfg_and_clk1() {
    let osc = OscConfig {
        source: "pri".to_string(),
        target_fosc_hz: 0,
        crystal_hz: 10_000_000,
        poscmd: "HS".to_string(),
    };
    let (pragmas, init) = generate_osc_code_for_device("DSPIC33AK64MC105", &osc);
    assert!(pragmas.contains("Primary oscillator mode: HS"));
    assert!(init.contains("OSCCFGbits.POSCMD = 2U;"));
    assert!(init.contains("CLK1CONbits.NOSC = 3U;"));
    assert!(init.contains("CLK1CONbits.CLKRDY == 0U"));
}

#[test]
fn test_managed_config_fields_tracks_pll_and_primary_crystal_options() {
    let frc_pll = managed_config_fields(&OscConfig {
        source: "frc_pll".to_string(),
        target_fosc_hz: 200_000_000,
        crystal_hz: 0,
        poscmd: "EC".to_string(),
    });
    assert!(frc_pll.contains("FNOSC"));
    assert!(frc_pll.contains("PLLKEN"));
    assert!(!frc_pll.contains("XTCFG"));

    let pri_xt = managed_config_fields(&OscConfig {
        source: "pri".to_string(),
        target_fosc_hz: 8_000_000,
        crystal_hz: 8_000_000,
        poscmd: "XT".to_string(),
    });
    assert!(pri_xt.contains("FNOSC"));
    assert!(pri_xt.contains("XTCFG"));
    assert!(!pri_xt.contains("PLLKEN"));
}
