//! DFP Pack Manager: find, fetch, and extract Microchip Device Family Pack files.
//! Loads pinout overlays for alternate package variants.
//!
//! Root precedence: read from the repo root first, then app-data as fallback.
//! Writes go to the first existing matching root so caches and overlays do not
//! split across multiple directories.

use regex::Regex;
use serde_json;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::parser::edc_parser::{parse_edc_file, DeviceData, Pad, Pinout};
use crate::parser::pack_index;

fn is_synthetic_package_name(name: &str) -> bool {
    name.trim().eq_ignore_ascii_case("default")
}

fn replace_redundant_default_pinout(device: &mut DeviceData) {
    let default_name = device.default_pinout.clone();
    if !is_synthetic_package_name(&default_name) {
        return;
    }

    let Some(default_pinout) = device.pinouts.get(&default_name).cloned() else {
        return;
    };

    let mut replacement_names: Vec<String> = device
        .pinouts
        .iter()
        .filter_map(|(name, pinout)| {
            if name.eq_ignore_ascii_case(&default_name) || is_synthetic_package_name(name) {
                return None;
            }
            if pinout.pin_count != default_pinout.pin_count {
                return None;
            }
            if pinout.pins != default_pinout.pins {
                return None;
            }
            Some(name.clone())
        })
        .collect();
    replacement_names.sort();

    if let Some(replacement_name) = replacement_names.into_iter().next() {
        device.pinouts.remove(&default_name);
        device.default_pinout = replacement_name;
    }
}

fn device_families() -> &'static Vec<(&'static str, &'static str)> {
    use once_cell::sync::Lazy;
    static FAMILIES: Lazy<Vec<(&str, &str)>> = Lazy::new(|| {
        vec![
            // dsPIC33 families
            ("dsPIC33CK-MP", r"DSPIC33CK\d+MP\d+"),
            ("dsPIC33CK-MC", r"DSPIC33CK\d+MC\d+"),
            ("dsPIC33CH-MP", r"DSPIC33CH\d+MP\d+"),
            ("dsPIC33CD-MP", r"DSPIC33CD\d+MP\d+"),
            ("dsPIC33CD-MC", r"DSPIC33CD\d+MC\d+"),
            ("dsPIC33E-GM-GP-MC-GU-MU", r"DSPIC33E[A-Z]+\d+[A-Z]*\d+"),
            ("dsPIC33F-GP-MC", r"DSPIC33F[A-Z]+\d+[A-Z]*\d+"),
            ("dsPIC33AK-MC", r"DSPIC33AK\d+MC\d+"),
            ("dsPIC33AK-MP", r"DSPIC33AK\d+MP\d+"),
            // PIC24 families
            ("PIC24EP-GP-MC-GU-MU", r"PIC24EP\d+[A-Z]+\d+"),
            ("PIC24FJ-GA-GB-GC-DA", r"PIC24FJ\d+[A-Z]+\d+"),
            ("PIC24F-KA-KL-KM", r"PIC24F\d+K[A-Z]\d+"),
            ("PIC24HJ-GP-GS", r"PIC24HJ\d+[A-Z]+\d+"),
        ]
    });
    &FAMILIES
}

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn read_roots() -> Vec<PathBuf> {
    let mut roots = vec![project_root()];

    let app_data = base_dir();
    if !roots.contains(&app_data) {
        roots.push(app_data);
    }

    roots
}

pub fn base_dir() -> PathBuf {
    // Use platform-standard app data directory:
    //   macOS:   ~/Library/Application Support/pickle
    //   Linux:   ~/.local/share/pickle
    //   Windows: %APPDATA%/pickle
    let dir = dirs::data_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("pickle");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn preferred_write_root(subdir: &str) -> PathBuf {
    // Keep writes colocated with the data the app is already reading. Otherwise
    // the user can end up reading stale overlays from one root and writing new
    // ones into another.
    for root in [project_root()] {
        if root.as_os_str().is_empty() {
            continue;
        }
        if root.join(subdir).exists() {
            return root;
        }
    }
    base_dir()
}

pub fn devices_dir() -> PathBuf {
    preferred_write_root("devices").join("devices")
}

pub fn dfp_cache_dir() -> PathBuf {
    preferred_write_root("dfp_cache").join("dfp_cache")
}

pub fn datasheets_dir() -> PathBuf {
    let dir = dfp_cache_dir().join("datasheets");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn pinouts_dir() -> PathBuf {
    preferred_write_root("pinouts").join("pinouts")
}

/// Returns (path, max_scan_depth) pairs. Cache directories get unlimited depth;
/// broad user directories like ~/Downloads are shallow to avoid long scans.
fn search_paths() -> Vec<(PathBuf, u32)> {
    let mut paths: Vec<(PathBuf, u32)> = Vec::new();

    for root in read_roots() {
        // Search extracted pack caches first so repeated loads stay offline before
        // falling back to broader user-level Microchip install/download folders.
        let cache = root.join("dfp_cache");
        if cache.exists() && !paths.iter().any(|(p, _)| p == &cache) {
            paths.push((cache, u32::MAX));
        }
    }

    if let Some(home) = dirs::home_dir() {
        let mchp = home.join(".mchp_packs").join("Microchip");
        if mchp.exists() {
            paths.push((mchp, 3));
        }
        // ~/Downloads only gets a shallow scan (depth 1) to avoid freezing
        // on large download folders with deep directory trees.
        let downloads = home.join("Downloads");
        if downloads.exists() {
            paths.push((downloads, 1));
        }
    }
    paths
}

fn compiler_pack_search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    for mplab_root in [
        PathBuf::from("/Applications/microchip/mplabx"),
        PathBuf::from("/opt/microchip/mplabx"),
    ] {
        let Ok(entries) = fs::read_dir(&mplab_root) else {
            continue;
        };
        for entry in entries.flatten() {
            let packs = entry.path().join("packs").join("Microchip");
            if packs.exists() && !roots.contains(&packs) {
                roots.push(packs);
            }
        }
    }

    if let Some(home) = dirs::home_dir() {
        let mchp = home.join(".mchp_packs").join("Microchip");
        if mchp.exists() && !roots.contains(&mchp) {
            roots.push(mchp);
        }
    }

    roots
}

fn pack_version_dir_from_pic_path(pic_path: &Path) -> Option<PathBuf> {
    let edc_dir = pic_path.parent()?;
    if !edc_dir
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("edc"))
    {
        return None;
    }
    edc_dir.parent().map(Path::to_path_buf)
}

fn compiler_support_dir_for_pack_version(pack_version_dir: &Path) -> Option<PathBuf> {
    let support_dir = pack_version_dir.join("xc16");
    if support_dir.join("bin").join("c30_device.info").is_file() {
        Some(support_dir)
    } else {
        None
    }
}

fn find_installed_compiler_support_dir(part_number: &str) -> Option<PathBuf> {
    let part_upper = part_number.to_ascii_uppercase();
    let mut matches = Vec::new();

    for root in compiler_pack_search_roots() {
        let Ok(entries) = glob_recursive_depth(&root, "*.PIC", 4) else {
            continue;
        };
        for pic_file in entries {
            let stem = pic_file
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_ascii_uppercase();
            if stem != part_upper {
                continue;
            }
            if let Some(pack_version_dir) = pack_version_dir_from_pic_path(&pic_file) {
                if let Some(support_dir) = compiler_support_dir_for_pack_version(&pack_version_dir)
                {
                    matches.push(support_dir);
                }
            }
        }
    }

    matches.sort();
    matches.pop()
}

fn find_atpack_for_part(part_number: &str) -> Option<PathBuf> {
    let part_upper = part_number.to_uppercase();
    let mut family_key: Option<&str> = None;

    for (fam, pattern) in device_families() {
        let re = Regex::new(pattern).ok()?;
        if re.is_match(&part_upper) {
            family_key = Some(fam);
            break;
        }
    }

    for (search_dir, max_depth) in search_paths() {
        // `.atpack` filenames are generally family/version oriented rather than
        // exact device names, so try a family match before a shorter part-number
        // substring heuristic.
        if let Ok(entries) = glob_recursive_depth(&search_dir, "*.atpack", max_depth) {
            for atpack in entries {
                if let Some(fam) = family_key {
                    let name = atpack
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .replace(['-', '_'], "");
                    if name.contains(&fam.replace('-', "")) {
                        return Some(atpack);
                    }
                }
                let name_lower = atpack
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                if name_lower.contains(&part_upper[..10.min(part_upper.len())].to_lowercase()) {
                    return Some(atpack);
                }
            }
        }

        // Some caches already contain extracted EDC `.PIC` files. Prefer those
        // over reopening and extracting the same `.atpack`.
        if let Ok(entries) = glob_recursive_depth(&search_dir, "*.PIC", max_depth) {
            for pic_file in entries {
                let stem = pic_file
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_uppercase();
                if stem.contains(&part_upper) {
                    return Some(pic_file);
                }
            }
        }
    }

    None
}

/// Walk `dir` for files matching `*.{ext}`, descending at most `max_depth` levels.
/// Depth 1 means only immediate children of `dir`.
fn glob_recursive_depth(
    dir: &Path,
    pattern: &str,
    max_depth: u32,
) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut results = Vec::new();
    let ext = pattern.trim_start_matches("*.");
    if max_depth == 0 || !dir.is_dir() {
        return Ok(results);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Ok(sub) = glob_recursive_depth(&path, pattern, max_depth - 1) {
                results.extend(sub);
            }
        } else if let Some(e) = path.extension() {
            if e.to_string_lossy().eq_ignore_ascii_case(ext) {
                results.push(path);
            }
        }
    }
    Ok(results)
}

fn extract_pic_from_atpack(atpack_path: &Path, part_number: &str) -> Option<PathBuf> {
    let part_upper = part_number.to_uppercase();
    let cache_dir = dfp_cache_dir();
    let edc_dir = cache_dir.join("edc");
    fs::create_dir_all(&edc_dir).ok()?;

    let output_path = edc_dir.join(format!("{}.PIC", part_upper));
    if output_path.exists() {
        return Some(output_path);
    }

    let file = fs::File::open(atpack_path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;

    let mut match_name: Option<String> = None;

    for i in 0..archive.len() {
        if let Ok(entry) = archive.by_index(i) {
            let name = entry.name().to_string();
            let name_upper = name.to_uppercase();
            if name_upper == format!("EDC/{}.PIC", part_upper) {
                match_name = Some(name);
                break;
            }
        }
    }

    if match_name.is_none() {
        for i in 0..archive.len() {
            if let Ok(entry) = archive.by_index(i) {
                let name = entry.name().to_string();
                if name.ends_with(".PIC") && name.to_uppercase().contains(&part_upper) {
                    match_name = Some(name);
                    break;
                }
            }
        }
    }

    if let Some(name) = match_name {
        if let Ok(mut entry) = archive.by_name(&name) {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).ok()?;
            fs::write(&output_path, &buf).ok()?;
            return Some(output_path);
        }
    }

    None
}

fn extract_compiler_support_from_atpack(atpack_path: &Path) -> Option<PathBuf> {
    let stem = atpack_path.file_stem()?.to_string_lossy().to_string();
    let toolchain_root = dfp_cache_dir().join("toolchain").join(stem);
    let support_dir = toolchain_root.join("xc16");
    if support_dir.join("bin").join("c30_device.info").is_file() {
        return Some(support_dir);
    }

    fs::create_dir_all(&toolchain_root).ok()?;
    let file = fs::File::open(atpack_path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;

    for i in 0..archive.len() {
        let Ok(mut entry) = archive.by_index(i) else {
            continue;
        };
        let name = entry.name().replace('\\', "/");
        let Some(relative) = name.strip_prefix("xc16/") else {
            continue;
        };
        let output_path = support_dir.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output_path).ok()?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).ok()?;
        }
        let mut out = fs::File::create(&output_path).ok()?;
        std::io::copy(&mut entry, &mut out).ok()?;
    }

    if support_dir.join("bin").join("c30_device.info").is_file() {
        Some(support_dir)
    } else {
        None
    }
}

fn load_pinout_overlays(device: &mut DeviceData) {
    let re_suffix = Regex::new(r"_\d+$").unwrap();

    for root in read_roots() {
        let overlay_path = root
            .join("pinouts")
            .join(format!("{}.json", device.part_number.to_uppercase()));
        if !overlay_path.exists() {
            continue;
        }

        let text = match fs::read_to_string(&overlay_path) {
            Ok(t) => t,
            Err(_) => continue,
        };

        let overlay: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if let Some(packages) = overlay.get("packages").and_then(|p| p.as_object()) {
            for (pkg_name, pkg_data) in packages {
                // Overlays only add alternate packages missing from the base EDC.
                // Case-insensitive check: the LLM may emit "28-PIN SSOP" while
                // the EDC defines "28-pin SSOP".
                let already_exists = device
                    .pinouts
                    .keys()
                    .any(|k| k.eq_ignore_ascii_case(pkg_name));
                if already_exists {
                    continue;
                }
                if pkg_data.get("source").and_then(|s| s.as_str()) != Some("overlay") {
                    continue;
                }

                let mut pin_map: HashMap<u32, String> = HashMap::new();
                if let Some(pins) = pkg_data.get("pins").and_then(|p| p.as_object()) {
                    for (pos_str, pad_val) in pins {
                        if let (Ok(pos), Some(pad_name)) =
                            (pos_str.parse::<u32>(), pad_val.as_str())
                        {
                            pin_map.insert(pos, pad_name.to_string());

                            // Overlay packages can reference suffixed pad aliases such as
                            // `VDD_2`. Clone the base pad metadata so downstream code can
                            // still resolve functions and port bindings from the alias.
                            if !device.pads.contains_key(pad_name) {
                                let base = re_suffix.replace(pad_name, "").to_string();
                                if let Some(src) = device.pads.get(&base).cloned() {
                                    device.pads.insert(
                                        pad_name.to_string(),
                                        Pad {
                                            name: pad_name.to_string(),
                                            functions: src.functions,
                                            rp_number: src.rp_number,
                                            port: src.port,
                                            port_bit: src.port_bit,
                                            analog_channels: src.analog_channels,
                                            is_power: src.is_power,
                                        },
                                    );
                                }
                            }
                        }
                    }
                }

                let pin_count = pkg_data
                    .get("pin_count")
                    .and_then(|c| c.as_u64())
                    .unwrap_or(pin_map.len() as u64) as u32;

                device.pinouts.insert(
                    pkg_name.clone(),
                    Pinout {
                        package: pkg_name.clone(),
                        pin_count,
                        source: "overlay".to_string(),
                        pins: pin_map,
                    },
                );
            }
        }

        replace_redundant_default_pinout(device);
    }
}

pub fn get_cached_device(part_number: &str) -> Option<(DeviceData, bool)> {
    let filename = format!("{}.json", part_number.to_uppercase());

    for root in read_roots() {
        let dir = root.join("devices");
        let json_path = dir.join(&filename);
        if json_path.exists() {
            if let Ok(text) = fs::read_to_string(&json_path) {
                let has_clc_key = text.contains("\"clc_module_id\"");
                if let Ok(device) = DeviceData::from_json(&text) {
                    return Some((device, has_clc_key));
                }
            }
        }
    }

    None
}

pub fn save_cached_device(device: &DeviceData) -> Option<PathBuf> {
    let dir = devices_dir();
    fs::create_dir_all(&dir).ok()?;
    let json_path = dir.join(format!("{}.json", device.part_number.to_uppercase()));
    fs::write(&json_path, device.to_json()).ok()?;
    Some(json_path)
}

pub fn list_cached_devices() -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();

    for root in read_roots() {
        let dir = root.join("devices");
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "json").unwrap_or(false) {
                    if let Some(stem) = path.file_stem() {
                        names.insert(stem.to_string_lossy().to_uppercase());
                    }
                }
            }
        }

        let edc_dir = root.join("dfp_cache").join("edc");
        if edc_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&edc_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map(|e| e == "PIC").unwrap_or(false) {
                        if let Some(stem) = path.file_stem() {
                            names.insert(stem.to_string_lossy().to_uppercase());
                        }
                    }
                }
            }
        }

        let po_dir = root.join("pinouts");
        if po_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&po_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map(|e| e == "json").unwrap_or(false) {
                        if let Some(stem) = path.file_stem() {
                            names.insert(stem.to_string_lossy().to_uppercase());
                        }
                    }
                }
            }
        }
    }

    names.into_iter().collect()
}

/// Merge locally cached devices with the remote pack index without letting a
/// transient index failure hide devices the user can already open offline.
pub fn list_all_known_devices() -> Vec<String> {
    let mut all = std::collections::BTreeSet::new();
    for d in list_cached_devices() {
        all.insert(d);
    }
    if let Ok(remote) = std::panic::catch_unwind(pack_index::list_all_devices) {
        for d in remote {
            all.insert(d);
        }
    }
    all.into_iter().collect()
}

pub fn load_device(part_number: &str) -> Option<DeviceData> {
    let part_upper = part_number.to_uppercase();

    // Fast path: parsed JSON cache. Re-parse if the cache is stale:
    //   - empty fuse_defs: predates DCR parsing pass
    //   - any field with empty values: predates range-field filtering
    //   - missing clc_module_id key: predates CLC module detection pass
    if let Some((mut cached, has_clc_key)) = get_cached_device(&part_upper) {
        let fuse_stale = cached.fuse_defs.is_empty()
            || cached
                .fuse_defs
                .iter()
                .any(|r| r.fields.iter().any(|f| f.values.is_empty()));
        if !fuse_stale && has_clc_key {
            load_pinout_overlays(&mut cached);
            load_clc_sources(&mut cached);
            return Some(cached);
        }
        // Stale cache — fall through to re-parse from the .PIC file.
    }

    let mut found = find_atpack_for_part(&part_upper);

    if found.is_none() {
        if let Some((url, filename)) = pack_index::lookup_device_pack(&part_upper) {
            match pack_index::download_atpack(&url, &filename) {
                Ok(path) => found = Some(path),
                Err(e) => {
                    eprintln!("Failed to download DFP for {}: {}", part_upper, e);
                    return None;
                }
            }
        } else {
            return None;
        }
    }

    let found = found?;
    let pic_path = if found.extension().map(|e| e == "PIC").unwrap_or(false) {
        Some(found)
    } else if found.extension().map(|e| e == "atpack").unwrap_or(false) {
        extract_pic_from_atpack(&found, &part_upper)
    } else {
        None
    };

    let pic_path = pic_path?;
    if !pic_path.exists() {
        return None;
    }

    let mut device = parse_edc_file(&pic_path).ok()?;
    save_cached_device(&device);
    // Apply overlays and CLC source mappings after parsing/caching so cached
    // and freshly parsed devices expose the same data to the frontend.
    load_pinout_overlays(&mut device);
    load_clc_sources(&mut device);

    Some(device)
}

/// Resolve a compiler-ready DFP directory for the given device.
///
/// XC16 and XC-DSC both expect `-mdfp` to point at the pack's `xc16/`
/// subtree because `elf-cc1` resolves `bin/c30_device.info` relative to that
/// path. Prefer installed MPLAB X packs, then fall back to a cached/downloaded
/// `.atpack` extracted into pickle's own cache.
pub fn find_compiler_support_dir(part_number: &str) -> Option<PathBuf> {
    let part_upper = part_number.to_ascii_uppercase();

    if let Some(installed) = find_installed_compiler_support_dir(&part_upper) {
        return Some(installed);
    }

    let atpack = match find_atpack_for_part(&part_upper) {
        Some(path)
            if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("atpack")) =>
        {
            Some(path)
        }
        _ => pack_index::lookup_device_pack(&part_upper)
            .and_then(|(url, filename)| pack_index::download_atpack(&url, &filename).ok()),
    }?;

    extract_compiler_support_from_atpack(&atpack)
}

/// Known CLC input source MUX mappings keyed by the IP-block module ID
/// extracted from the EDC `_modsrc` attribute.  Each entry is 4 groups (DS1–DS4)
/// of 8 source labels matching the CLCxSEL register definition in the datasheet.
///
/// Source: DS70005363E §21.1, Register 21-3 (CLCxSEL) — dsPIC33CK64MP105 family.
/// All 82 dsPIC33CK devices share the same CLC IP block.
fn known_clc_sources(module_id: &str) -> Option<Vec<Vec<String>>> {
    match module_id {
        "DOS-01577_cla_clc_upb_v1.Module" => Some(vec![
            // DS1[2:0] — page 373
            vec![
                "CLCINA".into(),
                "Fcy".into(),
                "CLC3OUT".into(),
                "LPRC".into(),
                "REFCLKO".into(),
                "Reserved".into(),
                "SCCP2 Aux".into(),
                "SCCP4 Aux".into(),
            ],
            // DS2[2:0] — page 372
            vec![
                "CLCINB".into(),
                "Reserved".into(),
                "CMP1".into(),
                "UART1 TX".into(),
                "Reserved".into(),
                "Reserved".into(),
                "SCCP1 OC".into(),
                "SCCP2 OC".into(),
            ],
            // DS3[2:0] — page 372
            vec![
                "CLCINC".into(),
                "CLC1OUT".into(),
                "CMP2".into(),
                "SPI1 SDO".into(),
                "UART1 RX".into(),
                "CLC4OUT".into(),
                "SCCP3 CEF".into(),
                "SCCP4 CEF".into(),
            ],
            // DS4[2:0] — page 372
            vec![
                "PWM Event A".into(),
                "CLC2OUT".into(),
                "CMP3".into(),
                "SPI1 SDI".into(),
                "Reserved".into(),
                "CLCIND".into(),
                "SCCP1 Aux".into(),
                "SCCP3 Aux".into(),
            ],
        ]),
        _ => None,
    }
}

/// Directory for CLC source mapping overrides (LLM-extracted or user-supplied).
pub fn clc_sources_dir() -> PathBuf {
    preferred_write_root("clc_sources").join("clc_sources")
}

/// Populate `device.clc_input_sources` from the best available source:
/// 1. Per-device JSON override in `clc_sources/{PART}.json`
/// 2. Hardcoded mapping for the device's `clc_module_id`
fn load_clc_sources(device: &mut DeviceData) {
    // 1. Check for per-device file override (from LLM extraction or manual edit)
    let part_upper = device.part_number.to_uppercase();
    for root in read_roots() {
        let path = root
            .join("clc_sources")
            .join(format!("{}.json", part_upper));
        if path.exists() {
            if let Ok(text) = fs::read_to_string(&path) {
                if let Ok(sources) = serde_json::from_str::<Vec<Vec<String>>>(&text) {
                    if sources.len() == 4 && sources.iter().all(|g| g.len() == 8) {
                        device.clc_input_sources = Some(sources);
                        return;
                    }
                }
            }
        }
    }

    // 2. Fall back to hardcoded mapping by CLC module ID
    if let Some(ref mod_id) = device.clc_module_id {
        if let Some(sources) = known_clc_sources(mod_id) {
            device.clc_input_sources = Some(sources);
        }
    }
}

/// Try to find a datasheet PDF locally. Searches:
/// 1. Fetcher's PDF cache (`dfp_cache/datasheets/pdf/`)
/// 2. Legacy datasheets dir (`dfp_cache/datasheets/`)
/// 3. `~/Downloads` (shallow) for PDFs matching the part number or family
///
/// Returns the path to the PDF if found, or None.
pub fn find_local_datasheet(part_number: &str) -> Option<PathBuf> {
    let part_upper = part_number.to_uppercase();
    let part_lower = part_number.to_lowercase();

    // 1. Check fetcher PDF cache (exact name)
    let fetcher_pdf = dfp_cache_dir()
        .join("datasheets")
        .join("pdf")
        .join(format!("{}.pdf", part_upper));
    if fetcher_pdf.exists() {
        return Some(fetcher_pdf);
    }

    // 2. Check legacy datasheets dir
    let ds_dir = datasheets_dir();
    if let Ok(entries) = fs::read_dir(&ds_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .map(|e| e.eq_ignore_ascii_case("pdf"))
                .unwrap_or(false)
            {
                let stem = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_uppercase();
                if stem.contains(&part_upper) {
                    return Some(path);
                }
            }
        }
    }

    // 3. Search ~/Downloads (shallow, top-level only) for matching PDFs
    // Derive a family prefix for broader matching (e.g. "dspic33ck" from "DSPIC33CK64MP102")
    let family_lower = {
        let re = Regex::new(r"^((?:DSPIC|PIC)\d+[A-Z]+)").unwrap();
        re.captures(&part_upper)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_lowercase())
            .unwrap_or_default()
    };

    if let Some(home) = dirs::home_dir() {
        let downloads = home.join("Downloads");
        if let Ok(entries) = fs::read_dir(&downloads) {
            let mut best: Option<PathBuf> = None;
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                if !path
                    .extension()
                    .map(|e| e.eq_ignore_ascii_case("pdf"))
                    .unwrap_or(false)
                {
                    continue;
                }
                let name_lower = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                if name_lower.contains(&part_lower) {
                    return Some(path);
                }
                if !family_lower.is_empty() && name_lower.contains(&family_lower) && best.is_none()
                {
                    best = Some(path);
                }
            }
            if best.is_some() {
                return best;
            }
        }
    }

    None
}

/// Cache a datasheet PDF so future lookups find it without prompting.
pub fn cache_datasheet(part_number: &str, pdf_bytes: &[u8]) -> Option<PathBuf> {
    let dir = datasheets_dir();
    let path = dir.join(format!("{}.pdf", part_number.to_uppercase()));
    fs::write(&path, pdf_bytes).ok()?;
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_device(default_name: &str, default_pinout: Pinout, extra_pinouts: Vec<(String, Pinout)>) -> DeviceData {
        let mut pinouts = HashMap::new();
        pinouts.insert(default_name.to_string(), default_pinout);
        for (name, pinout) in extra_pinouts {
            pinouts.insert(name, pinout);
        }

        DeviceData {
            part_number: "TEST123".to_string(),
            pads: HashMap::new(),
            pinouts,
            default_pinout: default_name.to_string(),
            remappable_inputs: Vec::new(),
            remappable_outputs: Vec::new(),
            pps_input_mappings: Vec::new(),
            pps_output_mappings: Vec::new(),
            port_registers: HashMap::new(),
            ansel_bits: HashMap::new(),
            fuse_defs: Vec::new(),
            clc_module_id: None,
            clc_input_sources: None,
        }
    }

    #[test]
    fn pack_version_dir_is_parent_of_edc_file() {
        let pic = PathBuf::from(
            "/tmp/packs/Microchip/dsPIC33CK-MP_DFP/1.15.423/edc/DSPIC33CK64MP105.PIC",
        );
        let expected = PathBuf::from("/tmp/packs/Microchip/dsPIC33CK-MP_DFP/1.15.423");
        assert_eq!(pack_version_dir_from_pic_path(&pic), Some(expected));
    }

    #[test]
    fn compiler_support_dir_uses_xc16_subtree() {
        let temp = tempfile::tempdir().expect("temp dir");
        let pack = temp.path().join("pack").join("1.0.0");
        fs::create_dir_all(pack.join("xc16/bin")).expect("support dir");
        fs::write(pack.join("xc16/bin/c30_device.info"), "").expect("device info");

        assert_eq!(
            compiler_support_dir_for_pack_version(&pack),
            Some(pack.join("xc16"))
        );
    }

    #[test]
    fn redundant_default_pinout_is_replaced_by_matching_real_package() {
        let default_pinout = Pinout {
            package: "default".to_string(),
            pin_count: 2,
            source: "edc".to_string(),
            pins: HashMap::from([(1, "RA0".to_string()), (2, "RA1".to_string())]),
        };
        let real_pinout = Pinout {
            package: "64-Pin VQFN-TQFP".to_string(),
            pin_count: 2,
            source: "overlay".to_string(),
            pins: HashMap::from([(1, "RA0".to_string()), (2, "RA1".to_string())]),
        };
        let mut device = test_device(
            "default",
            default_pinout,
            vec![("64-Pin VQFN-TQFP".to_string(), real_pinout)],
        );

        replace_redundant_default_pinout(&mut device);

        assert_eq!(device.default_pinout, "64-Pin VQFN-TQFP");
        assert!(!device.pinouts.contains_key("default"));
        assert!(device.pinouts.contains_key("64-Pin VQFN-TQFP"));
    }

    #[test]
    fn default_pinout_is_kept_when_real_package_differs() {
        let default_pinout = Pinout {
            package: "default".to_string(),
            pin_count: 2,
            source: "edc".to_string(),
            pins: HashMap::from([(1, "RA0".to_string()), (2, "RA1".to_string())]),
        };
        let real_pinout = Pinout {
            package: "64-Pin VQFN-TQFP".to_string(),
            pin_count: 2,
            source: "overlay".to_string(),
            pins: HashMap::from([(1, "RA0".to_string()), (2, "RB1".to_string())]),
        };
        let mut device = test_device(
            "default",
            default_pinout,
            vec![("64-Pin VQFN-TQFP".to_string(), real_pinout)],
        );

        replace_redundant_default_pinout(&mut device);

        assert_eq!(device.default_pinout, "default");
        assert!(device.pinouts.contains_key("default"));
        assert!(device.pinouts.contains_key("64-Pin VQFN-TQFP"));
    }
}
