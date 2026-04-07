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
    /// When the datasheet was resolved from a sibling product page (e.g.
    /// dsPIC33CDVL64MC106 for a dsPIC33CDV128MC106 query), this field
    /// records the sibling part slug so the UI can inform the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sibling_source: Option<String>,
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

/// Return cached datasheet metadata for a part without triggering any network
/// lookup. Validation code uses this after a successful `resolve()` call so a
/// downloaded family datasheet can be accepted by its authoritative DS number.
pub fn cached_metadata(part: &str) -> Option<DatasheetRef> {
    load_metadata(part)
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

fn sibling_family_prefix(part: &str) -> Option<String> {
    let re = Regex::new(r"^((?:DSPIC|PIC)\d+[A-Z]+)").unwrap();
    re.captures(&part.to_uppercase())
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

fn sibling_pin_suffix(part: &str) -> Option<String> {
    let re = Regex::new(r"([A-Z]{2,}\d{3}[A-Z]?)$").unwrap();
    re.captures(&part.to_uppercase())
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

fn is_compatible_sibling_part(requested_part: &str, sibling_part: &str) -> bool {
    let requested_upper = requested_part.to_uppercase();
    let sibling_upper = sibling_part.to_uppercase();

    if sibling_upper == requested_upper {
        return true;
    }

    sibling_family_prefix(&requested_upper)
        .zip(sibling_pin_suffix(&requested_upper))
        .map(|(family, suffix)| sibling_upper.contains(&family) && sibling_upper.contains(&suffix))
        .unwrap_or(false)
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

fn parse_direct_pdf_url(markdown: &str) -> Option<String> {
    let re_pdf = Regex::new(
        r#"(?i)\[PDF\]\((https://ww1\.microchip\.com/downloads/aemDocuments/documents/[^\s)]+\.pdf)\)"#,
    )
    .unwrap();
    re_pdf
        .captures(markdown)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())
}

fn revision_from_pdf_url(pdf_url: &str) -> Option<String> {
    // Match with revision letter (DS70000657J) or bare number (DS70005539)
    let re_rev = Regex::new(r"(?i)\b(DS\d{8}[A-Z]?)\.pdf\b").unwrap();
    re_rev
        .captures(pdf_url)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_uppercase())
}

fn datasheet_number_from_revision(revision: &str) -> Option<String> {
    // Strip trailing revision letter if present; bare DS number is also valid
    let re_number = Regex::new(r"(?i)\b(DS\d{8})[A-Z]?\b").unwrap();
    re_number
        .captures(revision)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_uppercase())
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
/// if we don't find a hit. Some datasheets have no revision letter at all
/// (e.g. DS70005539.pdf), so the bare number is always included.
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

    // Some datasheets have no revision letter (e.g. DS70005539)
    add(base_number);

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
// Direct URL probing (bypasses Jina proxy for validation)
// ---------------------------------------------------------------------------

/// Check whether a PDF URL exists on the Microchip CDN via a HEAD request.
/// This is much faster and more reliable than routing through the Jina proxy.
fn probe_pdf_url(url: &str) -> bool {
    let client = http_client();
    match client.head(url).timeout(Duration::from_secs(10)).send() {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                // Verify it's actually a PDF (not a redirect to an error page)
                if let Some(ct) = resp.headers().get("content-type") {
                    return ct.to_str().unwrap_or("").contains("pdf");
                }
                // No content-type header but 200 — assume valid
                return true;
            }
            false
        }
        Err(_) => false,
    }
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
        if cap[1]
            .to_lowercase()
            .contains(&expected_title.to_lowercase())
        {
            return true;
        }
    }

    // Fallback: title present AND page markers exist
    lower.contains(&expected_title.to_lowercase())
        && (lower.contains("number of pages:")
            || lower.contains(&format!("{}-page", ds_id.to_lowercase())))
}

// ---------------------------------------------------------------------------
// Sibling product page discovery via DuckDuckGo
// ---------------------------------------------------------------------------

const DDG_HTML_URL: &str = "https://html.duckduckgo.com/html/";

/// Search DuckDuckGo for sibling Microchip product pages that may share the
/// same family datasheet.  Returns up to `limit` product page URLs.
fn search_sibling_product_pages(part: &str, limit: usize) -> Vec<String> {
    let query = format!("{} datasheet site:microchip.com/en-us/product", part);
    let client = http_client();
    let resp = match client
        .post(DDG_HTML_URL)
        .form(&[("q", query.as_str())])
        .timeout(Duration::from_secs(15))
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!("sibling search failed: {e}");
            return Vec::new();
        }
    };

    let body = match resp.text() {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };

    // Extract Microchip product URLs from DuckDuckGo result links
    let re = Regex::new(r"(?i)https?://www\.microchip\.com/en[/-]us/product/([a-z0-9]+)").unwrap();

    let mut seen = std::collections::HashSet::new();
    let part_lower = part.to_lowercase();
    let mut urls = Vec::new();

    for cap in re.captures_iter(&body) {
        let slug = cap[1].to_lowercase();
        // Skip the exact part we already tried
        if slug == part_lower {
            continue;
        }
        if seen.insert(slug.clone()) {
            urls.push(format!("https://www.microchip.com/en-us/product/{}", slug));
            if urls.len() >= limit {
                break;
            }
        }
    }

    urls
}

// ---------------------------------------------------------------------------
// Core resolution from a rendered product page
// ---------------------------------------------------------------------------

/// Try to build a DatasheetRef from rendered product page markdown.  Returns
/// `None` if the page didn't contain parseable datasheet information.
fn try_resolve_from_page(
    part_upper: &str,
    product_url: &str,
    page_text: &str,
) -> Option<DatasheetRef> {
    let (title, ds_number) = match parse_product_page(page_text) {
        Ok(v) => v,
        Err(_) => return None,
    };

    // Prefer the exact PDF URL already linked from the product page
    if let Some(pdf_url) = parse_direct_pdf_url(page_text) {
        let datasheet_revision =
            revision_from_pdf_url(&pdf_url).unwrap_or_else(|| ds_number.clone());
        let datasheet_number =
            datasheet_number_from_revision(&datasheet_revision).unwrap_or(ds_number.clone());

        if probe_pdf_url(&pdf_url) {
            log::info!(
                "resolve: found direct datasheet URL for {} ({})",
                part_upper,
                datasheet_revision
            );
            return Some(DatasheetRef {
                part_number: part_upper.to_string(),
                product_url: product_url.to_string(),
                datasheet_title: title,
                datasheet_number,
                datasheet_revision,
                pdf_url,
                sibling_source: None,
            });
        }
        log::warn!("resolve: direct PDF URL returned non-200: {}", pdf_url);
    }

    // Try candidate revisions via HEAD probes
    let revisions = candidate_revisions(&ds_number, None);
    log::info!(
        "resolve: trying {} candidate revisions for {}",
        revisions.len(),
        part_upper
    );

    for (i, rev) in revisions.iter().enumerate() {
        let pdf_url = build_candidate_pdf_url(part_upper, &title, rev);
        if probe_pdf_url(&pdf_url) {
            log::info!("resolve: HEAD hit for {} after {} probes", rev, i + 1);
            return Some(DatasheetRef {
                part_number: part_upper.to_string(),
                product_url: product_url.to_string(),
                datasheet_title: title.clone(),
                datasheet_number: ds_number.clone(),
                datasheet_revision: rev.clone(),
                pdf_url,
                sibling_source: None,
            });
        }
    }

    // Fallback: proxy-based preview validation
    for (i, rev) in revisions.iter().enumerate() {
        let pdf_url = build_candidate_pdf_url(part_upper, &title, rev);
        let proxied_pdf = proxy_url(&pdf_url);

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
            log::info!("resolve: proxy hit for {} after {} probes", rev, i + 1);
            return Some(DatasheetRef {
                part_number: part_upper.to_string(),
                product_url: product_url.to_string(),
                datasheet_title: title.clone(),
                datasheet_number: ds_number.clone(),
                datasheet_revision: rev.clone(),
                pdf_url,
                sibling_source: None,
            });
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Resolve the datasheet reference for a part number.  Uses cache, then
/// fetches the Microchip product page via proxy to discover the PDF URL.
/// If the primary product page fails to render (common for JS-heavy pages),
/// searches for sibling product pages from the same device family.
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

    // 3. Try to resolve from the primary product page
    if let Some(mut ds_ref) = try_resolve_from_page(&part_upper, &product_url, &page_text) {
        ds_ref.part_number = part_upper.clone();
        save_metadata(&ds_ref);
        return Ok(ds_ref);
    }

    // 4. Primary page didn't render (JS-only shell).  Search DuckDuckGo for
    //    sibling product pages from the same family — those often share the
    //    same family datasheet PDF.
    log::info!(
        "resolve: primary product page for {} did not contain datasheet info, trying siblings",
        part_upper
    );
    let siblings = search_sibling_product_pages(&part_upper, 5);
    log::info!("resolve: found {} sibling product pages", siblings.len());

    for sibling_url in &siblings {
        let sibling_slug = sibling_url
            .rsplit('/')
            .next()
            .unwrap_or(sibling_url)
            .to_string();
        if !is_compatible_sibling_part(&part_upper, &sibling_slug) {
            log::info!(
                "resolve: skipping incompatible sibling {} for {}",
                sibling_slug,
                part_upper
            );
            continue;
        }

        let proxied_sibling = proxy_url(sibling_url);
        let sibling_text = match fetch_text(&proxied_sibling, 30) {
            Ok(t) => t,
            Err(e) => {
                log::warn!("resolve: sibling page fetch failed: {e}");
                continue;
            }
        };

        if let Some(mut ds_ref) = try_resolve_from_page(&part_upper, sibling_url, &sibling_text) {
            log::info!(
                "resolve: using sibling {} datasheet for {}",
                sibling_slug,
                part_upper
            );
            ds_ref.part_number = part_upper.clone();
            ds_ref.sibling_source = Some(sibling_slug);
            save_metadata(&ds_ref);
            return Ok(ds_ref);
        }
    }

    Err(format!(
        "Could not resolve datasheet for {} (primary page unrenderable, tried {} siblings)",
        part_upper,
        siblings.len()
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

#[cfg(test)]
mod tests {
    use super::{
        candidate_revisions, datasheet_number_from_revision, is_compatible_sibling_part,
        parse_direct_pdf_url, revision_from_pdf_url,
    };

    #[test]
    fn extracts_direct_pdf_url_from_product_page_markdown() {
        let markdown = r#"
Data Sheet:

[PDF](https://ww1.microchip.com/downloads/aemDocuments/documents/MCU16/ProductDocuments/DataSheets/dsPIC33EPXXXGP50X-dsPIC33EPXXXMC20X-50X-and-PIC24EPXXXGP-MC20X-Family-Data-Sheet-DS70000657J.pdf)

[dsPIC33EPXXXGP50X, dsPIC33EPXXXMC20X/50X, and PIC24EPXXXGP/MC20X Data Sheet](https://ww1.microchip.com/downloads/aemDocuments/documents/MCU16/ProductDocuments/DataSheets/dsPIC33EPXXXGP50X-dsPIC33EPXXXMC20X-50X-and-PIC24EPXXXGP-MC20X-Family-Data-Sheet-DS70000657J.pdf)
"#;

        assert_eq!(
            parse_direct_pdf_url(markdown).as_deref(),
            Some("https://ww1.microchip.com/downloads/aemDocuments/documents/MCU16/ProductDocuments/DataSheets/dsPIC33EPXXXGP50X-dsPIC33EPXXXMC20X-50X-and-PIC24EPXXXGP-MC20X-Family-Data-Sheet-DS70000657J.pdf")
        );
    }

    #[test]
    fn extracts_revision_from_pdf_url_with_letter() {
        let pdf_url = "https://ww1.microchip.com/downloads/aemDocuments/documents/MCU16/ProductDocuments/DataSheets/dsPIC33EPXXXGP50X-dsPIC33EPXXXMC20X-50X-and-PIC24EPXXXGP-MC20X-Family-Data-Sheet-DS70000657J.pdf";
        assert_eq!(
            revision_from_pdf_url(pdf_url).as_deref(),
            Some("DS70000657J")
        );
    }

    #[test]
    fn extracts_revision_from_pdf_url_without_letter() {
        let pdf_url = "https://ww1.microchip.com/downloads/aemDocuments/documents/MCU16/ProductDocuments/DataSheets/dsPIC33AK128MC106-Family-Data-Sheet-DS70005539.pdf";
        assert_eq!(
            revision_from_pdf_url(pdf_url).as_deref(),
            Some("DS70005539")
        );
    }

    #[test]
    fn extracts_datasheet_number_from_revision_with_letter() {
        assert_eq!(
            datasheet_number_from_revision("DS70000657J").as_deref(),
            Some("DS70000657")
        );
    }

    #[test]
    fn extracts_datasheet_number_from_bare_ds_number() {
        assert_eq!(
            datasheet_number_from_revision("DS70005539").as_deref(),
            Some("DS70005539")
        );
    }

    #[test]
    fn candidate_revisions_includes_bare_number() {
        let candidates = candidate_revisions("DS70005539", None);
        assert!(candidates.contains(&"DS70005539".to_string()));
        // Bare number should be last (after all lettered variants)
        assert_eq!(candidates.last().unwrap(), "DS70005539");
    }

    #[test]
    fn candidate_revisions_respects_hint() {
        let candidates = candidate_revisions("DS70005539", Some("DS70005539C"));
        assert_eq!(candidates[0], "DS70005539C");
    }

    #[test]
    fn sibling_compatibility_requires_same_family_and_final_suffix() {
        assert!(is_compatible_sibling_part(
            "DSPIC33CDV128MC106",
            "dspic33cdvl64mc106"
        ));
        assert!(!is_compatible_sibling_part(
            "DSPIC33AK64MC105",
            "dspic33ak128mc106"
        ));
    }
}
