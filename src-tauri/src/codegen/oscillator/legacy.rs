//! Legacy dsPIC33CK-style oscillator pragma and PLL emission.
//!
//! Older dsPIC33/PIC24 families still use the classic `#pragma config` clock
//! model plus PLL divider SFRs, so this module keeps that legacy generation
//! path separate from dsPIC33AK runtime clock-generator handling.

use crate::codegen::oscillator::model::{calculate_pll, xtcfg_for_crystal, OscConfig, FRC_FREQ_HZ};

/// Generate oscillator configuration code.
/// Returns (pragmas, init_function) as separate strings.
pub(crate) fn generate_legacy_osc_code(osc: &OscConfig) -> (String, String) {
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
                Some(pll) => pll,
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
