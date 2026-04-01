//! Pack Index Manager: fetch, parse, and cache the Microchip pack repository index.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::parser::dfp_manager::dfp_cache_dir;

const INDEX_URL: &str = "https://packs.download.microchip.com/index.idx";
const PACK_BASE_URL: &str = "https://packs.download.microchip.com/";
const STALE_SECONDS: u64 = 7 * 24 * 3600;
const ATMEL_NS: &str = "http://packs.download.atmel.com/pack-idx-atmel-extension";
const RELEVANT_PREFIXES: &[&str] = &["dsPIC33", "PIC24"];

fn index_cache_file() -> PathBuf {
    dfp_cache_dir().join("pack_index.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackInfo {
    pub name: String,
    pub version: String,
    pub pdsc_name: String,
}

impl PackInfo {
    pub fn atpack_url(&self) -> String {
        format!(
            "{}Microchip.{}.{}.atpack",
            PACK_BASE_URL, self.name, self.version
        )
    }

    pub fn atpack_filename(&self) -> String {
        format!("Microchip.{}.{}.atpack", self.name, self.version)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceEntry {
    pub name: String,
    pub family: String,
    pub pack_name: String,
    pub pack_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackIndex {
    pub fetched_at: f64,
    pub packs: HashMap<String, PackInfo>,
    pub devices: HashMap<String, DeviceEntry>,
}

impl PackIndex {
    pub fn is_stale(&self) -> bool {
        if self.fetched_at == 0.0 {
            return true;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        (now - self.fetched_at) > STALE_SECONDS as f64
    }

    pub fn age_hours(&self) -> f64 {
        if self.fetched_at == 0.0 {
            return f64::INFINITY;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        (now - self.fetched_at) / 3600.0
    }
}

fn fetch_index_xml() -> Result<Vec<u8>, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("pickle/0.1 (pin configurator)")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp = client
        .get(INDEX_URL)
        .send()
        .map_err(|e| format!("Fetch error: {e}"))?;

    resp.bytes()
        .map(|b| b.to_vec())
        .map_err(|e| format!("Read error: {e}"))
}

fn parse_index_xml(xml_bytes: &[u8]) -> Result<PackIndex, String> {
    let xml_str = std::str::from_utf8(xml_bytes).map_err(|e| format!("UTF-8 error: {e}"))?;
    let doc = roxmltree::Document::parse(xml_str).map_err(|e| format!("XML parse error: {e}"))?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();

    let mut index = PackIndex {
        fetched_at: now,
        packs: HashMap::new(),
        devices: HashMap::new(),
    };

    for pdsc in doc
        .root_element()
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "pdsc")
    {
        let pack_name = pdsc
            .attribute((ATMEL_NS, "name"))
            .or_else(|| pdsc.attribute("name"))
            .unwrap_or("")
            .to_string();

        if pack_name.is_empty() {
            continue;
        }

        if !RELEVANT_PREFIXES.iter().any(|p| pack_name.starts_with(p)) {
            continue;
        }

        let version = pdsc.attribute("version").unwrap_or("").to_string();
        let pdsc_name = pdsc.attribute("name").unwrap_or("").to_string();

        index.packs.insert(
            pack_name.clone(),
            PackInfo {
                name: pack_name.clone(),
                version: version.clone(),
                pdsc_name,
            },
        );

        // Extract device names
        for device_el in pdsc
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "device")
        {
            let dev_name = device_el.attribute("name").unwrap_or("").to_string();
            let dev_family = device_el.attribute("family").unwrap_or("").to_string();
            if !dev_name.is_empty() {
                let key = dev_name.to_uppercase();
                index.devices.insert(
                    key,
                    DeviceEntry {
                        name: dev_name,
                        family: dev_family,
                        pack_name: pack_name.clone(),
                        pack_version: version.clone(),
                    },
                );
            }
        }
    }

    Ok(index)
}

fn load_cached_index() -> Option<PackIndex> {
    let path = index_cache_file();
    if !path.exists() {
        return None;
    }
    let text = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_cached_index(index: &PackIndex) {
    let path = index_cache_file();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(index) {
        let _ = fs::write(&path, json);
    }
}

pub fn get_pack_index(force_refresh: bool) -> Result<PackIndex, String> {
    if !force_refresh {
        if let Some(cached) = load_cached_index() {
            if !cached.is_stale() {
                return Ok(cached);
            }
        }
    }

    match fetch_index_xml() {
        Ok(xml_bytes) => {
            let index = parse_index_xml(&xml_bytes)?;
            save_cached_index(&index);
            Ok(index)
        }
        Err(e) => {
            if let Some(cached) = load_cached_index() {
                eprintln!(
                    "Warning: pack index fetch failed ({}), using stale cache",
                    e
                );
                Ok(cached)
            } else {
                Err(format!(
                    "Cannot fetch pack index and no cache available: {}",
                    e
                ))
            }
        }
    }
}

pub fn lookup_device_pack(part_number: &str) -> Option<(String, String)> {
    let index = get_pack_index(false).ok()?;
    let key = part_number.to_uppercase();
    let entry = index.devices.get(&key)?;
    let pack = index.packs.get(&entry.pack_name)?;
    Some((pack.atpack_url(), pack.atpack_filename()))
}

pub fn list_all_devices() -> Vec<String> {
    match get_pack_index(false) {
        Ok(index) => index.devices.keys().cloned().collect(),
        Err(_) => Vec::new(),
    }
}

pub fn download_atpack(url: &str, filename: &str) -> Result<PathBuf, String> {
    let cache_dir = dfp_cache_dir();
    let _ = fs::create_dir_all(&cache_dir);
    let dest = cache_dir.join(filename);

    if dest.exists() {
        return Ok(dest);
    }

    eprintln!("Downloading {}...", filename);

    let client = reqwest::blocking::Client::builder()
        .user_agent("pickle/0.1 (pin configurator)")
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp = client
        .get(url)
        .send()
        .map_err(|e| format!("Download error: {e}"))?;

    let bytes = resp.bytes().map_err(|e| format!("Read error: {e}"))?;

    fs::write(&dest, &bytes).map_err(|e| format!("Write error: {e}"))?;

    eprintln!(
        "Downloaded {} ({:.1} MB)",
        filename,
        bytes.len() as f64 / 1_048_576.0
    );

    Ok(dest)
}
