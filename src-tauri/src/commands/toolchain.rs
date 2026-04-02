//! Compile-check toolchain discovery and invocation.

use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use crate::parser::dfp_manager;
use crate::settings;

use super::{write_text_file, CompileCheckRequest, CompileCheckResponse, CompilerResponse};

const COMPILER_SEARCH_ROOTS: &[&str] = &["/Applications/microchip", "/opt/microchip"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceFamily {
    Pic24,
    Dspic33,
    Unknown,
}

impl DeviceFamily {
    fn as_key(self) -> &'static str {
        match self {
            Self::Pic24 => "pic24",
            Self::Dspic33 => "dspic33",
            Self::Unknown => "unknown",
        }
    }
}

fn detect_device_family(part_number: Option<&str>) -> DeviceFamily {
    let upper = part_number.unwrap_or_default().trim().to_ascii_uppercase();
    if upper.starts_with("PIC24") {
        DeviceFamily::Pic24
    } else if upper.starts_with("DSPIC33") {
        DeviceFamily::Dspic33
    } else {
        DeviceFamily::Unknown
    }
}

fn loaded_toolchain_settings() -> settings::ToolchainSettings {
    settings::load()
        .map(|settings| settings.toolchain)
        .unwrap_or_default()
}

fn resolve_compiler_command(part_number: Option<&str>) -> (String, DeviceFamily) {
    let family = detect_device_family(part_number);
    let settings = loaded_toolchain_settings();
    let configured = match family {
        DeviceFamily::Pic24 => &settings.family_compilers.pic24,
        DeviceFamily::Dspic33 => &settings.family_compilers.dspic33,
        DeviceFamily::Unknown => &settings.fallback_compiler,
    };

    let command = configured.trim();
    let fallback = settings.fallback_compiler.trim();

    if command.is_empty() {
        (fallback.to_string(), family)
    } else {
        (command.to_string(), family)
    }
}

fn search_root_candidates(root: &Path, binary: &str) -> Vec<PathBuf> {
    let mut candidates = vec![root.join(binary), root.join("bin").join(binary)];
    let Ok(entries) = fs::read_dir(root) else {
        return candidates;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        candidates.push(path.join(binary));
        candidates.push(path.join("bin").join(binary));
        if !path.is_dir() {
            continue;
        }

        if let Ok(children) = fs::read_dir(&path) {
            for child in children.flatten() {
                let child_path = child.path();
                candidates.push(child_path.join(binary));
                candidates.push(child_path.join("bin").join(binary));
            }
        }
    }

    candidates
}

fn find_compiler(binary: &str) -> Option<String> {
    if let Ok(output) = Command::new("which").arg(binary).output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
    }

    for root in COMPILER_SEARCH_ROOTS {
        for candidate in search_root_candidates(Path::new(root), binary) {
            if candidate.is_file() {
                return Some(candidate.display().to_string());
            }
        }
    }
    None
}

fn part_to_mcpu(part_number: &str) -> String {
    let upper = part_number.to_uppercase();
    if let Some(rest) = upper.strip_prefix("DSPIC") {
        rest.to_string()
    } else if let Some(rest) = upper.strip_prefix("PIC") {
        rest.to_string()
    } else {
        upper
    }
}

fn generated_output_filenames(source_code: &str) -> (String, String) {
    for line in source_code.lines() {
        let trimmed = line.trim();
        let Some(include) = trimmed.strip_prefix("#include \"") else {
            continue;
        };
        let Some((header_name, _)) = include.split_once('"') else {
            continue;
        };
        let header_file = Path::new(header_name)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| name.ends_with(".h"))
            .map(ToOwned::to_owned);

        if let Some(header_file) = header_file {
            let source_file = Path::new(&header_file)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(|stem| format!("{stem}.c"))
                .unwrap_or_else(|| format!("{}.c", settings::default_codegen_output_basename()));
            return (source_file, header_file);
        }
    }

    let basename = settings::default_codegen_output_basename();
    (format!("{basename}.c"), format!("{basename}.h"))
}

#[tauri::command]
pub fn compiler_info(part_number: Option<String>) -> Result<CompilerResponse, String> {
    let (command, family) = resolve_compiler_command(part_number.as_deref());
    let dfp_available = part_number
        .as_deref()
        .map(|part| dfp_manager::find_compiler_support_dir(part).is_some())
        .unwrap_or(true);

    match find_compiler(&command) {
        Some(path) => {
            let version = Command::new(&path)
                .arg("--version")
                .output()
                .ok()
                .and_then(|output| {
                    String::from_utf8(output.stdout)
                        .ok()
                        .and_then(|text| text.lines().next().map(|line| line.to_string()))
                })
                .unwrap_or_else(|| "unknown".to_string());
            Ok(CompilerResponse {
                available: dfp_available,
                command,
                device_family: family.as_key().to_string(),
                path: Some(path),
                version: Some(version),
            })
        }
        None => Ok(CompilerResponse {
            available: false,
            command,
            device_family: family.as_key().to_string(),
            path: None,
            version: None,
        }),
    }
}

#[tauri::command]
pub fn compile_check(request: CompileCheckRequest) -> Result<CompileCheckResponse, String> {
    let (command, family) = resolve_compiler_command(Some(&request.part_number));
    let gcc = find_compiler(&command)
        .ok_or_else(|| format!("{command} compiler not found on this system"))?;
    let dfp = dfp_manager::find_compiler_support_dir(&request.part_number).ok_or_else(|| {
        format!(
            "No compiler DFP found for {}. Install MPLAB X device packs or refresh pickle's DFP cache.",
            request.part_number
        )
    })?;

    let mcpu = part_to_mcpu(&request.part_number);
    let tmpdir = tempfile::tempdir().map_err(|e| format!("Temp dir error: {e}"))?;
    let (source_filename, header_filename) = generated_output_filenames(&request.code);
    let object_filename = Path::new(&source_filename)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| format!("{stem}.o"))
        .unwrap_or_else(|| format!("{}.o", settings::default_codegen_output_basename()));

    if !request.header.is_empty() {
        let header = tmpdir.path().join(header_filename);
        write_text_file(&header, &request.header)
            .map_err(|e| format!("Write header error: {e}"))?;
    }

    let source = tmpdir.path().join(source_filename);
    write_text_file(&source, &request.code).map_err(|e| format!("Write source error: {e}"))?;

    let output = Command::new(&gcc)
        .arg(format!("-mdfp={}", dfp.display()))
        .arg(format!("-mcpu={mcpu}"))
        .arg("-c")
        .arg(format!("-I{}", tmpdir.path().display()))
        .arg("-Wall")
        .arg("-Werror")
        .arg("-std=c99")
        .arg("-o")
        .arg(tmpdir.path().join(object_filename))
        .arg(&source)
        .output()
        .map_err(|e| format!("Compiler execution error: {e}"))?;

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if output.status.success() {
        return Ok(CompileCheckResponse {
            success: true,
            command,
            device_family: family.as_key().to_string(),
            errors: String::new(),
            warnings: if stderr.is_empty() {
                String::new()
            } else {
                stderr
            },
        });
    }

    Ok(CompileCheckResponse {
        success: false,
        command,
        device_family: family.as_key().to_string(),
        errors: if stderr.is_empty() { stdout } else { stderr },
        warnings: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_pic24_family_from_part_number_prefix() {
        assert_eq!(
            detect_device_family(Some(" PIC24FJ128GA204 ")),
            DeviceFamily::Pic24
        );
    }

    #[test]
    fn detects_dspic33_family_from_part_number_prefix() {
        assert_eq!(
            detect_device_family(Some(" dspic33ck64mp102 ")),
            DeviceFamily::Dspic33
        );
    }

    #[test]
    fn unknown_family_falls_back_to_generic_toolchain() {
        assert_eq!(
            detect_device_family(Some("PIC18F27Q43")),
            DeviceFamily::Unknown
        );
        assert_eq!(detect_device_family(None), DeviceFamily::Unknown);
    }

    #[test]
    fn part_to_mcpu_strips_microchip_family_prefixes() {
        assert_eq!(part_to_mcpu("DSPIC33CK64MP102"), "33CK64MP102");
        assert_eq!(part_to_mcpu("PIC24FJ128GA204"), "24FJ128GA204");
        assert_eq!(part_to_mcpu("CUSTOM"), "CUSTOM");
    }

    #[test]
    fn compile_check_uses_generated_header_include_name_when_present() {
        assert_eq!(
            generated_output_filenames(
                "#include \"board_setup.h\"\nint main(void) { return 0; }\n"
            ),
            ("board_setup.c".to_string(), "board_setup.h".to_string())
        );
    }
}
