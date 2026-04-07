//! Shared oscillator model and PLL-search helpers.
//!
//! This module holds the public data shapes plus the reusable frequency and
//! fuse-ownership helpers used by both legacy dsPIC33CK generation and newer
//! dsPIC33AK runtime clock generation.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::part_profile::PartProfile;

/// dsPIC33CK FRC nominal frequency
pub const FRC_FREQ_HZ: u64 = 8_000_000;
/// dsPIC33 low-power RC nominal frequency
pub const LPRC_FREQ_HZ: u64 = 32_000;

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
                    Some(best) => error_ppm < best.error_ppm,
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

pub(crate) fn xtcfg_for_crystal(crystal_hz: u64) -> &'static str {
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

pub fn instruction_cycle_hz(part_number: &str, fosc_hz: u64) -> u64 {
    PartProfile::from_part_number(part_number).instruction_cycle_hz(fosc_hz)
}

/// Return the fuse fields owned by the oscillator panel for the selected source.
///
/// These fields are emitted directly by `generate_osc_code()`, so the generic
/// fuse generator must not emit them again from the device DCR definitions.
pub fn managed_config_fields(osc: &OscConfig) -> BTreeSet<&'static str> {
    let mut fields = BTreeSet::new();
    let source = osc.source.trim().to_ascii_lowercase();
    let poscmd = osc.poscmd.trim().to_ascii_uppercase();

    match source.as_str() {
        "frc" | "lprc" | "pri" | "frc_pll" | "pri_pll" => {
            fields.extend(["FNOSC", "IESO", "POSCMD", "FCKSM"]);
        }
        _ => return fields,
    }

    if matches!(source.as_str(), "frc_pll" | "pri_pll") {
        fields.insert("PLLKEN");
    }

    if matches!(source.as_str(), "pri" | "pri_pll") && matches!(poscmd.as_str(), "XT" | "HS") {
        fields.insert("XTCFG");
    }

    fields
}

pub fn managed_config_fields_for_device(
    part_number: &str,
    osc: &OscConfig,
) -> BTreeSet<&'static str> {
    if !PartProfile::from_part_number(part_number).manages_legacy_clock_fuses() {
        ["FNOSC", "IESO", "POSCMD", "XTCFG", "FCKSM", "PLLKEN"]
            .into_iter()
            .collect()
    } else {
        managed_config_fields(osc)
    }
}
