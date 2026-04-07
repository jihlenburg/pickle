//! Datasheet probing, validation, and local-cache helpers for DFP workflows.
//!
//! The DFP manager uses these helpers to validate that a local or downloaded
//! PDF matches the selected part before it is reused or cached. Keeping the PDF
//! probing logic here avoids mixing filesystem/path policy with PDF metadata and
//! family-matching heuristics.

use log::warn;
use pdfium_auto::bind_pdfium_silent;
use pdfium_render::prelude::PdfDocumentMetadataTagType;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

use crate::parser::datasheet_fetcher;
use crate::parser::dfp_paths::{datasheet_pdf_cache_path, datasheets_dir};

const MIN_LOCAL_DATASHEET_PAGES: u32 = 32;

pub(crate) fn datasheet_family_markers(part_number: &str) -> Vec<String> {
    let part_upper = part_number.trim().to_uppercase();
    let mut markers = vec![part_upper.clone()];

    let re = Regex::new(r"^((?:DSPIC|PIC)\d+[A-Z]+)").unwrap();
    if let Some(family) = re
        .captures(&part_upper)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
    {
        if !markers.contains(&family) {
            markers.push(family);
        }
    }

    markers
}

pub(crate) fn datasheet_part_suffix(part_number: &str) -> Option<String> {
    let part_upper = part_number.trim().to_uppercase();
    let re = Regex::new(r"([A-Z]{2,}\d{3}[A-Z]?)$").unwrap();
    re.captures(&part_upper)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

pub(crate) fn datasheet_part_series_marker(part_number: &str) -> Option<String> {
    let suffix = datasheet_part_suffix(part_number)?;
    let mut chars = suffix.chars();
    chars.next_back()?;
    let marker: String = chars.collect();
    if marker.len() >= 3 {
        Some(marker)
    } else {
        None
    }
}

pub(crate) fn datasheet_probe_matches_part(text: &str, part_number: &str) -> bool {
    let uppercase_text = text.to_uppercase();
    let part_upper = part_number.trim().to_uppercase();

    if uppercase_text.contains(&part_upper) {
        return true;
    }

    let markers = datasheet_family_markers(&part_upper);
    let family_prefix = markers.get(1);
    let family_series = datasheet_part_series_marker(&part_upper);

    family_prefix
        .zip(family_series.as_ref())
        .map(|(family, series)| uppercase_text.contains(family) && uppercase_text.contains(series))
        .unwrap_or(false)
}

pub(crate) fn datasheet_probe_matches_resolved_reference(
    text: &str,
    resolved: &datasheet_fetcher::DatasheetRef,
) -> bool {
    let uppercase_text = text.to_uppercase();

    [
        resolved.datasheet_number.as_str(),
        resolved.datasheet_revision.as_str(),
        resolved.datasheet_title.as_str(),
    ]
    .into_iter()
    .map(str::trim)
    .filter(|marker| !marker.is_empty())
    .map(str::to_uppercase)
    .any(|marker| uppercase_text.contains(&marker))
}

#[derive(Debug, Clone)]
struct DatasheetProbe {
    text: String,
    page_count: u32,
}

fn summarize_probe_excerpt(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(3)
        .collect::<Vec<_>>()
        .join(" / ")
}

fn extract_datasheet_probe(pdf_bytes: &[u8]) -> Result<DatasheetProbe, String> {
    let temp_pdf = NamedTempFile::new().map_err(|error| format!("Tempfile error: {error}"))?;
    fs::write(temp_pdf.path(), pdf_bytes)
        .map_err(|error| format!("Failed to write temporary PDF: {error}"))?;

    let pdfium = bind_pdfium_silent().map_err(|error| format!("Failed to bind PDFium: {error}"))?;
    let document = pdfium
        .load_pdf_from_file(temp_pdf.path(), None)
        .map_err(|error| format!("Failed to open PDF: {error}"))?;

    let total_pages = document.pages().len() as u32;
    let page_count = total_pages.min(4);
    let mut probe = String::new();

    // Microchip family datasheets often identify the canonical family only in
    // PDF metadata, while the first extracted text pages contain just the cover
    // footer and revision stamp. Include metadata before page text so the
    // datasheet matcher can recognize valid family PDFs.
    for tag in [
        PdfDocumentMetadataTagType::Title,
        PdfDocumentMetadataTagType::Subject,
        PdfDocumentMetadataTagType::Keywords,
    ] {
        if let Some(value) = document
            .metadata()
            .get(tag)
            .map(|entry| entry.value().trim().to_string())
        {
            if !value.is_empty() {
                probe.push_str(&value);
                probe.push('\n');
            }
        }
    }

    for index in 0..page_count {
        let page = document
            .pages()
            .get(index as u16)
            .map_err(|error| format!("Failed to access PDF page {}: {error}", index + 1))?;
        let page_text = page
            .text()
            .map_err(|error| {
                format!(
                    "Failed to extract text from PDF page {}: {error}",
                    index + 1
                )
            })?
            .all();
        if !page_text.trim().is_empty() {
            probe.push_str(&page_text);
            probe.push('\n');
        }
    }

    Ok(DatasheetProbe {
        text: probe,
        page_count: total_pages,
    })
}

fn datasheet_probe_mismatch_reason(probe: &DatasheetProbe, part_number: &str) -> Option<String> {
    if probe.text.trim().is_empty() {
        return None;
    }

    if datasheet_probe_matches_part(&probe.text, part_number) {
        return None;
    }

    // After `datasheet_fetcher::resolve()` succeeds, its cached DS number/title
    // is more authoritative than our family-suffix heuristic. This lets
    // family PDFs like `dsPIC33AK512MPS512 Family Data Sheet (DS70005591)`
    // validate for a part whose product page explicitly resolves to that PDF.
    let resolved_reference = datasheet_fetcher::cached_metadata(part_number)
        .filter(|resolved| resolved.part_number.eq_ignore_ascii_case(part_number));
    if let Some(resolved) = resolved_reference.as_ref() {
        if datasheet_probe_matches_resolved_reference(&probe.text, resolved) {
            return None;
        }
    }

    let markers = datasheet_family_markers(part_number);
    let series = datasheet_part_series_marker(part_number);
    let family_expected = match (markers.get(1), series.as_deref()) {
        (Some(family), Some(series)) => format!(
            "{} or sibling-family markers {} + {}",
            part_number.to_uppercase(),
            family,
            series
        ),
        _ => part_number.to_uppercase(),
    };
    let expected = if let Some(resolved) = resolved_reference.as_ref() {
        format!(
            "{} or resolved Microchip datasheet {} ({})",
            family_expected, resolved.datasheet_number, resolved.datasheet_title
        )
    } else {
        family_expected
    };

    let excerpt = summarize_probe_excerpt(&probe.text);
    Some(format!(
        "Selected datasheet does not appear to match {}. Expected to find {}, but saw: {}",
        part_number.to_uppercase(),
        expected,
        excerpt
    ))
}

fn local_datasheet_cache_reason(pdf_bytes: &[u8], part_number: &str) -> Option<String> {
    let probe = match extract_datasheet_probe(pdf_bytes) {
        Ok(probe) => probe,
        Err(error) => {
            warn!(
                "datasheet validation skipped for {} because PDF probing failed: {}",
                part_number, error
            );
            return None;
        }
    };

    if let Some(reason) = datasheet_probe_mismatch_reason(&probe, part_number) {
        return Some(reason);
    }

    if probe.page_count < MIN_LOCAL_DATASHEET_PAGES {
        return Some(format!(
            "Local datasheet cache expects a full datasheet PDF, but this file has only {} pages",
            probe.page_count
        ));
    }

    None
}

pub fn datasheet_pdf_mismatch_reason(pdf_bytes: &[u8], part_number: &str) -> Option<String> {
    match extract_datasheet_probe(pdf_bytes) {
        Ok(probe) => datasheet_probe_mismatch_reason(&probe, part_number),
        Err(error) => {
            warn!(
                "datasheet validation skipped for {} because PDF probing failed: {}",
                part_number, error
            );
            None
        }
    }
}

fn local_datasheet_matches_part(path: &Path, part_number: &str) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };

    if let Some(reason) = local_datasheet_cache_reason(&bytes, part_number) {
        warn!(
            "Ignoring local datasheet candidate {} for {}: {}",
            path.display(),
            part_number,
            reason
        );
        return false;
    }

    true
}

/// Try to find a datasheet PDF locally. Searches:
/// 1. Fetcher's PDF cache (`dfp_cache/datasheets/pdf/`)
/// 2. Legacy datasheets dir (`dfp_cache/datasheets/`)
/// 3. `~/Downloads` (shallow) for PDFs matching the part number or family
pub fn find_local_datasheet(part_number: &str) -> Option<PathBuf> {
    let part_upper = part_number.to_uppercase();
    let part_lower = part_number.to_lowercase();

    let fetcher_pdf = datasheet_pdf_cache_path(&part_upper);
    if fetcher_pdf.exists() && local_datasheet_matches_part(&fetcher_pdf, &part_upper) {
        return Some(fetcher_pdf);
    }

    let ds_dir = datasheets_dir();
    if let Ok(entries) = fs::read_dir(&ds_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .map(|ext| ext.eq_ignore_ascii_case("pdf"))
                .unwrap_or(false)
            {
                let stem = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_uppercase();
                if stem.contains(&part_upper) && local_datasheet_matches_part(&path, &part_upper) {
                    return Some(path);
                }
            }
        }
    }

    let family_lower = {
        let re = Regex::new(r"^((?:DSPIC|PIC)\d+[A-Z]+)").unwrap();
        re.captures(&part_upper)
            .and_then(|caps| caps.get(1))
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
                    .map(|ext| ext.eq_ignore_ascii_case("pdf"))
                    .unwrap_or(false)
                {
                    continue;
                }
                let name_lower = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                if name_lower.contains(&part_lower)
                    && local_datasheet_matches_part(&path, &part_upper)
                {
                    return Some(path);
                }
                if !family_lower.is_empty()
                    && name_lower.contains(&family_lower)
                    && best.is_none()
                    && local_datasheet_matches_part(&path, &part_upper)
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
    if let Some(reason) = local_datasheet_cache_reason(pdf_bytes, part_number) {
        warn!(
            "Skipping datasheet cache write for {} because the PDF does not match the selected part: {}",
            part_number,
            reason
        );
        return None;
    }

    let path = datasheet_pdf_cache_path(part_number);
    fs::write(&path, pdf_bytes).ok()?;

    // Keep reading legacy flat-cache PDFs for backward compatibility, but new
    // writes should only live in the structured `datasheets/pdf/` cache.
    let legacy_path = datasheets_dir().join(format!("{}.pdf", part_number.to_uppercase()));
    if legacy_path.exists() {
        let structured_len = fs::metadata(&path).ok().map(|meta| meta.len());
        let legacy_len = fs::metadata(&legacy_path).ok().map(|meta| meta.len());
        if structured_len.is_some() && structured_len == legacy_len {
            let _ = fs::remove_file(&legacy_path);
        }
    }

    Some(path)
}
