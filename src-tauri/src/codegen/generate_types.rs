//! Shared public data types for code generation.
//!
//! These structs form the stable interface between the command layer and the
//! generator. Keeping them outside the main generation implementation prevents
//! `generate.rs` from also having to act as the type-definition module.

use serde::{Deserialize, Serialize};

pub const DEFAULT_OUTPUT_BASENAME: &str = "mcu_init";

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinConfig {
    pub part_number: String,
    #[serde(default)]
    pub assignments: Vec<PinAssignment>,
    #[serde(default)]
    pub digital_pins: Vec<u32>,
}
