//! Oscillator and PLL configuration for dsPIC33CK devices.
//!
//! Calculates PLL divider values (N1, M, N2, N3) to achieve a target system clock
//! frequency, and generates C initialization code with #pragma config lines.

use serde::{Deserialize, Serialize};

/// dsPIC33CK FRC nominal frequency
pub const FRC_FREQ_HZ: u64 = 8_000_000;

// PLL constraints (dsPIC33CK)
const PLL_M_MIN: u64 = 16;
const PLL_M_MAX: u64 = 200;
const PLL_N1_MIN: u64 = 1;
const PLL_N1_MAX: u64 = 8;
const PLL_N2_MIN: u64 = 1;
const PLL_N2_MAX: u64 = 7;
const PLL_N3_MIN: u64 = 1;
const PLL_N3_MAX: u64 = 7;
const VCO_MIN_HZ: u64 = 400_000_000;
const VCO_MAX_HZ: u64 = 1_600_000_000;
const FPFD_MIN_HZ: u64 = 8_000_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OscConfig {
    pub source: String,
    pub target_fosc_hz: u64,
    #[serde(default)]
    pub crystal_hz: u64,
    #[serde(default = "default_poscmd")]
    pub poscmd: String,
}

fn default_poscmd() -> String {
    "EC".to_string()
}

#[derive(Debug, Clone)]
pub struct PLLResult {
    pub n1: u64,
    pub m: u64,
    pub n2: u64,
    pub n3: u64,
    pub fplli: u64,
    pub fvco: u64,
    pub fosc: u64,
    pub fcy: u64,
    pub error_ppm: i64,
}

/// Search the legal dsPIC33 PLL divider space and return the closest match to
/// the requested FOSC, rejecting combinations that violate FPFD/VCO limits.
pub fn calculate_pll(fplli_hz: u64, target_fosc_hz: u64) -> Option<PLLResult> {
    let mut best: Option<PLLResult> = None;

    for n1 in PLL_N1_MIN..=PLL_N1_MAX {
        let fpfd = fplli_hz / n1;
        if fpfd < FPFD_MIN_HZ {
            continue;
        }

        for n2 in PLL_N2_MIN..=PLL_N2_MAX {
            for n3 in PLL_N3_MIN..=PLL_N3_MAX {
                // M = target_fosc * N1 * N2 * N3 / fplli
                let m_exact = (target_fosc_hz as f64) * (n1 as f64) * (n2 as f64) * (n3 as f64)
                    / (fplli_hz as f64);
                let m = m_exact.round() as u64;
                if !(PLL_M_MIN..=PLL_M_MAX).contains(&m) {
                    continue;
                }

                let fvco = fplli_hz * m / n1;
                if !(VCO_MIN_HZ..=VCO_MAX_HZ).contains(&fvco) {
                    continue;
                }

                let fosc = fvco / (n2 * n3);
                let fcy = fosc / 2;

                let error_ppm = if target_fosc_hz > 0 {
                    ((fosc as i64 - target_fosc_hz as i64).unsigned_abs() * 1_000_000
                        / target_fosc_hz) as i64
                } else {
                    0
                };

                let result = PLLResult {
                    n1,
                    m,
                    n2,
                    n3,
                    fplli: fplli_hz,
                    fvco,
                    fosc,
                    fcy,
                    error_ppm,
                };

                let is_better = match &best {
                    None => true,
                    Some(b) => error_ppm < b.error_ppm,
                };

                if is_better {
                    best = Some(result);
                    if error_ppm == 0 {
                        return best;
                    }
                }
            }
        }
    }

    best
}

fn xtcfg_for_crystal(crystal_hz: u64) -> &'static str {
    let mhz = crystal_hz as f64 / 1_000_000.0;
    if mhz <= 8.0 {
        "G0"
    } else if mhz <= 16.0 {
        "G1"
    } else if mhz <= 24.0 {
        "G2"
    } else {
        "G3"
    }
}

/// Generate oscillator configuration code.
/// Returns (pragmas, init_function) as separate strings.
pub fn generate_osc_code(osc: &OscConfig) -> (String, String) {
    let mut pragma_lines = Vec::new();
    let mut init_lines = Vec::new();

    match osc.source.as_str() {
        "frc" => {
            pragma_lines.push("/* Oscillator configuration: FRC (8 MHz), Fcy = 4 MHz */".into());
            pragma_lines.push("#pragma config FNOSC = FRC        /* Fast RC Oscillator */".into());
            pragma_lines.push(
                "#pragma config IESO = OFF         /* Start with selected oscillator */".into(),
            );
            pragma_lines
                .push("#pragma config POSCMD = NONE      /* Primary oscillator disabled */".into());
            pragma_lines
                .push("#pragma config FCKSM = CSDCMD     /* Clock switching disabled */".into());
            return (pragma_lines.join("\n"), String::new());
        }
        "lprc" => {
            pragma_lines.push("/* Oscillator configuration: LPRC (32 kHz), Fcy = 16 kHz */".into());
            pragma_lines
                .push("#pragma config FNOSC = LPRC       /* Low-Power RC Oscillator */".into());
            pragma_lines.push(
                "#pragma config IESO = OFF         /* Start with selected oscillator */".into(),
            );
            pragma_lines
                .push("#pragma config POSCMD = NONE      /* Primary oscillator disabled */".into());
            pragma_lines
                .push("#pragma config FCKSM = CSDCMD     /* Clock switching disabled */".into());
            return (pragma_lines.join("\n"), String::new());
        }
        "pri" => {
            let fosc = osc.crystal_hz;
            let fcy = fosc / 2;
            pragma_lines.push(format!(
                "/* Oscillator configuration: Primary ({}), Fosc = {:.3} MHz, Fcy = {:.3} MHz */",
                osc.poscmd,
                fosc as f64 / 1e6,
                fcy as f64 / 1e6
            ));
            pragma_lines.push("#pragma config FNOSC = PRI        /* Primary Oscillator */".into());
            pragma_lines.push(
                "#pragma config IESO = OFF         /* Start with selected oscillator */".into(),
            );
            let poscmd_pad = if osc.poscmd.len() < 4 { "  " } else { " " };
            pragma_lines.push(format!(
                "#pragma config POSCMD = {}{}/* Primary oscillator mode */",
                osc.poscmd, poscmd_pad
            ));
            if osc.poscmd == "XT" || osc.poscmd == "HS" {
                let xtcfg = xtcfg_for_crystal(osc.crystal_hz);
                pragma_lines.push(format!(
                    "#pragma config XTCFG = {}          /* Crystal range: {} */",
                    xtcfg, xtcfg
                ));
            }
            pragma_lines
                .push("#pragma config FCKSM = CSDCMD     /* Clock switching disabled */".into());
            return (pragma_lines.join("\n"), String::new());
        }
        "frc_pll" | "pri_pll" => {
            let (fplli, fnosc, fnosc_comment, poscmd) = if osc.source == "frc_pll" {
                (FRC_FREQ_HZ, "FRCPLL", "FRC with PLL", "NONE".to_string())
            } else {
                (
                    osc.crystal_hz,
                    "PRIPLL",
                    "Primary Oscillator with PLL",
                    osc.poscmd.clone(),
                )
            };

            let pll = match calculate_pll(fplli, osc.target_fosc_hz) {
                Some(p) => p,
                None => {
                    pragma_lines.push(
                        "/* ERROR: no valid PLL configuration found for target frequency */".into(),
                    );
                    return (pragma_lines.join("\n"), String::new());
                }
            };

            pragma_lines.push(format!("/* Oscillator configuration: {} */", fnosc_comment));
            pragma_lines.push(format!(
                "/* Fosc = {:.3} MHz, Fcy = {:.3} MHz */",
                pll.fosc as f64 / 1e6,
                pll.fcy as f64 / 1e6
            ));
            pragma_lines.push(format!(
                "/* PLL: FPLLI={:.1} MHz, M={}, N1={}, N2={}, N3={}, FVCO={:.1} MHz */",
                pll.fplli as f64 / 1e6,
                pll.m,
                pll.n1,
                pll.n2,
                pll.n3,
                pll.fvco as f64 / 1e6
            ));
            if pll.error_ppm > 0 {
                pragma_lines.push(format!("/* Frequency error: {} ppm */", pll.error_ppm));
            }
            let fnosc_pad = if fnosc.len() < 6 { "  " } else { " " };
            pragma_lines.push(format!(
                "#pragma config FNOSC = {}{}/* {} */",
                fnosc, fnosc_pad, fnosc_comment
            ));
            pragma_lines.push(
                "#pragma config IESO = OFF         /* Start with selected oscillator */".into(),
            );
            let poscmd_pad = if poscmd.len() < 4 { "  " } else { " " };
            pragma_lines.push(format!(
                "#pragma config POSCMD = {}{}/* Primary oscillator mode */",
                poscmd, poscmd_pad
            ));
            if poscmd == "XT" || poscmd == "HS" {
                let xtcfg = xtcfg_for_crystal(osc.crystal_hz);
                pragma_lines.push(format!(
                    "#pragma config XTCFG = {}          /* Crystal range: {} */",
                    xtcfg, xtcfg
                ));
            }
            pragma_lines
                .push("#pragma config FCKSM = CSDCMD     /* Clock switching disabled */".into());
            pragma_lines.push(
                "#pragma config PLLKEN = ON        /* Disable output if PLL loses lock */".into(),
            );

            // Generate PLL init function
            init_lines.push(
                "/* ---------------------------------------------------------------------------"
                    .into(),
            );
            init_lines.push(" * configure_oscillator".into());
            init_lines.push(" *".into());
            init_lines.push(format!(
                " * Configures the PLL for Fosc = {:.3} MHz (Fcy = {:.3} MHz).",
                pll.fosc as f64 / 1e6,
                pll.fcy as f64 / 1e6
            ));
            init_lines.push(format!(
                " * PLL input: {:.1} MHz, VCO: {:.1} MHz",
                pll.fplli as f64 / 1e6,
                pll.fvco as f64 / 1e6
            ));
            init_lines.push(format!(
                " * Fosc = FPLLI * M / (N1 * N2 * N3) = {:.1} * {} / ({} * {} * {})",
                pll.fplli as f64 / 1e6,
                pll.m,
                pll.n1,
                pll.n2,
                pll.n3
            ));
            init_lines.push(
                " * -------------------------------------------------------------------------*/"
                    .into(),
            );
            init_lines.push("void configure_oscillator(void)".into());
            init_lines.push("{".into());
            init_lines.push(format!("    /* PLL prescaler: N1 = {} */", pll.n1));
            init_lines.push(format!("    CLKDIVbits.PLLPRE = {}U;", pll.n1));
            init_lines.push(String::new());
            init_lines.push(format!("    /* PLL feedback divider: M = {} */", pll.m));
            init_lines.push(format!("    PLLFBDbits.PLLFBDIV = {}U;", pll.m));
            init_lines.push(String::new());
            init_lines.push(format!("    /* PLL postscaler #1: N2 = {} */", pll.n2));
            init_lines.push(format!("    PLLDIVbits.POST1DIV = {}U;", pll.n2));
            init_lines.push(String::new());
            init_lines.push(format!("    /* PLL postscaler #2: N3 = {} */", pll.n3));
            init_lines.push(format!("    PLLDIVbits.POST2DIV = {}U;", pll.n3));
            init_lines.push(String::new());
            init_lines.push("    /* Wait for PLL to lock */".into());
            init_lines.push("    while (OSCCONbits.LOCK != 1U)".into());
            init_lines.push("    {".into());
            init_lines.push("        /* Intentionally empty — MISRA C:2012 Rule 15.6 */".into());
            init_lines.push("    }".into());
            init_lines.push("}".into());
            init_lines.push(String::new());

            return (pragma_lines.join("\n"), init_lines.join("\n"));
        }
        _ => {}
    }

    (String::new(), String::new())
}

#[cfg(test)]
mod tests {
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
        // 5 GHz is far above what this PLL can produce (max ~1.6 GHz Fvco)
        let result = calculate_pll(8_000_000, 5_000_000_000);
        if let Some(r) = &result {
            // Best-effort result should have massive error
            assert!(
                r.error_ppm > 500_000,
                "error_ppm={} should be huge",
                r.error_ppm
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
}
