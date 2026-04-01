//! Datasheet Fetcher: resolves and downloads Microchip datasheets by part number.
//!
//! The algorithm mirrors the Python `config-pic` implementation:
//! 1. Check metadata cache for a previously resolved datasheet reference
//! 2. Build the Microchip product page URL from the part number
//! 3. Fetch the product page via a text proxy (Jina) to bypass Akamai CDN blocking
//! 4. Extract the datasheet title and DS document number
//! 5. Generate candidate PDF URLs with revision letter search (Z→A)
//! 6. Validate candidates via proxy preview
//! 7. Download the real PDF (or fall back to proxy-extracted text)
//!
//! All resolved metadata, PDFs, and text are cached under `dfp_cache/datasheets/`.

use regex::Regex;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use crate::parser::dfp_manager::dfp_cache_dir;

const TEXT_PROXY_PREFIX: &str = "https://r.jina.ai/http://";

// ---------------------------------------------------------------------------
// Cache paths
// ---------------------------------------------------------------------------

fn meta_dir() -> PathBuf {
    let d = dfp_cache_dir().join("datasheets").join("meta");
    let _ = fs::create_dir_all(&d);
    d
}

fn pdf_dir() -> PathBuf {
    let d = dfp_cache_dir().join("datasheets").join("pdf");
    let _ = fs::create_dir_all(&d);
    d
}

fn text_dir() -> PathBuf {
    let d = dfp_cache_dir().join("datasheets").join("text");
    let _ = fs::create_dir_all(&d);
    d
}

fn meta_path(part: &str) -> PathBuf {
    meta_dir().join(format!("{}.json", part.to_uppercase()))
}

fn pdf_path(part: &str) -> PathBuf {
    pdf_dir().join(format!("{}.pdf", part.to_uppercase()))
}

fn text_path(part: &str) -> PathBuf {
    text_dir().join(format!("{}.md", part.to_uppercase()))
}

// ---------------------------------------------------------------------------
// Metadata
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasheetRef {
    pub part_number: String,
    pub product_url: String,
    pub datasheet_title: String,
    pub datasheet_number: String,   // e.g. DS70005363
    pub datasheet_revision: String, // e.g. DS70005363E
    pub pdf_url: String,
}

fn save_metadata(r: &DatasheetRef) {
    let path = meta_path(&r.part_number);
    let _ = fs::write(&path, serde_json::to_string_pretty(r).unwrap_or_default());
}

fn load_metadata(part: &str) -> Option<DatasheetRef> {
    let path = meta_path(part);
    let text = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

fn http_client() -> Client {
    Client::builder()
        .user_agent("Mozilla/5.0 (compatible; pickle/0.1)")
        .build()
        .unwrap_or_else(|_| Client::new())
}

fn proxy_url(url: &str) -> String {
    format!("{}{}", TEXT_PROXY_PREFIX, url)
}

fn fetch_text(url: &str, timeout_secs: u64) -> Result<String, String> {
    let client = http_client();
    let resp = client
        .get(url)
        .timeout(Duration::from_secs(timeout_secs))
        .send()
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    resp.text().map_err(|e| format!("Read body failed: {e}"))
}

fn download_bytes(url: &str, timeout_secs: u64) -> Result<Vec<u8>, String> {
    let client = http_client();
    let resp = client
        .get(url)
        .timeout(Duration::from_secs(timeout_secs))
        .send()
        .map_err(|e| format!("Download failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    resp.bytes()
        .map(|b| b.to_vec())
        .map_err(|e| format!("Read bytes failed: {e}"))
}

// ---------------------------------------------------------------------------
// Product page parsing
// ---------------------------------------------------------------------------

fn product_url_for_part(part: &str) -> String {
    format!(
        "https://www.microchip.com/en-us/product/{}",
        part.to_lowercase()
    )
}

/// Extract (datasheet_title, ds_number) from a Microchip product page's text.
fn parse_product_page(markdown: &str) -> Result<(String, String), String> {
    // Try primary pattern: "Data Sheet:\n(optional PDF\n)title"
    let re_title = Regex::new(r"(?i)Data Sheet:\s*\n(?:PDF\s*\n)?([^\n]+)").unwrap();
    let title = if let Some(cap) = re_title.captures(markdown) {
        cap[1].trim().to_string()
    } else {
        // Fallback: "title\n  Data Sheets"
        let re_fb = Regex::new(r"(?i)\n([^\n]*Data Sheet[^\n]*)\n\s*Data Sheets\b").unwrap();
        re_fb
            .captures(markdown)
            .map(|c| c[1].trim().to_string())
            .ok_or("Could not locate datasheet title on product page")?
    };

    // Extract DS number (DS followed by 8 digits)
    let re_ds = Regex::new(r"(?i)\b(DS\d{8})\b").unwrap();
    let ds_number = re_ds
        .captures(markdown)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_uppercase())
        .ok_or("Could not locate datasheet document number on product page")?;

    Ok((title, ds_number))
}

// ---------------------------------------------------------------------------
// Candidate URL generation
// ---------------------------------------------------------------------------

fn document_root_for_part(part: &str) -> &'static str {
    let p = part.to_uppercase();
    if p.starts_with("DSPIC33") || p.starts_with("PIC24") {
        "MCU16"
    } else if p.starts_with("PIC32") {
        "MCU32"
    } else if p.starts_with("PIC10")
        || p.starts_with("PIC12")
        || p.starts_with("PIC16")
        || p.starts_with("PIC18")
    {
        "MCU8"
    } else {
        "MCU16"
    }
}

fn slugify_title(title: &str) -> String {
    let re = Regex::new(r"[^A-Za-z0-9]+").unwrap();
    let slug = re.replace_all(title, "-");
    let slug = slug.trim_matches('-');
    let re2 = Regex::new(r"-{2,}").unwrap();
    re2.replace_all(slug, "-").to_string()
}

/// Generate candidate revision IDs: hinted first, then recent revisions
/// down to older ones. Most Microchip datasheets are in the A–H range,
/// so we search a small high-probability window first, then widen only
/// if we don't find a hit.
fn candidate_revisions(base_number: &str, hint: Option<&str>) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut add = |val: &str| {
        let upper = val.to_uppercase();
        if seen.insert(upper.clone()) {
            candidates.push(upper);
        }
    };

    if let Some(h) = hint {
        add(h);
    }

    // Most datasheets are in A–H range; try recent-first then older
    for letter in ('A'..='H').rev() {
        add(&format!("{}{}", base_number, letter));
    }
    // Extend to I–Z only if the common range didn't hit
    for letter in ('I'..='Z').rev() {
        add(&format!("{}{}", base_number, letter));
    }

    candidates
}

fn build_candidate_pdf_url(part: &str, title: &str, ds_id: &str) -> String {
    let root = document_root_for_part(part);
    let slug = slugify_title(title);
    format!(
        "https://ww1.microchip.com/downloads/aemDocuments/documents/{}/ProductDocuments/DataSheets/{}-{}.pdf",
        root, slug, ds_id
    )
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn looks_like_datasheet_preview(preview: &str, expected_title: &str, ds_id: &str) -> bool {
    let lower = preview.to_lowercase();

    // Reject Jina error / generic Microchip page
    if lower.contains("title: empowering innovation") {
        return false;
    }

    // Check metadata title match
    let re_title = Regex::new(r"(?mi)^Title:\s*(.+)$").unwrap();
    if let Some(cap) = re_title.captures(preview) {
        if cap[1].to_lowercase().contains(&expected_title.to_lowercase()) {
            return true;
        }
    }

    // Fallback: title present AND page markers exist
    lower.contains(&expected_title.to_lowercase())
        && (lower.contains("number of pages:")
            || lower.contains(&format!("{}-page", ds_id.to_lowercase())))
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Resolve the datasheet reference for a part number. Uses cache, then
/// fetches the Microchip product page via proxy to discover the PDF URL.
pub fn resolve(part_number: &str) -> Result<DatasheetRef, String> {
    let part_upper = part_number.to_uppercase();

    // 1. Check metadata cache
    if let Some(cached) = load_metadata(&part_upper) {
        return Ok(cached);
    }

    // 2. Fetch product page via Jina proxy
    let product_url = product_url_for_part(&part_upper);
    let proxied = proxy_url(&product_url);
    let page_text = fetch_text(&proxied, 30)?;

    // 3. Parse title and DS number
    let (title, ds_number) = parse_product_page(&page_text)?;

    // 4. Try candidate revisions — high-probability first (H→A), then I→Z.
    //    Use a short timeout per probe to avoid hanging on slow proxy responses.
    let revisions = candidate_revisions(&ds_number, None);
    log::info!("resolve: trying {} candidate revisions for {}", revisions.len(), part_upper);

    for (i, rev) in revisions.iter().enumerate() {
        let pdf_url = build_candidate_pdf_url(&part_upper, &title, rev);
        let proxied_pdf = proxy_url(&pdf_url);

        // Quick probe with short timeout — most misses fail fast
        let preview = match fetch_text(&proxied_pdf, 8) {
            Ok(t) => t,
            Err(_) => continue,
        };

        let preview_short = if preview.len() > 2000 {
            &preview[..2000]
        } else {
            &preview
        };

        if looks_like_datasheet_preview(preview_short, &title, rev) {
            log::info!("resolve: found {} after {} probes", rev, i + 1);
            let ds_ref = DatasheetRef {
                part_number: part_upper.clone(),
                product_url: product_url.clone(),
                datasheet_title: title.clone(),
                datasheet_number: ds_number.clone(),
                datasheet_revision: rev.clone(),
                pdf_url: pdf_url.clone(),
            };
            save_metadata(&ds_ref);
            return Ok(ds_ref);
        }
    }

    Err(format!(
        "Could not resolve datasheet for {} (tried {} revisions)",
        part_upper,
        revisions.len()
    ))
}

/// Get or download the actual PDF bytes.
pub fn get_or_download_pdf(part_number: &str, pdf_url: &str) -> Result<Vec<u8>, String> {
    let path = pdf_path(part_number);
    if path.exists() {
        return fs::read(&path).map_err(|e| format!("Read cached PDF: {e}"));
    }

    let data = download_bytes(pdf_url, 90)?;
    if !data.starts_with(b"%PDF") {
        return Err("Resolved URL did not return a PDF".to_string());
    }

    let _ = fs::write(&path, &data);
    Ok(data)
}

/// Get or fetch proxy-extracted text as a fallback when PDF download is blocked.
pub fn get_or_fetch_text(part_number: &str, pdf_url: &str) -> Result<String, String> {
    let path = text_path(part_number);
    if path.exists() {
        return fs::read_to_string(&path).map_err(|e| format!("Read cached text: {e}"));
    }

    let text = fetch_text(&proxy_url(pdf_url), 120)?;
    let lower = text.to_lowercase();
    if !lower.contains("number of pages:") && !lower.contains("-page ") {
        return Err("Datasheet text extraction did not look valid".to_string());
    }

    let _ = fs::write(&path, &text);
    Ok(text)
}
