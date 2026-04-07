//! Oscillator and PLL configuration facade for dsPIC33/PIC24 code generation.
//!
//! The public `codegen::oscillator` API stays here, while the implementation is
//! split into shared PLL/model helpers, legacy CK-style pragma generation, and
//! dsPIC33AK runtime clock-generator emission.

#[path = "oscillator/ak.rs"]
mod ak;
#[path = "oscillator/legacy.rs"]
mod legacy;
#[path = "oscillator/model.rs"]
mod model;

pub use model::{
    calculate_pll, instruction_cycle_hz, managed_config_fields, managed_config_fields_for_device,
    OscConfig, PLLResult, FRC_FREQ_HZ, LPRC_FREQ_HZ,
};

use crate::part_profile::PartProfile;

pub fn is_dspic33ak_part(part_number: &str) -> bool {
    PartProfile::from_part_number(part_number).is_dspic33ak()
}

/// Generate oscillator configuration code for legacy dsPIC33/PIC24 families.
/// Returns `(pragmas, init_function)` as separate strings.
pub fn generate_osc_code(osc: &OscConfig) -> (String, String) {
    legacy::generate_legacy_osc_code(osc)
}

pub fn generate_osc_code_for_device(part_number: &str, osc: &OscConfig) -> (String, String) {
    if is_dspic33ak_part(part_number) {
        ak::generate_ak_clock_code(osc)
    } else {
        generate_osc_code(osc)
    }
}

#[cfg(test)]
#[path = "oscillator/tests.rs"]
mod tests;
