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
    if let Some(mut cached) = get_cached_device(&part_upper) {
        let fuse_stale = cached.fuse_defs.is_empty()
            || cached
                .fuse_defs
                .iter()
                .any(|r| r.fields.iter().any(|f| f.values.is_empty()));
        if !fuse_stale {
            load_pinout_overlays(&mut cached);
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
    // Apply overlays after parsing/caching so cached and freshly parsed devices
    // expose the same package set to the frontend.
    load_pinout_overlays(&mut device);

    Some(device)
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
