//! Shared backend part-number classification helpers.
//!
//! Parsing family/series/branch traits in one place keeps AK/CK/MPS behavior
//! from being reimplemented with ad hoc string-prefix checks across codegen,
//! parser-cache policy, toolchain selection, and verification filtering.

use once_cell::sync::Lazy;
use regex::Regex;

static FAMILY_PREFIX_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^((?:DSPIC|PIC)\d+[A-Z]+)").unwrap());
static BRANCH_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"([A-Z]{2,})\d{3}[A-Z]?$").unwrap());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceFamily {
    Pic24,
    Dspic33,
    Unknown,
}

impl DeviceFamily {
    pub fn as_key(self) -> &'static str {
        match self {
            Self::Pic24 => "pic24",
            Self::Dspic33 => "dspic33",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DsPic33Series {
    Ak,
    Ck,
    Ch,
    Cd,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartProfile {
    normalized_part_number: String,
    family: DeviceFamily,
    dspic33_series: Option<DsPic33Series>,
    family_prefix: Option<String>,
    branch: Option<String>,
}

impl PartProfile {
    pub fn from_part_number(part_number: &str) -> Self {
        let normalized_part_number = part_number.trim().to_ascii_uppercase();
        let family = if normalized_part_number.starts_with("PIC24") {
            DeviceFamily::Pic24
        } else if normalized_part_number.starts_with("DSPIC33") {
            DeviceFamily::Dspic33
        } else {
            DeviceFamily::Unknown
        };

        let dspic33_series = if normalized_part_number.starts_with("DSPIC33AK") {
            Some(DsPic33Series::Ak)
        } else if normalized_part_number.starts_with("DSPIC33CK") {
            Some(DsPic33Series::Ck)
        } else if normalized_part_number.starts_with("DSPIC33CH") {
            Some(DsPic33Series::Ch)
        } else if normalized_part_number.starts_with("DSPIC33CD") {
            Some(DsPic33Series::Cd)
        } else if family == DeviceFamily::Dspic33 {
            Some(DsPic33Series::Other)
        } else {
            None
        };

        let family_prefix = FAMILY_PREFIX_RE
            .captures(&normalized_part_number)
            .and_then(|caps| caps.get(1))
            .map(|capture| capture.as_str().to_string());
        let branch = BRANCH_RE
            .captures(&normalized_part_number)
            .and_then(|caps| caps.get(1))
            .map(|capture| capture.as_str().to_string());

        Self {
            normalized_part_number,
            family,
            dspic33_series,
            family_prefix,
            branch,
        }
    }

    pub fn family(&self) -> DeviceFamily {
        self.family
    }

    pub fn dspic33_series(&self) -> Option<DsPic33Series> {
        self.dspic33_series
    }

    pub fn family_prefix(&self) -> Option<&str> {
        self.family_prefix.as_deref()
    }

    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    pub fn is_dspic33ak(&self) -> bool {
        self.dspic33_series == Some(DsPic33Series::Ak)
    }

    pub fn is_dspic33ak_mps(&self) -> bool {
        self.is_dspic33ak() && self.branch() == Some("MPS")
    }

    pub fn is_dspic33ak_mc(&self) -> bool {
        self.is_dspic33ak() && self.branch() == Some("MC")
    }

    pub fn instruction_cycle_hz(&self, fosc_hz: u64) -> u64 {
        if self.is_dspic33ak() {
            fosc_hz
        } else {
            fosc_hz / 2
        }
    }

    pub fn manages_legacy_clock_fuses(&self) -> bool {
        !self.is_dspic33ak()
    }
}

pub fn detect_device_family(part_number: Option<&str>) -> DeviceFamily {
    PartProfile::from_part_number(part_number.unwrap_or_default()).family()
}

#[cfg(test)]
mod tests {
    use super::{detect_device_family, DeviceFamily, DsPic33Series, PartProfile};

    #[test]
    fn detects_pic24_and_dspic33_families() {
        assert_eq!(
            detect_device_family(Some(" PIC24FJ128GA204 ")),
            DeviceFamily::Pic24
        );
        assert_eq!(
            detect_device_family(Some(" dspic33ck64mp102 ")),
            DeviceFamily::Dspic33
        );
        assert_eq!(
            detect_device_family(Some("PIC18F27Q43")),
            DeviceFamily::Unknown
        );
    }

    #[test]
    fn captures_ak_branch_and_family_prefix() {
        let profile = PartProfile::from_part_number("DSPIC33AK256MPS205");
        assert_eq!(profile.family(), DeviceFamily::Dspic33);
        assert_eq!(profile.dspic33_series(), Some(DsPic33Series::Ak));
        assert_eq!(profile.family_prefix(), Some("DSPIC33AK"));
        assert_eq!(profile.branch(), Some("MPS"));
        assert!(profile.is_dspic33ak());
        assert!(profile.is_dspic33ak_mps());
        assert!(!profile.manages_legacy_clock_fuses());
        assert_eq!(profile.instruction_cycle_hz(200_000_000), 200_000_000);
    }

    #[test]
    fn non_ak_parts_keep_half_rate_instruction_cycle() {
        let profile = PartProfile::from_part_number("DSPIC33CK64MP102");
        assert_eq!(profile.dspic33_series(), Some(DsPic33Series::Ck));
        assert_eq!(profile.branch(), Some("MP"));
        assert!(profile.manages_legacy_clock_fuses());
        assert_eq!(profile.instruction_cycle_hz(200_000_000), 100_000_000);
    }
}
