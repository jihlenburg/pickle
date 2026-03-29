//! Configuration fuse generation for dsPIC33CK devices.
//! Generates #pragma config lines for FICD, FWDT, and FBORPOR registers.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuseConfig {
    #[serde(default = "default_ics")]
    pub ics: u32,
    #[serde(default = "default_off")]
    pub jtagen: String,
    #[serde(default = "default_off")]
    pub fwdten: String,
    #[serde(default = "default_wdtps")]
    pub wdtps: String,
    #[serde(default = "default_on")]
    pub boren: String,
    #[serde(default = "default_borv")]
    pub borv: String,
}

fn default_ics() -> u32 {
    1
}
fn default_off() -> String {
    "OFF".to_string()
}
fn default_on() -> String {
    "ON".to_string()
}
fn default_wdtps() -> String {
    "PS1024".to_string()
}
fn default_borv() -> String {
    "BOR_HIGH".to_string()
}

impl Default for FuseConfig {
    fn default() -> Self {
        Self {
            ics: 1,
            jtagen: "OFF".to_string(),
            fwdten: "OFF".to_string(),
            wdtps: "PS1024".to_string(),
            boren: "ON".to_string(),
            borv: "BOR_HIGH".to_string(),
        }
    }
}

pub fn generate_fuse_pragmas(fuse: &FuseConfig) -> String {
    let mut lines = Vec::new();

    // FICD: Debug Configuration
    lines.push("/* FICD — Debug Configuration */".to_string());
    lines.push(format!(
        "#pragma config ICS = ICS{}        /* Use PGC{}/PGD{} for debugging */",
        fuse.ics, fuse.ics, fuse.ics
    ));
    let jtag_pad = if fuse.jtagen.len() < 3 {
        "      "
    } else {
        "     "
    };
    let jtag_state = if fuse.jtagen == "ON" {
        "enabled"
    } else {
        "disabled"
    };
    lines.push(format!(
        "#pragma config JTAGEN = {}{}/* JTAG port {} */",
        fuse.jtagen, jtag_pad, jtag_state
    ));

    lines.push(String::new());

    // FWDT: Watchdog Timer
    lines.push("/* FWDT — Watchdog Timer */".to_string());
    let wdt_comment = match fuse.fwdten.as_str() {
        "OFF" => "Watchdog timer disabled",
        "ON" => "Watchdog timer always enabled",
        _ => "Watchdog timer controlled by software (WDTCON)",
    };
    let fwdten_pad = if fuse.fwdten.len() < 4 {
        "     "
    } else {
        "    "
    };
    lines.push(format!(
        "#pragma config FWDTEN = {}{}/* {} */",
        fuse.fwdten, fwdten_pad, wdt_comment
    ));
    let wdtps_pad = if fuse.wdtps.len() < 6 { "  " } else { " " };
    lines.push(format!(
        "#pragma config WDTPS = {}{}/* Watchdog prescaler: {} */",
        fuse.wdtps, wdtps_pad, fuse.wdtps
    ));

    lines.push(String::new());

    // FBORPOR: Brown-out / Power-on Reset
    lines.push("/* FBORPOR — Brown-out / Power-on Reset */".to_string());
    let boren_pad = if fuse.boren.len() < 3 {
        "       "
    } else {
        "      "
    };
    let boren_state = if fuse.boren == "ON" {
        "enabled"
    } else {
        "disabled"
    };
    lines.push(format!(
        "#pragma config BOREN = {}{}/* Brown-out reset {} */",
        fuse.boren, boren_pad, boren_state
    ));
    let borv_label = match fuse.borv.as_str() {
        "BOR_LOW" => "low threshold",
        "BOR_MID" => "mid threshold",
        "BOR_HIGH" => "high threshold",
        _ => &fuse.borv,
    };
    let borv_pad = if fuse.borv.len() < 8 { "  " } else { " " };
    lines.push(format!(
        "#pragma config BORV = {}{}/* Brown-out voltage: {} */",
        fuse.borv, borv_pad, borv_label
    ));

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_fuses() {
        let fuse = FuseConfig::default();
        let output = generate_fuse_pragmas(&fuse);

        assert!(output.contains("ICS = ICS1"));
        assert!(output.contains("JTAGEN = OFF"));
        assert!(output.contains("FWDTEN = OFF"));
        assert!(output.contains("WDTPS = PS1024"));
        assert!(output.contains("BOREN = ON"));
        assert!(output.contains("BORV = BOR_HIGH"));
    }

    #[test]
    fn test_custom_fuses() {
        let fuse = FuseConfig {
            ics: 2,
            jtagen: "ON".to_string(),
            fwdten: "SWON".to_string(),
            wdtps: "PS256".to_string(),
            boren: "OFF".to_string(),
            borv: "BOR_LOW".to_string(),
        };
        let output = generate_fuse_pragmas(&fuse);

        assert!(output.contains("ICS = ICS2"));
        assert!(output.contains("JTAGEN = ON"));
        assert!(output.contains("FWDTEN = SWON"));
        assert!(output.contains("WDTPS = PS256"));
        assert!(output.contains("BOREN = OFF"));
        assert!(output.contains("BORV = BOR_LOW"));
    }

    #[test]
    fn test_all_sections_present() {
        let fuse = FuseConfig::default();
        let output = generate_fuse_pragmas(&fuse);

        assert!(output.contains("FICD"));
        assert!(output.contains("FWDT"));
        assert!(output.contains("FBORPOR"));
    }

    #[test]
    fn test_all_pragma_fields_present() {
        let fuse = FuseConfig::default();
        let output = generate_fuse_pragmas(&fuse);

        for field in &["ICS", "JTAGEN", "FWDTEN", "WDTPS", "BOREN", "BORV"] {
            assert!(
                output.contains(&format!("#pragma config {} =", field)),
                "Missing pragma for {}",
                field
            );
        }
    }
}
