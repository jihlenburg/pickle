//! Behavior settings persisted in a small TOML file under the app data root.
//!
//! The frontend uses this for user preferences that should survive app restarts
//! without being hardcoded into the UI. Runtime credentials are intentionally
//! kept out of this file.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::parser::dfp_manager;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppearanceSettings {
    #[serde(default = "default_theme")]
    pub theme: String,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StartupSettings {
    #[serde(default = "default_startup_device")]
    pub device: String,
    #[serde(default)]
    pub package: String,
}

impl Default for StartupSettings {
    fn default() -> Self {
        Self {
            device: default_startup_device(),
            package: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolchainFamilySettings {
    #[serde(default = "default_pic24_compiler")]
    pub pic24: String,
    #[serde(default = "default_dspic33_compiler")]
    pub dspic33: String,
}

impl Default for ToolchainFamilySettings {
    fn default() -> Self {
        Self {
            pic24: default_pic24_compiler(),
            dspic33: default_dspic33_compiler(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolchainSettings {
    #[serde(
        default = "default_compile_check_compiler",
        rename = "fallback_compiler",
        alias = "compiler"
    )]
    pub fallback_compiler: String,
    #[serde(default)]
    pub family_compilers: ToolchainFamilySettings,
}

impl Default for ToolchainSettings {
    fn default() -> Self {
        Self {
            fallback_compiler: default_compile_check_compiler(),
            family_compilers: ToolchainFamilySettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodegenSettings {
    #[serde(default = "default_codegen_output_basename")]
    pub output_basename: String,
}

impl Default for CodegenSettings {
    fn default() -> Self {
        Self {
            output_basename: default_codegen_output_basename(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct LastUsedDevice {
    #[serde(default)]
    pub part_number: String,
    #[serde(default)]
    pub package: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AppSettings {
    #[serde(default)]
    pub appearance: AppearanceSettings,
    #[serde(default)]
    pub startup: StartupSettings,
    #[serde(default)]
    pub toolchain: ToolchainSettings,
    #[serde(default)]
    pub codegen: CodegenSettings,
    #[serde(default)]
    pub last_used: LastUsedDevice,
}

fn default_theme() -> String {
    "dark".to_string()
}

fn default_startup_device() -> String {
    "last-used".to_string()
}

pub fn default_compile_check_compiler() -> String {
    default_dspic33_compiler()
}

pub fn default_pic24_compiler() -> String {
    "xc16-gcc".to_string()
}

pub fn default_dspic33_compiler() -> String {
    "xc-dsc-gcc".to_string()
}

pub fn default_codegen_output_basename() -> String {
    "mcu_init".to_string()
}

fn normalize_theme(theme: &str) -> String {
    match theme.trim().to_ascii_lowercase().as_str() {
        "light" => "light".to_string(),
        "system" => "system".to_string(),
        _ => "dark".to_string(),
    }
}

fn normalize_startup_device(device: &str) -> String {
    let trimmed = device.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("last-used") {
        "last-used".to_string()
    } else {
        trimmed.to_ascii_uppercase()
    }
}

fn normalize_package(package: &str) -> String {
    package.trim().to_string()
}

fn normalize_part_number(part_number: &str) -> String {
    part_number.trim().to_ascii_uppercase()
}

fn normalize_compiler(compiler: &str) -> String {
    let normalized = compiler.trim();
    if normalized.is_empty() {
        default_compile_check_compiler()
    } else {
        normalized.to_string()
    }
}

fn normalize_output_basename(output_basename: &str) -> String {
    let mut normalized = String::new();
    let mut last_was_separator = false;

    for ch in output_basename.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
            last_was_separator = false;
            continue;
        }

        if matches!(ch, '_' | '-' | '.' | ' ') && !normalized.is_empty() && !last_was_separator {
            normalized.push('_');
            last_was_separator = true;
        }
    }

    while normalized.ends_with('_') {
        normalized.pop();
    }

    if normalized.is_empty() {
        default_codegen_output_basename()
    } else {
        normalized
    }
}

fn toml_string(value: &str) -> String {
    toml::Value::String(value.to_string()).to_string()
}

impl AppSettings {
    /// Canonicalize user-editable values so the file stays predictable and the
    /// frontend does not need to defend against arbitrary casing or whitespace.
    pub fn normalized(mut self) -> Self {
        self.appearance.theme = normalize_theme(&self.appearance.theme);
        self.startup.device = normalize_startup_device(&self.startup.device);
        self.startup.package = normalize_package(&self.startup.package);
        self.toolchain.fallback_compiler = normalize_compiler(&self.toolchain.fallback_compiler);
        self.toolchain.family_compilers.pic24 =
            normalize_compiler(&self.toolchain.family_compilers.pic24);
        self.toolchain.family_compilers.dspic33 =
            normalize_compiler(&self.toolchain.family_compilers.dspic33);
        self.codegen.output_basename = normalize_output_basename(&self.codegen.output_basename);
        self.last_used.part_number = normalize_part_number(&self.last_used.part_number);
        self.last_used.package = normalize_package(&self.last_used.package);
        self
    }
}

pub fn settings_path() -> PathBuf {
    dfp_manager::base_dir().join("settings.toml")
}

fn parse_settings(text: &str) -> Result<AppSettings, String> {
    toml::from_str::<AppSettings>(text)
        .map(AppSettings::normalized)
        .map_err(|e| format!("Cannot parse {}: {e}", settings_path().display()))
}

fn render_settings(settings: &AppSettings) -> String {
    let settings = settings.clone().normalized();

    format!(
        concat!(
            "# pickle behavior settings\n",
            "#\n",
            "# This file controls user-facing behavior that should persist across\n",
            "# app launches. Credentials are intentionally kept elsewhere.\n",
            "#\n",
            "# Startup behavior:\n",
            "#   startup.device = \"last-used\"     -> reopen the most recently loaded device/package\n",
            "#   startup.device = \"DSPIC33...\"    -> always start with that exact part number\n",
            "# If startup.device is \"last-used\" and no device has been loaded yet, the app starts blank.\n",
            "\n",
            "[appearance]\n",
            "# Theme mode: \"dark\", \"light\", or \"system\".\n",
            "theme = {}\n",
            "\n",
            "[startup]\n",
            "# Device to load automatically on startup.\n",
            "device = {}\n",
            "# Optional package override used only when startup.device names a fixed part.\n",
            "package = {}\n",
            "\n",
            "[toolchain]\n",
            "# Fallback compiler executable used when no known device family matches.\n",
            "fallback_compiler = {}\n",
            "\n",
            "[toolchain.family_compilers]\n",
            "# PIC24 devices compile with the classic XC16 toolchain.\n",
            "pic24 = {}\n",
            "# dsPIC33 devices compile with the XC-DSC toolchain.\n",
            "dspic33 = {}\n",
            "\n",
            "[codegen]\n",
            "# Basename used for generated C and header files.\n",
            "# Examples: \"mcu_init\" -> mcu_init.c / mcu_init.h\n",
            "output_basename = {}\n",
            "\n",
            "[last_used]\n",
            "# Updated automatically after every successful device load.\n",
            "part_number = {}\n",
            "package = {}\n"
        ),
        toml_string(&settings.appearance.theme),
        toml_string(&settings.startup.device),
        toml_string(&settings.startup.package),
        toml_string(&settings.toolchain.fallback_compiler),
        toml_string(&settings.toolchain.family_compilers.pic24),
        toml_string(&settings.toolchain.family_compilers.dspic33),
        toml_string(&settings.codegen.output_basename),
        toml_string(&settings.last_used.part_number),
        toml_string(&settings.last_used.package),
    )
}

pub fn load() -> Result<AppSettings, String> {
    let path = settings_path();
    if !path.exists() {
        let settings = AppSettings::default();
        save(&settings)?;
        return Ok(settings);
    }

    let text =
        fs::read_to_string(&path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if text.trim().is_empty() {
        let settings = AppSettings::default();
        save(&settings)?;
        return Ok(settings);
    }

    let parsed = parse_settings(&text)?;
    if text != render_settings(&parsed) {
        save(&parsed)?;
    }
    Ok(parsed)
}

pub fn save(settings: &AppSettings) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
    }
    fs::write(&path, render_settings(settings))
        .map_err(|e| format!("Cannot write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_use_last_used_startup_policy() {
        let settings = AppSettings::default();
        assert_eq!(settings.startup.device, "last-used");
        assert_eq!(settings.appearance.theme, "dark");
        assert_eq!(settings.toolchain.fallback_compiler, "xc-dsc-gcc");
        assert_eq!(settings.toolchain.family_compilers.pic24, "xc16-gcc");
        assert_eq!(settings.toolchain.family_compilers.dspic33, "xc-dsc-gcc");
        assert_eq!(settings.codegen.output_basename, "mcu_init");
    }

    #[test]
    fn normalization_canonicalizes_case_and_whitespace() {
        let settings = AppSettings {
            appearance: AppearanceSettings {
                theme: " SYSTEM ".to_string(),
            },
            startup: StartupSettings {
                device: " dspic33ck64mp102 ".to_string(),
                package: " tqfp-28 ".to_string(),
            },
            toolchain: ToolchainSettings {
                fallback_compiler: " xc-dsc-gcc ".to_string(),
                family_compilers: ToolchainFamilySettings {
                    pic24: " xc16-gcc ".to_string(),
                    dspic33: " xc-dsc-gcc ".to_string(),
                },
            },
            codegen: CodegenSettings {
                output_basename: " MCU-Init.v2 ".to_string(),
            },
            last_used: LastUsedDevice {
                part_number: " dspic33ck64mp102 ".to_string(),
                package: " tqfp-28 ".to_string(),
            },
        }
        .normalized();

        assert_eq!(settings.appearance.theme, "system");
        assert_eq!(settings.startup.device, "DSPIC33CK64MP102");
        assert_eq!(settings.startup.package, "tqfp-28");
        assert_eq!(settings.toolchain.fallback_compiler, "xc-dsc-gcc");
        assert_eq!(settings.toolchain.family_compilers.pic24, "xc16-gcc");
        assert_eq!(settings.toolchain.family_compilers.dspic33, "xc-dsc-gcc");
        assert_eq!(settings.codegen.output_basename, "mcu_init_v2");
        assert_eq!(settings.last_used.part_number, "DSPIC33CK64MP102");
        assert_eq!(settings.last_used.package, "tqfp-28");
    }

    #[test]
    fn rendered_file_documents_startup_behavior() {
        let text = render_settings(&AppSettings::default());
        assert!(text.contains("startup.device = \"last-used\""));
        assert!(text.contains("reopen the most recently loaded device/package"));
        assert!(text.contains("Theme mode: \"dark\", \"light\", or \"system\"."));
        assert!(text.contains("fallback_compiler = \"xc-dsc-gcc\""));
        assert!(text.contains("pic24 = \"xc16-gcc\""));
        assert!(text.contains("dspic33 = \"xc-dsc-gcc\""));
        assert!(text.contains("output_basename = \"mcu_init\""));
    }

    #[test]
    fn parse_settings_normalizes_invalid_and_missing_values() {
        let parsed = parse_settings(
            r#"
                [appearance]
                theme = " neon "

                [startup]
                device = " "
                package = " qfn-28 "

                [toolchain]
                fallback_compiler = "  "

                [toolchain.family_compilers]
                pic24 = " xc16-gcc "
                dspic33 = " xc-dsc-gcc "

                [codegen]
                output_basename = " Board Init "

                [last_used]
                part_number = " dspic33ch128mp508 "
            "#,
        )
        .expect("settings should parse");

        assert_eq!(parsed.appearance.theme, "dark");
        assert_eq!(parsed.startup.device, "last-used");
        assert_eq!(parsed.startup.package, "qfn-28");
        assert_eq!(parsed.toolchain.fallback_compiler, "xc-dsc-gcc");
        assert_eq!(parsed.toolchain.family_compilers.pic24, "xc16-gcc");
        assert_eq!(parsed.toolchain.family_compilers.dspic33, "xc-dsc-gcc");
        assert_eq!(parsed.codegen.output_basename, "board_init");
        assert_eq!(parsed.last_used.part_number, "DSPIC33CH128MP508");
        assert_eq!(parsed.last_used.package, "");
    }

    #[test]
    fn legacy_single_compiler_setting_maps_to_fallback_compiler() {
        let parsed = parse_settings(
            r#"
                [toolchain]
                compiler = " xc-dsc-gcc "
            "#,
        )
        .expect("legacy settings should parse");

        assert_eq!(parsed.toolchain.fallback_compiler, "xc-dsc-gcc");
        assert_eq!(parsed.toolchain.family_compilers.pic24, "xc16-gcc");
        assert_eq!(parsed.toolchain.family_compilers.dspic33, "xc-dsc-gcc");
    }

    #[test]
    fn render_settings_canonicalizes_values_before_writing() {
        let text = render_settings(&AppSettings {
            appearance: AppearanceSettings {
                theme: " SYSTEM ".to_string(),
            },
            startup: StartupSettings {
                device: " dspic33ck64mp102 ".to_string(),
                package: " tqfp-28 ".to_string(),
            },
            toolchain: ToolchainSettings {
                fallback_compiler: " xc-dsc-gcc ".to_string(),
                family_compilers: ToolchainFamilySettings {
                    pic24: " xc16-gcc ".to_string(),
                    dspic33: " xc-dsc-gcc ".to_string(),
                },
            },
            codegen: CodegenSettings {
                output_basename: " MCU Init ".to_string(),
            },
            last_used: LastUsedDevice {
                part_number: " dspic33ck64mp102 ".to_string(),
                package: " ssop-28 ".to_string(),
            },
        });

        assert!(text.contains("theme = \"system\""));
        assert!(text.contains("device = \"DSPIC33CK64MP102\""));
        assert!(text.contains("package = \"tqfp-28\""));
        assert!(text.contains("fallback_compiler = \"xc-dsc-gcc\""));
        assert!(text.contains("pic24 = \"xc16-gcc\""));
        assert!(text.contains("dspic33 = \"xc-dsc-gcc\""));
        assert!(text.contains("output_basename = \"mcu_init\""));
        assert!(text.contains("part_number = \"DSPIC33CK64MP102\""));
        assert!(text.contains("package = \"ssop-28\""));
        assert!(!text.contains(" SYSTEM "));
    }

    #[test]
    fn output_basename_normalization_falls_back_when_no_safe_name_remains() {
        assert_eq!(normalize_output_basename("   "), "mcu_init");
        assert_eq!(normalize_output_basename("..."), "mcu_init");
        assert_eq!(normalize_output_basename(" Board-Init "), "board_init");
    }

    #[test]
    fn parse_settings_errors_include_settings_path_context() {
        let error = parse_settings("[appearance").expect_err("invalid TOML should fail");
        assert!(error.contains("Cannot parse"));
        assert!(error.contains("settings.toml"));
    }
}
