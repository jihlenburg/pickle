//! dsPIC33AK runtime clock-generator emission.
//!
//! dsPIC33AK no longer uses the legacy CK-style oscillator pragma model, so
//! this module emits the runtime `OSCCFG` / `CLK1*` / `PLL1*` initialization
//! sequence instead.

use crate::codegen::oscillator::model::{
    calculate_pll, instruction_cycle_hz, OscConfig, FRC_FREQ_HZ, LPRC_FREQ_HZ,
};

const AK_CLOCK_SRC_FRC: u8 = 1;
const AK_CLOCK_SRC_BFRC: u8 = 2;
const AK_CLOCK_SRC_POSC: u8 = 3;
const AK_CLOCK_SRC_LPRC: u8 = 4;
const AK_CLOCK_SRC_PLL1_FOUT: u8 = 5;

fn ak_poscmd_bits(poscmd: &str) -> Option<u8> {
    match poscmd.trim().to_ascii_uppercase().as_str() {
        "EC" => Some(0),
        "XT" => Some(1),
        "HS" => Some(2),
        "NONE" | "DISABLED" => Some(3),
        _ => None,
    }
}

fn push_busy_wait(lines: &mut Vec<String>, condition: &str) {
    lines.push(format!("    while ({condition})"));
    lines.push("    {".into());
    lines.push("        /* Intentionally empty — MISRA C:2012 Rule 15.6 */".into());
    lines.push("    }".into());
}

fn push_ak_clock_generator_setup(lines: &mut Vec<String>) {
    lines.push("    /* Enable system clock generator 1 with BFRC fail-safe backup. */".into());
    lines.push("    CLK1CONbits.ON = 1U;".into());
    lines.push("    CLK1CONbits.OE = 1U;".into());
    lines.push(format!("    CLK1CONbits.BOSC = {}U;", AK_CLOCK_SRC_BFRC));
    lines.push("    CLK1CONbits.FSCMEN = 1U;".into());
    lines.push(String::new());
    lines.push("    /* Keep CLK1 undivided so Fosc matches the selected source. */".into());
    lines.push("    CLK1DIVbits.INTDIV = 0U;".into());
    lines.push("    CLK1DIVbits.FRACDIV = 0U;".into());
    lines.push("    CLK1CONbits.DIVSWEN = 1U;".into());
    push_busy_wait(lines, "CLK1CONbits.DIVSWEN == 1U");
    lines.push(String::new());
}

fn push_ak_clock_switch(
    lines: &mut Vec<String>,
    register: &str,
    source_value: u8,
    ready_wait: &str,
) {
    lines.push(format!("    {register}bits.NOSC = {source_value}U;"));
    lines.push(format!("    {register}bits.OSWEN = 1U;"));
    push_busy_wait(lines, &format!("{register}bits.OSWEN == 1U"));
    push_busy_wait(lines, ready_wait);
}

pub(crate) fn generate_ak_clock_code(osc: &OscConfig) -> (String, String) {
    let mut summary_lines = Vec::new();
    let mut init_lines = Vec::new();
    let source = osc.source.trim().to_ascii_lowercase();

    summary_lines.push("/* dsPIC33AK clock configuration */".into());
    summary_lines.push(
        "/* Clock selection is handled at runtime through OSCCFG / CLK1CON / CLK1DIV / */".into(),
    );
    summary_lines.push("/* PLL1CON / PLL1DIV. dsPIC33AK devices run Fcy = Fosc. */".into());
    summary_lines.push(format!("/* Requested source: {} */", osc.source));

    init_lines.push(
        "/* ---------------------------------------------------------------------------".into(),
    );
    init_lines.push(" * configure_oscillator".into());
    init_lines.push(" *".into());
    init_lines.push(" * Configures the dsPIC33AK clock-generator / PLL registers using the".into());
    init_lines.push(
        " * source-selection and update sequence documented in the family data sheet.".into(),
    );
    init_lines.push(
        " * -------------------------------------------------------------------------*/".into(),
    );
    init_lines.push("void configure_oscillator(void)".into());
    init_lines.push("{".into());

    match source.as_str() {
        "frc" => {
            let fosc = FRC_FREQ_HZ;
            summary_lines.push(format!(
                "/* Fosc = {:.3} MHz, Fcy = {:.3} MHz */",
                fosc as f64 / 1e6,
                instruction_cycle_hz("DSPIC33AK", fosc) as f64 / 1e6
            ));
            push_ak_clock_generator_setup(&mut init_lines);
            init_lines.push("    /* Switch CLK1 to the internal FRC oscillator. */".into());
            push_ak_clock_switch(
                &mut init_lines,
                "CLK1CON",
                AK_CLOCK_SRC_FRC,
                "CLK1CONbits.CLKRDY == 0U",
            );
        }
        "lprc" => {
            let fosc = LPRC_FREQ_HZ;
            summary_lines.push(format!(
                "/* Fosc = {:.6} MHz, Fcy = {:.6} MHz */",
                fosc as f64 / 1e6,
                instruction_cycle_hz("DSPIC33AK", fosc) as f64 / 1e6
            ));
            push_ak_clock_generator_setup(&mut init_lines);
            init_lines.push("    /* Switch CLK1 to the low-power RC oscillator. */".into());
            push_ak_clock_switch(
                &mut init_lines,
                "CLK1CON",
                AK_CLOCK_SRC_LPRC,
                "CLK1CONbits.CLKRDY == 0U",
            );
        }
        "pri" => {
            let Some(poscmd_bits) = ak_poscmd_bits(&osc.poscmd) else {
                summary_lines
                    .push("/* ERROR: unsupported primary oscillator mode for dsPIC33AK. */".into());
                init_lines.push("    /* ERROR: unsupported primary oscillator mode. */".into());
                init_lines.push("}".into());
                init_lines.push(String::new());
                return (summary_lines.join("\n"), init_lines.join("\n"));
            };
            if osc.crystal_hz == 0 {
                summary_lines.push(
                    "/* ERROR: a primary-oscillator clock requires a crystal/external source frequency. */"
                        .into(),
                );
                init_lines.push(
                    "    /* ERROR: primary oscillator selected without a crystal/external source frequency. */"
                        .into(),
                );
                init_lines.push("}".into());
                init_lines.push(String::new());
                return (summary_lines.join("\n"), init_lines.join("\n"));
            }

            summary_lines.push(format!(
                "/* Fosc = {:.3} MHz, Fcy = {:.3} MHz */",
                osc.crystal_hz as f64 / 1e6,
                instruction_cycle_hz("DSPIC33AK", osc.crystal_hz) as f64 / 1e6
            ));
            summary_lines.push(format!(
                "/* Primary oscillator mode: {} */",
                osc.poscmd.trim().to_ascii_uppercase()
            ));

            push_ak_clock_generator_setup(&mut init_lines);
            init_lines.push("    /* Configure the primary oscillator pin mode. */".into());
            init_lines.push(format!("    OSCCFGbits.POSCMD = {poscmd_bits}U;"));
            init_lines.push(String::new());
            init_lines.push("    /* Switch CLK1 to the primary oscillator input. */".into());
            push_ak_clock_switch(
                &mut init_lines,
                "CLK1CON",
                AK_CLOCK_SRC_POSC,
                "CLK1CONbits.CLKRDY == 0U",
            );
        }
        "frc_pll" | "pri_pll" => {
            let (pll_input_hz, pll_input_source, summary_source) = if source == "frc_pll" {
                (FRC_FREQ_HZ, AK_CLOCK_SRC_FRC, "FRC through PLL1")
            } else {
                if osc.crystal_hz == 0 {
                    summary_lines.push(
                        "/* ERROR: a primary-oscillator PLL clock requires a crystal/external source frequency. */"
                            .into(),
                    );
                    init_lines.push(
                        "    /* ERROR: primary oscillator PLL selected without a crystal/external source frequency. */"
                            .into(),
                    );
                    init_lines.push("}".into());
                    init_lines.push(String::new());
                    return (summary_lines.join("\n"), init_lines.join("\n"));
                }
                (
                    osc.crystal_hz,
                    AK_CLOCK_SRC_POSC,
                    "Primary oscillator through PLL1",
                )
            };

            if source == "pri_pll" {
                let Some(poscmd_bits) = ak_poscmd_bits(&osc.poscmd) else {
                    summary_lines.push(
                        "/* ERROR: unsupported primary oscillator mode for dsPIC33AK. */".into(),
                    );
                    init_lines.push("    /* ERROR: unsupported primary oscillator mode. */".into());
                    init_lines.push("}".into());
                    init_lines.push(String::new());
                    return (summary_lines.join("\n"), init_lines.join("\n"));
                };
                summary_lines.push(format!(
                    "/* Primary oscillator mode: {} */",
                    osc.poscmd.trim().to_ascii_uppercase()
                ));
                init_lines.push("    /* Configure the primary oscillator pin mode. */".into());
                init_lines.push(format!("    OSCCFGbits.POSCMD = {poscmd_bits}U;"));
                init_lines.push(String::new());
            }

            let Some(pll) = calculate_pll(pll_input_hz, osc.target_fosc_hz) else {
                summary_lines.push(
                    "/* ERROR: no valid dsPIC33AK PLL configuration found for the requested Fosc. */"
                        .into(),
                );
                init_lines.push(
                    "    /* ERROR: no valid PLL divider combination found for the requested Fosc. */"
                        .into(),
                );
                init_lines.push("}".into());
                init_lines.push(String::new());
                return (summary_lines.join("\n"), init_lines.join("\n"));
            };

            summary_lines.push(format!(
                "/* Fosc = {:.3} MHz, Fcy = {:.3} MHz */",
                pll.fosc as f64 / 1e6,
                instruction_cycle_hz("DSPIC33AK", pll.fosc) as f64 / 1e6
            ));
            summary_lines.push(format!(
                "/* Source: {}, PLL1 = Fin * {} / ({} * {} * {}) */",
                summary_source, pll.m, pll.n1, pll.n2, pll.n3
            ));
            summary_lines.push(format!(
                "/* PLL input = {:.3} MHz, VCO = {:.3} MHz */",
                pll.fplli as f64 / 1e6,
                pll.fvco as f64 / 1e6
            ));
            if pll.error_ppm > 0 {
                summary_lines.push(format!("/* Frequency error: {} ppm */", pll.error_ppm));
            }

            push_ak_clock_generator_setup(&mut init_lines);
            init_lines.push("    /* Configure PLL1 for the requested system clock. */".into());
            init_lines.push("    PLL1CONbits.ON = 1U;".into());
            init_lines.push("    PLL1CONbits.OE = 1U;".into());
            init_lines.push(format!("    PLL1CONbits.BOSC = {}U;", AK_CLOCK_SRC_BFRC));
            init_lines.push("    PLL1CONbits.FSCMEN = 1U;".into());
            init_lines.push(format!("    PLL1DIVbits.PLLFBDIV = {}U;", pll.m));
            init_lines.push(format!("    PLL1DIVbits.PLLPRE = {}U;", pll.n1));
            init_lines.push(format!("    PLL1DIVbits.POSTDIV1 = {}U;", pll.n2));
            init_lines.push(format!("    PLL1DIVbits.POSTDIV2 = {}U;", pll.n3));
            init_lines.push(String::new());
            init_lines.push("    /* Commit PLL1 divider changes. */".into());
            init_lines.push("    PLL1CONbits.PLLSWEN = 1U;".into());
            push_busy_wait(&mut init_lines, "PLL1CONbits.PLLSWEN == 1U");
            init_lines.push("    PLL1CONbits.FOUTSWEN = 1U;".into());
            push_busy_wait(&mut init_lines, "PLL1CONbits.FOUTSWEN == 1U");
            init_lines.push(String::new());
            init_lines
                .push("    /* Select the PLL1 reference clock source and wait for lock. */".into());
            push_ak_clock_switch(
                &mut init_lines,
                "PLL1CON",
                pll_input_source,
                "OSCCTRLbits.PLL1RDY == 0U",
            );
            init_lines.push(String::new());
            init_lines
                .push("    /* Route the PLL1 Fout output to system clock generator 1. */".into());
            push_ak_clock_switch(
                &mut init_lines,
                "CLK1CON",
                AK_CLOCK_SRC_PLL1_FOUT,
                "CLK1CONbits.CLKRDY == 0U",
            );
        }
        _ => {
            summary_lines.push("/* ERROR: unsupported dsPIC33AK oscillator source. */".into());
            init_lines.push("    /* ERROR: unsupported oscillator source. */".into());
        }
    }

    init_lines.push("}".into());
    init_lines.push(String::new());

    (summary_lines.join("\n"), init_lines.join("\n"))
}
