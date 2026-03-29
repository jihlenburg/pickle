//! DFP Pack Manager: find, fetch, and extract Microchip Device Family Pack files.
//! Loads pinout overlays for alternate package variants.
//!
//! Root precedence is intentionally aligned with the sibling Python app in this
//! workspace: read from the repo root first, then `../config-pic`, then app-data.
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

fn dspic33_families() -> &'static Vec<(&'static str, &'static str)> {
    use once_cell::sync::Lazy;
    static FAMILIES: Lazy<Vec<(&str, &str)>> = Lazy::new(|| {
        vec![
            ("dsPIC33CK-MP", r"DSPIC33CK\d+MP\d+"),
            ("dsPIC33CK-MC", r"DSPIC33CK\d+MC\d+"),
            ("dsPIC33CH-MP", r"DSPIC33CH\d+MP\d+"),
            ("dsPIC33CD-MP", r"DSPIC33CD\d+MP\d+"),
            ("dsPIC33CD-MC", r"DSPIC33CD\d+MC\d+"),
            ("dsPIC33E-GM-GP-MC-GU-MU", r"DSPIC33E[A-Z]+\d+[A-Z]*\d+"),
            ("dsPIC33F-GP-MC", r"DSPIC33F[A-Z]+\d+[A-Z]*\d+"),
            ("dsPIC33AK-MC", r"DSPIC33AK\d+MC\d+"),
            ("dsPIC33AK-MP", r"DSPIC33AK\d+MP\d+"),
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

fn legacy_config_pic_root() -> Option<PathBuf> {
    let legacy = project_root().parent()?.join("config-pic");
    if legacy.is_dir() {
        Some(legacy)
    } else {
        None
    }
}

pub fn read_roots() -> Vec<PathBuf> {
    // Read precedence is deliberate: prefer the live workspace, then the legacy
    // `config-pic` checkout, then app-data as a portable fallback.
    let mut roots = vec![project_root()];

    if let Some(legacy) = legacy_config_pic_root() {
        if !roots.contains(&legacy) {
            roots.push(legacy);
        }
    }

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
    for root in [project_root(), legacy_config_pic_root().unwrap_or_default()] {
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

pub fn pinouts_dir() -> PathBuf {
    preferred_write_root("pinouts").join("pinouts")
}

fn search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    for root in read_roots() {
        // Search extracted pack caches first so repeated loads stay offline before
        // falling back to broader user-level Microchip install/download folders.
        let cache = root.join("dfp_cache");
        if cache.exists() && !paths.contains(&cache) {
            paths.push(cache);
        }
    }

    if let Some(home) = dirs::home_dir() {
        let mchp = home.join(".mchp_packs").join("Microchip");
        if mchp.exists() {
            paths.push(mchp);
        }
        let downloads = home.join("Downloads");
        if downloads.exists() {
            paths.push(downloads);
        }
    }
    paths
}

fn find_atpack_for_part(part_number: &str) -> Option<PathBuf> {
    let part_upper = part_number.to_uppercase();
    let mut family_key: Option<&str> = None;

    for (fam, pattern) in dspic33_families() {
        let re = Regex::new(pattern).ok()?;
        if re.is_match(&part_upper) {
            family_key = Some(fam);
            break;
        }
    }

    for search_dir in search_paths() {
        // `.atpack` filenames are generally family/version oriented rather than
        // exact device names, so try a family match before a shorter part-number
        // substring heuristic.
        if let Ok(entries) = glob_recursive(&search_dir, "*.atpack") {
            for atpack in entries {
                if let Some(fam) = family_key {
                    let name = atpack
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .replace('-', "")
                        .replace('_', "");
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
        if let Ok(entries) = glob_recursive(&search_dir, "*.PIC") {
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

fn glob_recursive(dir: &Path, pattern: &str) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut results = Vec::new();
    let ext = pattern.trim_start_matches("*.");
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                if let Ok(sub) = glob_recursive(&path, pattern) {
                    results.extend(sub);
                }
            } else if let Some(e) = path.extension() {
                if e.to_string_lossy().eq_ignore_ascii_case(ext) {
                    results.push(path);
                }
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
                // Do not silently replace an existing EDC-defined package here.
                if device.pinouts.contains_key(pkg_name) {
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
    }
}

pub fn get_cached_device(part_number: &str) -> Option<DeviceData> {
    let filename = format!("{}.json", part_number.to_uppercase());

    for root in read_roots() {
        // Honor the same precedence as the rest of runtime reads so the first hit
        // matches the device/overlay set the user would see elsewhere.
        let dir = root.join("devices");
        let json_path = dir.join(&filename);
        if json_path.exists() {
            if let Ok(text) = fs::read_to_string(&json_path) {
                if let Ok(device) = DeviceData::from_json(&text) {
                    return Some(device);
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

pub fn list_all_known_devices() -> Vec<String> {
    let mut all = std::collections::BTreeSet::new();
    for d in list_cached_devices() {
        all.insert(d);
    }
    if let Ok(remote) = std::panic::catch_unwind(|| pack_index::list_all_devices()) {
        for d in remote {
            all.insert(d);
        }
    }
    all.into_iter().collect()
}

pub fn load_device(part_number: &str) -> Option<DeviceData> {
    let part_upper = part_number.to_uppercase();

    // Fast path: parsed JSON cache, then local packs/extracted PICs, and finally
    // an on-demand download from the Microchip pack index.
    if let Some(mut cached) = get_cached_device(&part_upper) {
        load_pinout_overlays(&mut cached);
        return Some(cached);
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
    // Apply overlays after parsing/caching so cached and freshly parsed devices
    // expose the same package set to the frontend.
    load_pinout_overlays(&mut device);

    Some(device)
}
