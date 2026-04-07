//! DFP Pack Manager: find, fetch, and extract Microchip Device Family Pack files.
//! Loads pinout overlays for alternate package variants.
//!
//! Root precedence: read from the repo root first, then app-data as fallback.
//! Writes go to the first existing matching root so caches and overlays do not
//! split across multiple directories.

use regex::Regex;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::parser::dfp_datasheet;
use crate::parser::dfp_paths;
use crate::parser::dfp_store;
use crate::parser::edc_parser::{parse_edc_file, DeviceData};
use crate::parser::pack_index;
use crate::part_profile::PartProfile;

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

pub fn read_roots() -> Vec<PathBuf> {
    dfp_paths::read_roots()
}

pub fn base_dir() -> PathBuf {
    dfp_paths::base_dir()
}

pub fn devices_dir() -> PathBuf {
    dfp_paths::devices_dir()
}

pub fn dfp_cache_dir() -> PathBuf {
    dfp_paths::dfp_cache_dir()
}

pub fn datasheets_dir() -> PathBuf {
    dfp_paths::datasheets_dir()
}

pub fn datasheet_pdf_mismatch_reason(pdf_bytes: &[u8], part_number: &str) -> Option<String> {
    dfp_datasheet::datasheet_pdf_mismatch_reason(pdf_bytes, part_number)
}

pub fn pinouts_dir() -> PathBuf {
    dfp_paths::pinouts_dir()
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

pub fn get_cached_device(part_number: &str) -> Option<(DeviceData, bool, bool)> {
    dfp_store::get_cached_device(part_number)
}

pub fn save_cached_device(device: &DeviceData) -> Option<PathBuf> {
    dfp_store::save_cached_device(device)
}

pub fn list_cached_devices() -> Vec<String> {
    dfp_store::list_cached_devices()
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

fn is_dspic33ak_part(part_number: &str) -> bool {
    PartProfile::from_part_number(part_number).is_dspic33ak()
}

fn cached_device_needs_reparse(
    part_number: &str,
    cached: &DeviceData,
    has_clc_key: bool,
    has_device_info: bool,
) -> bool {
    let fuse_stale = cached.fuse_defs.is_empty()
        || cached
            .fuse_defs
            .iter()
            .any(|r| r.fields.iter().any(|f| f.values.is_empty()));
    let pps_stale = (!cached.remappable_inputs.is_empty() || !cached.remappable_outputs.is_empty())
        && cached.pps_input_mappings.is_empty()
        && cached.pps_output_mappings.is_empty();
    let ak_inventory_stale = is_dspic33ak_part(part_number)
        && cached.device_info.uarts == 0
        && cached.device_info.spis == 0
        && cached.device_info.i2c == 0
        && cached.device_info.pwm_generators == 0
        && cached.device_info.clc == 0
        && cached.device_info.qei == 0
        && cached.device_info.dac_channels == 0
        && cached.device_info.op_amps == 0;
    let ak_backup_fuse_stale = is_dspic33ak_part(part_number) && {
        let ficd_missing_bkbug = cached
            .fuse_defs
            .iter()
            .find(|register| register.cname == "FICD")
            .map(|register| {
                let has_jtagen = register.fields.iter().any(|field| field.cname == "JTAGEN");
                let has_bkbug = register.fields.iter().any(|field| field.cname == "BKBUG");
                has_jtagen && !has_bkbug
            })
            .unwrap_or(false);
        let fdevopt_missing_alti2c = cached
            .fuse_defs
            .iter()
            .find(|register| register.cname == "FDEVOPT")
            .map(|register| {
                let has_bistdis = register.fields.iter().any(|field| field.cname == "BISTDIS");
                let has_alti2c = register
                    .fields
                    .iter()
                    .any(|field| field.cname.starts_with("ALTI2C"));
                has_bistdis && cached.device_info.i2c > 0 && !has_alti2c
            })
            .unwrap_or(false);
        ficd_missing_bkbug || fdevopt_missing_alti2c
    };
    let clc_stale = cached.has_clc()
        && (cached.device_info.clc == 0 || cached.clc_module_id.is_none());

    fuse_stale
        || !has_clc_key
        || !has_device_info
        || pps_stale
        || ak_inventory_stale
        || ak_backup_fuse_stale
        || clc_stale
}

pub fn load_device(part_number: &str) -> Option<DeviceData> {
    let part_upper = part_number.to_uppercase();

    // Fast path: parsed JSON cache. Re-parse if the cache is stale:
    //   - empty fuse_defs: predates DCR parsing pass
    //   - any field with empty values: predates range-field filtering
    //   - empty PPS mappings despite remappable signals: predates PPS SFR parsing fixes
    //   - dsPIC33AK inventories with zero major peripherals: predates 32-bit SFR support
    //   - dsPIC33AK fuse caches missing backup-sector fields like ALTI2C1/BKBUG
    //   - caches that expose CLC PPS endpoints but still report no CLC inventory/module ID
    if let Some((mut cached, has_clc_key, has_device_info)) = get_cached_device(&part_upper) {
        if !cached_device_needs_reparse(&part_upper, &cached, has_clc_key, has_device_info) {
            dfp_store::load_pinout_overlays(&mut cached);
            dfp_store::load_clc_sources(&mut cached);
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
    dfp_store::load_pinout_overlays(&mut device);
    dfp_store::load_clc_sources(&mut device);

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

pub fn clc_sources_dir() -> PathBuf {
    dfp_store::clc_sources_dir_public()
}

/// Try to find a datasheet PDF locally. Searches:
/// 1. Fetcher's PDF cache (`dfp_cache/datasheets/pdf/`)
/// 2. Legacy datasheets dir (`dfp_cache/datasheets/`)
/// 3. `~/Downloads` (shallow) for PDFs matching the part number or family
///
/// Returns the path to the PDF if found, or None.
pub fn find_local_datasheet(part_number: &str) -> Option<PathBuf> {
    dfp_datasheet::find_local_datasheet(part_number)
}

/// Cache a datasheet PDF so future lookups find it without prompting.
pub fn cache_datasheet(part_number: &str, pdf_bytes: &[u8]) -> Option<PathBuf> {
    dfp_datasheet::cache_datasheet(part_number, pdf_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::datasheet_fetcher;
    use crate::parser::dfp_datasheet::{
        datasheet_family_markers, datasheet_part_series_marker, datasheet_part_suffix,
        datasheet_probe_matches_part, datasheet_probe_matches_resolved_reference,
    };
    use crate::parser::dfp_store::{
        apply_package_display_name_overrides, replace_redundant_default_pinout,
    };
    use crate::parser::edc_parser::{DeviceInfo, Pinout};
    use std::collections::HashMap;

    fn test_device(
        default_name: &str,
        default_pinout: Pinout,
        extra_pinouts: Vec<(String, Pinout)>,
    ) -> DeviceData {
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
            device_info: DeviceInfo::default(),
        }
    }

    #[test]
    fn datasheet_family_markers_include_exact_part_and_family_prefix() {
        assert_eq!(
            datasheet_family_markers("DSPIC33CDV128MC106"),
            vec!["DSPIC33CDV128MC106".to_string(), "DSPIC33CDV".to_string()]
        );
        assert_eq!(
            datasheet_family_markers("PIC24EP128MC206"),
            vec!["PIC24EP128MC206".to_string(), "PIC24EP".to_string()]
        );
    }

    #[test]
    fn datasheet_part_suffix_extracts_final_feature_code() {
        assert_eq!(
            datasheet_part_suffix("DSPIC33AK64MC105").as_deref(),
            Some("MC105")
        );
        assert_eq!(
            datasheet_part_suffix("DSPIC33CK64MP105").as_deref(),
            Some("MP105")
        );
    }

    #[test]
    fn datasheet_part_series_marker_keeps_the_family_level_feature_code() {
        assert_eq!(
            datasheet_part_series_marker("DSPIC33AK64MC105").as_deref(),
            Some("MC10")
        );
        assert_eq!(
            datasheet_part_series_marker("DSPIC33CK64MP105").as_deref(),
            Some("MP10")
        );
    }

    #[test]
    fn datasheet_probe_matching_accepts_exact_part_or_same_family_series() {
        assert!(datasheet_probe_matches_part(
            "dsPIC33AK64MC105 Family Data Sheet",
            "DSPIC33AK64MC105"
        ));
        assert!(datasheet_probe_matches_part(
            "dsPIC33CDVL64MC106 Family Data Sheet",
            "DSPIC33CDV128MC106"
        ));
        assert!(datasheet_probe_matches_part(
            "dsPIC33AK128MC106 Family Data Sheet",
            "DSPIC33AK64MC105"
        ));
        assert!(!datasheet_probe_matches_part(
            "dsPIC33AK128MP106 Family Data Sheet",
            "DSPIC33AK64MC105"
        ));
    }

    #[test]
    fn datasheet_probe_matching_accepts_resolved_family_pdf_by_ds_number() {
        let resolved = datasheet_fetcher::DatasheetRef {
            part_number: "DSPIC33AK256MPS205".to_string(),
            product_url: "https://www.microchip.com/en-us/product/dspic33ak256mps205".to_string(),
            datasheet_title: "dsPIC33AK512MPS512 Family Data Sheet".to_string(),
            datasheet_number: "DS70005591".to_string(),
            datasheet_revision: "DS70005591C".to_string(),
            pdf_url: "https://ww1.microchip.com/downloads/aemDocuments/documents/MCU16/ProductDocuments/DataSheets/dsPIC33AK512MPS512-Family-Data-Sheet-DS70005591.pdf".to_string(),
            sibling_source: None,
        };

        assert!(datasheet_probe_matches_resolved_reference(
            "dsPIC33AK512MPS512 Family Data Sheet\nDS70005591C",
            &resolved
        ));
        assert!(!datasheet_probe_matches_part(
            "dsPIC33AK512MPS512 Family Data Sheet",
            "DSPIC33AK256MPS205"
        ));
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
            display_name: None,
            pin_count: 2,
            source: "edc".to_string(),
            pins: HashMap::from([(1, "RA0".to_string()), (2, "RA1".to_string())]),
        };
        let real_pinout = Pinout {
            package: "64-Pin VQFN-TQFP".to_string(),
            display_name: None,
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
            display_name: None,
            pin_count: 2,
            source: "edc".to_string(),
            pins: HashMap::from([(1, "RA0".to_string()), (2, "RA1".to_string())]),
        };
        let real_pinout = Pinout {
            package: "64-Pin VQFN-TQFP".to_string(),
            display_name: None,
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

    #[test]
    fn package_display_name_overrides_apply_case_insensitively() {
        let default_pinout = Pinout {
            package: "STX04 (48-pin uQFN)".to_string(),
            display_name: None,
            pin_count: 48,
            source: "edc".to_string(),
            pins: HashMap::from([(1, "RA0".to_string())]),
        };
        let mut device = test_device("STX04 (48-pin uQFN)", default_pinout, Vec::new());

        apply_package_display_name_overrides(
            &mut device,
            &HashMap::from([("stx04 (48-pin uqfn)".to_string(), "48-PIN VQFN".to_string())]),
        );

        assert_eq!(
            device
                .pinouts
                .get("STX04 (48-pin uQFN)")
                .and_then(|pinout| pinout.display_name.as_deref()),
            Some("48-PIN VQFN")
        );
    }

    #[test]
    fn cache_reparse_detects_missing_pps_matrix() {
        let default_pinout = Pinout {
            package: "default".to_string(),
            display_name: None,
            pin_count: 1,
            source: "edc".to_string(),
            pins: HashMap::from([(1, "RA0".to_string())]),
        };
        let mut device = test_device("default", default_pinout, Vec::new());
        device
            .remappable_inputs
            .push(crate::parser::edc_parser::RemappablePeripheral {
                name: "U1RX".to_string(),
                direction: "in".to_string(),
                ppsval: None,
            });

        assert!(cached_device_needs_reparse(
            "DSPIC33CK64MP102",
            &device,
            true,
            true
        ));
    }

    #[test]
    fn cache_reparse_detects_sparse_dspic33ak_inventory() {
        let default_pinout = Pinout {
            package: "default".to_string(),
            display_name: None,
            pin_count: 1,
            source: "edc".to_string(),
            pins: HashMap::from([(1, "RA0".to_string())]),
        };
        let mut device = test_device("default", default_pinout, Vec::new());
        device
            .fuse_defs
            .push(crate::parser::edc_parser::DcrRegister {
                cname: "FICD".to_string(),
                desc: String::new(),
                addr: 0,
                default_value: 0,
                fields: vec![crate::parser::edc_parser::DcrField {
                    cname: "ICS".to_string(),
                    desc: String::new(),
                    mask: 1,
                    width: 1,
                    hidden: false,
                    values: vec![crate::parser::edc_parser::DcrFieldValue {
                        cname: "PGD1".to_string(),
                        desc: String::new(),
                        value: 0,
                    }],
                }],
            });

        assert!(cached_device_needs_reparse(
            "DSPIC33AK64MC105",
            &device,
            true,
            true
        ));
        assert!(!cached_device_needs_reparse(
            "DSPIC33CK64MP102",
            &device,
            true,
            true
        ));
    }

    #[test]
    fn cache_reparse_detects_missing_ak_backup_fuse_fields() {
        let default_pinout = Pinout {
            package: "default".to_string(),
            display_name: None,
            pin_count: 1,
            source: "edc".to_string(),
            pins: HashMap::from([(1, "RA0".to_string())]),
        };
        let mut device = test_device("default", default_pinout, Vec::new());
        device.device_info.i2c = 3;
        device.fuse_defs = vec![
            crate::parser::edc_parser::DcrRegister {
                cname: "FICD".to_string(),
                desc: String::new(),
                addr: 0,
                default_value: 0,
                fields: vec![crate::parser::edc_parser::DcrField {
                    cname: "JTAGEN".to_string(),
                    desc: String::new(),
                    mask: 1,
                    width: 1,
                    hidden: false,
                    values: vec![crate::parser::edc_parser::DcrFieldValue {
                        cname: "ON".to_string(),
                        desc: String::new(),
                        value: 1,
                    }],
                }],
            },
            crate::parser::edc_parser::DcrRegister {
                cname: "FDEVOPT".to_string(),
                desc: String::new(),
                addr: 0,
                default_value: 0,
                fields: vec![crate::parser::edc_parser::DcrField {
                    cname: "BISTDIS".to_string(),
                    desc: String::new(),
                    mask: 1,
                    width: 1,
                    hidden: false,
                    values: vec![crate::parser::edc_parser::DcrFieldValue {
                        cname: "OFF".to_string(),
                        desc: String::new(),
                        value: 1,
                    }],
                }],
            },
        ];

        assert!(cached_device_needs_reparse(
            "DSPIC33AK256MPS205",
            &device,
            true,
            true
        ));

        device.fuse_defs[0]
            .fields
            .push(crate::parser::edc_parser::DcrField {
                cname: "BKBUG".to_string(),
                desc: String::new(),
                mask: 2,
                width: 1,
                hidden: false,
                values: vec![crate::parser::edc_parser::DcrFieldValue {
                    cname: "OFF".to_string(),
                    desc: String::new(),
                    value: 1,
                }],
            });
        device.fuse_defs[1]
            .fields
            .push(crate::parser::edc_parser::DcrField {
                cname: "ALTI2C1".to_string(),
                desc: String::new(),
                mask: 4,
                width: 1,
                hidden: false,
                values: vec![crate::parser::edc_parser::DcrFieldValue {
                    cname: "OFF".to_string(),
                    desc: String::new(),
                    value: 1,
                }],
            });

        assert!(!cached_device_needs_reparse(
            "DSPIC33AK256MPS205",
            &device,
            true,
            true
        ));
    }

    #[test]
    fn cache_reparse_detects_missing_clc_metadata_when_clc_signals_exist() {
        let default_pinout = Pinout {
            package: "default".to_string(),
            display_name: None,
            pin_count: 48,
            source: "edc".to_string(),
            pins: HashMap::new(),
        };
        let mut device = test_device("default", default_pinout, Vec::new());
        device.part_number = "DSPIC33AK256MPS205".to_string();
        device.fuse_defs = vec![crate::parser::edc_parser::DcrRegister {
            cname: "FICD".to_string(),
            desc: String::new(),
            addr: 0,
            default_value: 0,
            fields: vec![crate::parser::edc_parser::DcrField {
                cname: "JTAGEN".to_string(),
                desc: String::new(),
                mask: 1,
                width: 1,
                hidden: false,
                values: vec![crate::parser::edc_parser::DcrFieldValue {
                    cname: "ON".to_string(),
                    desc: String::new(),
                    value: 1,
                }],
            }, crate::parser::edc_parser::DcrField {
                cname: "BKBUG".to_string(),
                desc: String::new(),
                mask: 2,
                width: 1,
                hidden: false,
                values: vec![crate::parser::edc_parser::DcrFieldValue {
                    cname: "OFF".to_string(),
                    desc: String::new(),
                    value: 1,
                }],
            }],
        }];
        device
            .remappable_inputs
            .push(crate::parser::edc_parser::RemappablePeripheral {
                name: "CLCINA".to_string(),
                direction: "in".to_string(),
                ppsval: None,
            });
        device
            .remappable_outputs
            .push(crate::parser::edc_parser::RemappablePeripheral {
                name: "CLC1OUT".to_string(),
                direction: "out".to_string(),
                ppsval: Some(69),
            });

        assert!(cached_device_needs_reparse(
            "DSPIC33AK256MPS205",
            &device,
            true,
            true
        ));

        device.device_info.clc = 10;
        device.clc_module_id = Some("DOS-01577_cla_clc_upb_v1_dspic33a.Module".to_string());
        device
            .pps_input_mappings
            .push(crate::parser::edc_parser::PPSInputMapping {
                peripheral: "CLCINA".to_string(),
                register: "RPINR20".to_string(),
                register_addr: 0,
                field_name: "CLCINAR".to_string(),
                field_mask: 0xff,
                field_offset: 0,
            });
        device
            .pps_output_mappings
            .push(crate::parser::edc_parser::PPSOutputMapping {
                rp_number: 1,
                register: "RPOR0".to_string(),
                register_addr: 0,
                field_name: "RP1R".to_string(),
                field_mask: 0x7f,
                field_offset: 0,
            });

        assert!(!cached_device_needs_reparse(
            "DSPIC33AK256MPS205",
            &device,
            true,
            true
        ));
    }
}
