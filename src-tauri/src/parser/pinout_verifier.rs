//! Pinout Verifier: cross-check parsed EDC pinout data against the device datasheet
//! using an LLM API (Anthropic or OpenAI). Provider selection follows the saved
//! app setting (`auto`, `openai`, or `anthropic`), with `auto` preferring OpenAI
//! when both are configured.

use image::ImageFormat;
use pdfium_auto::bind_pdfium_silent;
use pdfium_render::prelude::PdfRenderConfig;
use qpdf::{ObjectStreamMode, QPdf, StreamDataMode};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Cursor};
use std::path::PathBuf;
use std::time::Instant;
use tempfile::NamedTempFile;

use crate::parser::dfp_manager::{dfp_cache_dir, pinouts_dir};

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_FILES_API_URL: &str = "https://api.anthropic.com/v1/files";
const ANTHROPIC_API_VERSION: &str = "2023-06-01";
const ANTHROPIC_FILES_API_BETA: &str = "files-api-2025-04-14";
const ANTHROPIC_VERIFICATION_TOOL_NAME: &str = "submit_verification";
const ANTHROPIC_MODEL_BYTES: &[u8] = &[
    99, 108, 97, 117, 100, 101, 45, 115, 111, 110, 110, 101, 116, 45, 52, 45, 54,
];

const OPENAI_API_URL: &str = "https://api.openai.com/v1/responses";
const OPENAI_FILES_API_URL: &str = "https://api.openai.com/v1/files";
const OPENAI_MODEL: &str = "gpt-5.4";
const OPENAI_REASONING_EFFORT: &str = "high";
const OPENAI_MAX_OUTPUT_TOKENS: u32 = 65_536;
const PNG_IMAGE_MEDIA_TYPE: &str = "image/png";

const MAX_TOKENS: u32 = 16384;
const OPENAI_FILE_LIMIT_BYTES: usize = 50 * 1024 * 1024;
const RENDER_DPI: f32 = 300.0;

#[derive(Debug, Clone)]
struct BookmarkEntry {
    title: String,
    page: u32,
    depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PageSpan {
    start: u32,
    end: u32,
}

#[derive(Debug, Clone)]
struct ReducedPdf {
    bytes: Vec<u8>,
    page_spans: Vec<PageSpan>,
}

impl ReducedPdf {
    fn selected_pages(&self) -> u32 {
        self.page_spans
            .iter()
            .map(|span| span.end.saturating_sub(span.start) + 1)
            .sum()
    }
}

#[derive(Debug, Clone)]
struct RenderedPageImage {
    page_number: u32,
    bytes: Vec<u8>,
}

#[derive(Debug, Default)]
struct OpenAiStreamState {
    output_text: String,
    refusal_text: String,
    final_response: Option<Value>,
    last_error: Option<String>,
    event_count: usize,
    terminal_event_seen: bool,
}

#[derive(Debug, Clone, Copy)]
enum Provider {
    Anthropic,
    OpenAI,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VerifyTask {
    Pinout,
    Clc,
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Anthropic => "Anthropic",
        Provider::OpenAI => "OpenAI",
    }
}

fn provider_analysis_hint(provider: Provider, task: VerifyTask) -> &'static str {
    match (provider, task) {
        (Provider::Anthropic, VerifyTask::Pinout) => {
            "This is usually the slowest step. Large datasheets can take a couple of minutes or more."
        }
        (Provider::OpenAI, VerifyTask::Pinout) => {
            "This is usually the slowest step. Large datasheets can take up to 3 minutes or more."
        }
        (_, VerifyTask::Clc) => {
            "CLC-only lookups are usually shorter because pickle uploads just the CLC chapter pages."
        }
    }
}

fn task_label(task: VerifyTask) -> &'static str {
    match task {
        VerifyTask::Pinout => "pinout",
        VerifyTask::Clc => "CLC",
    }
}

fn task_sections_label(task: VerifyTask) -> &'static str {
    match task {
        VerifyTask::Pinout => "pinout pages",
        VerifyTask::Clc => "CLC chapter pages",
    }
}

fn task_reduce_progress_label(task: VerifyTask) -> &'static str {
    match task {
        VerifyTask::Pinout => "Trimming the datasheet to pinout pages",
        VerifyTask::Clc => "Trimming the datasheet to CLC pages",
    }
}

fn task_reduce_progress_detail(task: VerifyTask) -> &'static str {
    match task {
        VerifyTask::Pinout => {
            "pickle uploads only the relevant pinout pages instead of the entire family datasheet."
        }
        VerifyTask::Clc => {
            "pickle uploads only the CLC chapter pages for the background CLC lookup."
        }
    }
}

fn openai_instructions(task: VerifyTask) -> &'static str {
    match task {
        VerifyTask::Pinout => {
            "You are analyzing a reduced Microchip dsPIC33/PIC24 datasheet PDF to extract and verify pin mapping data. Return only valid JSON."
        }
        VerifyTask::Clc => {
            "You are analyzing a reduced Microchip dsPIC33/PIC24 datasheet PDF to extract CLC input source mappings. Return only valid JSON."
        }
    }
}

fn openai_image_instructions(task: VerifyTask) -> &'static str {
    match task {
        VerifyTask::Pinout => {
            "You are analyzing rendered datasheet page images to extract and verify pin mapping data. Return only valid JSON."
        }
        VerifyTask::Clc => {
            "You are analyzing rendered datasheet page images to extract CLC input source mappings. Return only valid JSON."
        }
    }
}

pub type ProgressCallback = dyn Fn(VerifyProgressUpdate) + Send + Sync;

#[derive(Debug, Clone, Serialize)]
pub struct VerifyProgressUpdate {
    pub stage: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub progress: f64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub indeterminate: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

impl VerifyProgressUpdate {
    pub fn new(stage: &str, progress: f64, label: impl Into<String>) -> Self {
        Self {
            stage: stage.to_string(),
            label: label.into(),
            detail: None,
            progress: progress.clamp(0.0, 1.0),
            indeterminate: false,
            provider: None,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn indeterminate(mut self, indeterminate: bool) -> Self {
        self.indeterminate = indeterminate;
        self
    }

    fn provider(mut self, provider: Provider) -> Self {
        self.provider = Some(provider_name(provider).to_string());
        self
    }
}

fn emit_progress(progress: Option<&ProgressCallback>, update: VerifyProgressUpdate) {
    if let Some(progress) = progress {
        progress(update);
    }
}

fn get_env_key(var_name: &str) -> Option<String> {
    // 1. OS keychain (highest priority — user-managed via Settings dialog)
    let provider = match var_name {
        "OPENAI_API_KEY" => Some("openai"),
        "ANTHROPIC_API_KEY" => Some("anthropic"),
        _ => None,
    };
    if let Some(p) = provider {
        if let Some(key) = crate::commands::keychain::get_keychain_key(p) {
            return Some(key);
        }
    }

    // 2. Environment variable
    if let Ok(key) = std::env::var(var_name) {
        if !key.is_empty() {
            return Some(key);
        }
    }

    // 3. .env files in data roots
    for root in crate::parser::dfp_manager::read_roots() {
        let env_path = root.join(".env");
        if env_path.exists() {
            if let Ok(text) = fs::read_to_string(&env_path) {
                for line in text.lines() {
                    let line = line.trim();
                    if let Some(rest) = line.strip_prefix(&format!("{}=", var_name)) {
                        let key = rest.trim();
                        if !key.is_empty() {
                            return Some(key.to_string());
                        }
                    }
                }
            }
        }
    }

    None
}

fn get_optional_env_value(var_name: &str) -> Option<String> {
    std::env::var(var_name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn anthropic_model() -> &'static str {
    std::str::from_utf8(ANTHROPIC_MODEL_BYTES).expect("valid Anthropic model identifier")
}

fn openai_model() -> String {
    get_optional_env_value("PICKLE_OPENAI_MODEL").unwrap_or_else(|| OPENAI_MODEL.to_string())
}

fn openai_reasoning_effort() -> String {
    match get_optional_env_value("PICKLE_OPENAI_REASONING_EFFORT") {
        Some(value) => match value.as_str() {
            "minimal" | "low" | "medium" | "high" => value,
            other => {
                log::warn!(
                    "verify_pinout: ignoring unsupported PICKLE_OPENAI_REASONING_EFFORT={}",
                    other
                );
                OPENAI_REASONING_EFFORT.to_string()
            }
        },
        None => OPENAI_REASONING_EFFORT.to_string(),
    }
}

fn verify_cache_disabled() -> bool {
    matches!(
        get_optional_env_value("PICKLE_DISABLE_VERIFY_CACHE")
            .as_deref()
            .map(|value| value.to_ascii_lowercase()),
        Some(ref value) if value == "1" || value == "true" || value == "yes"
    )
}

pub fn get_api_key() -> Option<String> {
    resolve_provider(None).ok().map(|(_, key)| key)
}

fn preferred_provider_setting() -> String {
    crate::settings::load()
        .map(|settings| settings.verification.provider)
        .unwrap_or_else(|_| crate::settings::default_verification_provider())
}

/// Determine which provider to use and return (Provider, api_key).
fn resolve_provider(override_key: Option<&str>) -> Result<(Provider, String), String> {
    // If caller provided a key, detect provider by prefix
    if let Some(k) = override_key {
        if !k.is_empty() {
            let provider = if k.starts_with("sk-proj-") || k.starts_with("sk-org-") {
                Provider::OpenAI
            } else {
                Provider::Anthropic
            };
            return Ok((provider, k.to_string()));
        }
    }

    match preferred_provider_setting().as_str() {
        "openai" => {
            if let Some(key) = get_env_key("OPENAI_API_KEY") {
                return Ok((Provider::OpenAI, key));
            }
            return Err(
                "Verification provider is set to OpenAI, but no OpenAI API key is configured."
                    .to_string(),
            );
        }
        "anthropic" => {
            if let Some(key) = get_env_key("ANTHROPIC_API_KEY") {
                return Ok((Provider::Anthropic, key));
            }
            return Err(
                "Verification provider is set to Anthropic, but no Anthropic API key is configured."
                    .to_string(),
            );
        }
        _ => {}
    }

    if let Some(key) = get_env_key("OPENAI_API_KEY") {
        return Ok((Provider::OpenAI, key));
    }
    if let Some(key) = get_env_key("ANTHROPIC_API_KEY") {
        return Ok((Provider::Anthropic, key));
    }

    Err("No API key configured. Set OPENAI_API_KEY or ANTHROPIC_API_KEY in .env".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinCorrection {
    pub pin_position: u32,
    pub current_pad: String,
    pub datasheet_pad: String,
    pub current_functions: Vec<String>,
    pub datasheet_functions: Vec<String>,
    pub correction_type: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageResult {
    pub package_name: String,
    pub pin_count: u32,
    pub pins: HashMap<u32, String>,
    pub pin_functions: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub corrections: Vec<PinCorrection>,
    #[serde(default)]
    pub match_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyResult {
    pub part_number: String,
    pub packages: HashMap<String, PackageResult>,
    #[serde(default)]
    pub notes: Vec<String>,
    /// CLC input source MUX mapping extracted from the CLCxSEL register chapter:
    /// 4 groups (DS1–DS4) of 8 source labels.  `None` when the datasheet lacks
    /// a CLC chapter or the LLM couldn't locate it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clc_input_sources: Option<Vec<Vec<String>>>,
    #[serde(skip)]
    pub raw_response: String,
}

impl VerifyResult {
    pub fn to_overlay_json(&self) -> Value {
        let mut packages = serde_json::Map::new();
        for (name, pkg) in &self.packages {
            let mut pins_map = serde_json::Map::new();
            let mut sorted_pins: Vec<_> = pkg.pins.iter().collect();
            sorted_pins.sort_by_key(|(k, _)| **k);
            for (pos, pad) in sorted_pins {
                pins_map.insert(pos.to_string(), Value::String(pad.clone()));
            }
            let mut pkg_obj = serde_json::Map::new();
            pkg_obj.insert("source".into(), Value::String("overlay".into()));
            pkg_obj.insert("pin_count".into(), Value::Number(pkg.pin_count.into()));
            pkg_obj.insert("pins".into(), Value::Object(pins_map));
            packages.insert(name.clone(), Value::Object(pkg_obj));
        }
        let mut root = serde_json::Map::new();
        root.insert("packages".into(), Value::Object(packages));
        Value::Object(root)
    }
}

const PINOUT_VERIFY_PROMPT: &str = r#"You are analyzing a Microchip dsPIC33/PIC24 datasheet PDF to extract pin mapping data.

## Where to Look

**Pinout data:** Located between the "Pin Diagrams" section and the Table of Contents. Focus on
the pin function tables (e.g., "28-PIN SSOP COMPLETE PIN FUNCTION DESCRIPTIONS").

## Task

This datasheet may cover multiple devices in the same family with different pin counts.
Extract the relevant package pinout tables — the results will be cached and filtered per-device later.

1. Find ALL package pinout tables in this datasheet (e.g., SPDIP, SOIC, SSOP, QFN, TQFP, UQFN, etc.)
2. For each package, extract the COMPLETE pin-to-pad mapping (every pin number → pad name)
3. For each pad, extract ALL listed functions/alternate names
4. Compare against the current parsed data (provided below) and identify any discrepancies

## Current Parsed Data

{current_data}

## Output Format

Return a JSON object with this exact structure (no markdown fencing, just raw JSON):

{{
  "packages": {{
    "<PackageName>": {{
      "pin_count": <int>,
      "pins": {{
        "<pin_number>": "<pad_name>",
        ...
      }},
      "pin_functions": {{
        "<pad_name>": ["func1", "func2", ...],
        ...
      }}
    }}
  }},
  "corrections": [
    {{
      "pin_position": <int>,
      "package": "<PackageName>",
      "current_pad": "<what EDC says>",
      "datasheet_pad": "<what datasheet says>",
      "type": "<wrong_pad|missing_functions|extra_functions|missing_pin>",
      "note": "<explanation>"
    }}
  ],
  "clc_input_sources": [],
  "notes": ["<any general observations about data quality>"]
}}

## Important Guidelines

- Use UPPERCASE for pad names (e.g., "RA0", "RB5", "MCLR", "VDD", "VSS", "AVDD")
- Pin numbers must be integers (as strings in JSON keys)
- Include ALL pins including power (VDD, VSS, AVDD, AVSS, VCAP) and special (MCLR)
- For pads with numbered duplicates (multiple VDD pins), use suffixes: VDD, VDD_2, VDD_3, etc.
- Functions should include the primary I/O name (e.g., "RA0"), analog channel (e.g., "AN0"), and any fixed peripheral functions
- If the datasheet shows a package not in the current data, include it as a new entry
- If pin data matches perfectly, say so in notes — don't invent corrections
- Be precise: only flag actual discrepancies, not formatting differences
- For this pinout-only pass, always return "clc_input_sources": []
"#;

const CLC_VERIFY_PROMPT: &str = r#"You are analyzing a Microchip dsPIC33/PIC24 datasheet PDF to extract CLC input source mappings.

## Where to Look

**CLC data:** Located in the "Configurable Logic Cell (CLC)" chapter. Find the **CLCxSEL register**
definition — it defines what signals each Data Selection MUX (DS1–DS4) can select from. Each
DSx[2:0] field lists 8 signal sources (values 000–111).

## Task

1. Find the CLC chapter and the CLCxSEL register definition pages
2. Extract the DS1–DS4 input source mappings exactly as listed
3. Return no package pinout data in this pass

## Device Summary

{device_summary}

## Output Format

Return a JSON object with this exact structure (no markdown fencing, just raw JSON):

{{
  "packages": [],
  "corrections": [],
  "clc_input_sources": [
    ["<DS1 source 0>", "<DS1 source 1>", ..., "<DS1 source 7>"],
    ["<DS2 source 0>", "<DS2 source 1>", ..., "<DS2 source 7>"],
    ["<DS3 source 0>", "<DS3 source 1>", ..., "<DS3 source 7>"],
    ["<DS4 source 0>", "<DS4 source 1>", ..., "<DS4 source 7>"]
  ],
  "notes": ["<any general observations about the CLC extraction>"]
}}

## Important Guidelines

- For this CLC-only pass, always return "packages": [] and "corrections": []
- For CLC sources: use short signal names (e.g., "CLCINA", "Fcy", "CLC3OUT", "CMP1", "UART1 TX")
- For CLC sources marked "Reserved" in the register, use the string "Reserved"
- If the datasheet has no CLC chapter or CLCxSEL register, return "clc_input_sources": []
"#;

fn build_current_data_summary(device_data: &Value) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "Part: {}",
        device_data
            .get("part_number")
            .and_then(|v| v.as_str())
            .unwrap_or("UNKNOWN")
    ));
    lines.push(format!(
        "Selected package: {}",
        device_data
            .get("selected_package")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
    ));
    let pin_count = device_data
        .get("pin_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    lines.push(format!("Device pin count: {}", pin_count));

    if let Some(pins) = device_data.get("pins").and_then(|v| v.as_array()) {
        lines.push(format!("\nCurrent pin mapping ({} pins):", pins.len()));
        for pin in pins {
            let pos = pin.get("position").and_then(|v| v.as_u64()).unwrap_or(0);
            let pad = pin
                .get("pad_name")
                .or_else(|| pin.get("pad"))
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let funcs: Vec<&str> = pin
                .get("functions")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).take(8).collect())
                .unwrap_or_default();
            let is_power = pin
                .get("is_power")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let rp = pin.get("rp_number").and_then(|v| v.as_u64());
            let func_str = funcs.join(", ");
            let rp_str = rp.map(|r| format!(" [RP{}]", r)).unwrap_or_default();
            let pwr_str = if is_power { " [POWER]" } else { "" };
            lines.push(format!(
                "  Pin {}: {}{}{} — {}",
                pos, pad, pwr_str, rp_str, func_str
            ));
        }
    }

    lines.join("\n")
}

fn build_clc_device_summary(device_data: &Value) -> String {
    format!(
        "Part: {}\nSelected package: {}\nDevice pin count: {}",
        device_data
            .get("part_number")
            .and_then(|v| v.as_str())
            .unwrap_or("UNKNOWN"),
        device_data
            .get("selected_package")
            .and_then(|v| v.as_str())
            .unwrap_or("default"),
        device_data
            .get("pin_count")
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
    )
}

fn build_task_prompt(provider: Provider, task: VerifyTask, device_data: &Value) -> String {
    match task {
        VerifyTask::Pinout => {
            let base_prompt = PINOUT_VERIFY_PROMPT.replace(
                "{current_data}",
                &build_current_data_summary(device_data),
            );
            match provider {
                Provider::Anthropic => base_prompt,
                Provider::OpenAI => {
                    let pin_count = device_data
                        .get("pin_count")
                        .and_then(|value| value.as_u64())
                        .unwrap_or(0);
                    format!(
                        "{base_prompt}\n\nOpenAI-specific scope reduction:\n- Only extract packages whose pin_count matches the current device pin count ({pin_count}).\n- Ignore package tables for other pin counts.\n- If multiple package names share this pin count, include all of them.\n- This is the pinout-only pass, so keep \"clc_input_sources\" empty.\n- The API enforces a structured response schema. Populate every required field and do not add extra keys."
                    )
                }
            }
        }
        VerifyTask::Clc => {
            let base_prompt = CLC_VERIFY_PROMPT.replace(
                "{device_summary}",
                &build_clc_device_summary(device_data),
            );
            match provider {
                Provider::Anthropic => base_prompt,
                Provider::OpenAI => format!(
                    "{base_prompt}\n\nOpenAI-specific scope reduction:\n- This is the CLC-only pass, so always return \"packages\": [] and \"corrections\": [].\n- The API enforces a structured response schema. Populate every required field and do not add extra keys."
                ),
            }
        }
    }
}

/// The verifier narrows the datasheet to bookmark-selected pinout and CLC
/// sections before uploading it to the provider. If reduced-PDF handling fails,
/// it falls back to rendered PNG page images.
fn prepare_pdf(pdf_bytes: &[u8]) -> Vec<u8> {
    pdf_bytes.to_vec()
}

fn normalize_bookmark_title(title: &str) -> String {
    let mut normalized = String::with_capacity(title.len());
    let mut previous_was_space = false;

    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
            previous_was_space = false;
        } else if !previous_was_space {
            normalized.push(' ');
            previous_was_space = true;
        }
    }

    normalized.trim().to_string()
}

fn title_matches_pin_diagrams(title: &str) -> bool {
    normalize_bookmark_title(title).contains("pin diagrams")
}

fn title_matches_pinout_descriptions(title: &str) -> bool {
    let normalized = normalize_bookmark_title(title);
    normalized.contains("pinout io descriptions")
        || normalized.contains("pinout i o descriptions")
        || normalized.contains("pin function descriptions")
}

fn title_matches_table_of_contents(title: &str) -> bool {
    normalize_bookmark_title(title).contains("table of contents")
}

fn title_matches_clc(title: &str) -> bool {
    let normalized = normalize_bookmark_title(title);
    normalized.contains("configurable logic cell") || normalized == "clc"
}

fn text_matches_clc_section(text: &str) -> bool {
    let normalized = normalize_bookmark_title(text);
    normalized.contains("configurable logic cell")
        || normalized.contains("clcxsel")
        || normalized.contains("clc1sel")
        || normalized.contains("clc2sel")
        || normalized.contains("clc3sel")
        || normalized.contains("clc4sel")
}

fn title_starts_numbered_chapter(title: &str) -> bool {
    normalize_bookmark_title(title)
        .chars()
        .next()
        .map(|ch| ch.is_ascii_digit())
        .unwrap_or(false)
}

fn first_matching_bookmark(
    bookmarks: &[BookmarkEntry],
    matcher: fn(&str) -> bool,
) -> Option<&BookmarkEntry> {
    bookmarks
        .iter()
        .filter(|bookmark| matcher(&bookmark.title))
        .min_by_key(|bookmark| (bookmark.page, bookmark.depth))
}

fn first_matching_page_after(
    bookmarks: &[BookmarkEntry],
    page: u32,
    matcher: fn(&str) -> bool,
) -> Option<u32> {
    bookmarks
        .iter()
        .filter(|bookmark| bookmark.page > page)
        .find(|bookmark| matcher(&bookmark.title))
        .map(|bookmark| bookmark.page)
}

fn first_numbered_page_after_at_or_above_depth(
    bookmarks: &[BookmarkEntry],
    start: &BookmarkEntry,
) -> Option<u32> {
    bookmarks
        .iter()
        .filter(|bookmark| {
            bookmark.page > start.page
                && bookmark.depth <= start.depth
                && title_starts_numbered_chapter(&bookmark.title)
        })
        .map(|bookmark| bookmark.page)
        .min()
}

fn next_page_after_at_or_above_depth(
    bookmarks: &[BookmarkEntry],
    start: &BookmarkEntry,
) -> Option<u32> {
    bookmarks
        .iter()
        .filter(|bookmark| bookmark.page > start.page && bookmark.depth <= start.depth)
        .map(|bookmark| bookmark.page)
        .min()
}

fn section_end_from_bookmarks(
    bookmarks: &[BookmarkEntry],
    start: &BookmarkEntry,
    total_pages: u32,
) -> u32 {
    first_matching_page_after(bookmarks, start.page, title_matches_table_of_contents)
        .map(|page| page.saturating_sub(1))
        .or_else(|| {
            first_numbered_page_after_at_or_above_depth(bookmarks, start)
                .map(|page| page.saturating_sub(1))
        })
        .unwrap_or(total_pages)
}

fn merge_page_spans(mut spans: Vec<PageSpan>) -> Vec<PageSpan> {
    spans.sort_unstable_by_key(|span| span.start);
    let mut merged: Vec<PageSpan> = Vec::new();

    for span in spans {
        if let Some(last) = merged.last_mut() {
            if span.start <= last.end.saturating_add(1) {
                last.end = last.end.max(span.end);
                continue;
            }
        }
        merged.push(span);
    }

    merged
}

fn select_pinout_page_spans(bookmarks: &[BookmarkEntry], total_pages: u32) -> Vec<PageSpan> {
    let mut spans = Vec::new();

    if let Some(pin_start) = first_matching_bookmark(bookmarks, title_matches_pin_diagrams) {
        let pin_end = section_end_from_bookmarks(bookmarks, pin_start, total_pages);

        if pin_end >= pin_start.page {
            spans.push(PageSpan {
                start: pin_start.page,
                end: pin_end,
            });
        }
    }

    if let Some(pinout_start) =
        first_matching_bookmark(bookmarks, title_matches_pinout_descriptions)
    {
        let pinout_end = section_end_from_bookmarks(bookmarks, pinout_start, total_pages);

        if pinout_end >= pinout_start.page {
            spans.push(PageSpan {
                start: pinout_start.page,
                end: pinout_end,
            });
        }
    }

    merge_page_spans(spans)
}

fn select_clc_page_spans(bookmarks: &[BookmarkEntry], total_pages: u32) -> Vec<PageSpan> {
    let mut spans = Vec::new();

    if let Some(clc_start) = first_matching_bookmark(bookmarks, title_matches_clc) {
        let clc_end = next_page_after_at_or_above_depth(bookmarks, clc_start)
            .map(|page| page.saturating_sub(1))
            .unwrap_or(total_pages);

        if clc_end >= clc_start.page {
            spans.push(PageSpan {
                start: clc_start.page,
                end: clc_end,
            });
        }
    }

    merge_page_spans(spans)
}

fn select_clc_page_spans_from_text_hits(hits: &[u32], total_pages: u32) -> Vec<PageSpan> {
    if hits.is_empty() {
        return Vec::new();
    }

    let start_hit = hits[0];
    let mut end_hit = hits[0];

    for &page in hits.iter().skip(1) {
        if page <= end_hit.saturating_add(2) && page <= start_hit.saturating_add(40) {
            end_hit = page;
        } else {
            break;
        }
    }

    vec![PageSpan {
        start: start_hit.saturating_sub(1).max(1),
        end: (end_hit + 1).min(total_pages),
    }]
}

fn task_page_spans(bookmarks: &[BookmarkEntry], total_pages: u32, task: VerifyTask) -> Vec<PageSpan> {
    match task {
        VerifyTask::Pinout => select_pinout_page_spans(bookmarks, total_pages),
        VerifyTask::Clc => select_clc_page_spans(bookmarks, total_pages),
    }
}

fn collect_pdf_bookmarks(pdf_bytes: &[u8]) -> Result<(Vec<BookmarkEntry>, u32), String> {
    let temp_pdf = NamedTempFile::new().map_err(|error| format!("Tempfile error: {error}"))?;
    fs::write(temp_pdf.path(), pdf_bytes)
        .map_err(|error| format!("Failed to write temporary PDF: {error}"))?;

    let pdfium = bind_pdfium_silent().map_err(|error| format!("Failed to bind PDFium: {error}"))?;
    let document = pdfium
        .load_pdf_from_file(temp_pdf.path(), None)
        .map_err(|error| format!("Failed to open PDF for bookmark scan: {error}"))?;

    let total_pages = document.pages().len() as u32;
    let mut bookmarks = Vec::new();

    for bookmark in document.bookmarks().iter() {
        let Some(title) = bookmark.title() else {
            continue;
        };
        let Some(page) = bookmark
            .destination()
            .and_then(|destination| destination.page_index().ok())
            .map(|page_index| page_index as u32 + 1)
        else {
            continue;
        };

        let mut depth = 0usize;
        let mut current = bookmark.parent();
        while let Some(parent) = current {
            depth += 1;
            current = parent.parent();
        }

        bookmarks.push(BookmarkEntry { title, page, depth });
    }

    Ok((bookmarks, total_pages))
}

fn find_clc_page_spans_from_text(pdf_bytes: &[u8]) -> Result<(Vec<PageSpan>, u32), String> {
    let temp_pdf = NamedTempFile::new().map_err(|error| format!("Tempfile error: {error}"))?;
    fs::write(temp_pdf.path(), pdf_bytes)
        .map_err(|error| format!("Failed to write temporary PDF: {error}"))?;

    let pdfium = bind_pdfium_silent().map_err(|error| format!("Failed to bind PDFium: {error}"))?;
    let document = pdfium
        .load_pdf_from_file(temp_pdf.path(), None)
        .map_err(|error| format!("Failed to open PDF for CLC text scan: {error}"))?;

    let total_pages = document.pages().len() as u32;
    let mut hits = Vec::new();

    for page_number in 1..=total_pages {
        let page = document
            .pages()
            .get((page_number - 1) as u16)
            .map_err(|error| format!("Failed to read PDF page {page_number}: {error}"))?;
        let page_text = page
            .text()
            .map_err(|error| format!("Failed to extract text from PDF page {page_number}: {error}"))?
            .all();

        if text_matches_clc_section(&page_text) {
            hits.push(page_number);
        }
    }

    let spans = select_clc_page_spans_from_text_hits(&hits, total_pages);
    Ok((spans, total_pages))
}

fn relevant_page_spans_for_pdf(pdf_bytes: &[u8], task: VerifyTask) -> Result<(Vec<PageSpan>, u32), String> {
    let (bookmarks, total_pages) = collect_pdf_bookmarks(pdf_bytes)?;
    let page_spans = task_page_spans(&bookmarks, total_pages, task);
    if page_spans.is_empty() {
        if task == VerifyTask::Clc {
            let (fallback_spans, total_pages) = find_clc_page_spans_from_text(pdf_bytes)?;
            if !fallback_spans.is_empty() {
                log::info!(
                    "verify_pinout: using PDF text fallback to locate CLC pages ({})",
                    describe_page_spans(&fallback_spans)
                );
                return Ok((fallback_spans, total_pages));
            }
        }

        return Err(match task {
            VerifyTask::Pinout => "No bookmark ranges found for pinout sections".to_string(),
            VerifyTask::Clc => "No bookmark or text ranges found for the CLC section".to_string(),
        });
    }

    Ok((page_spans, total_pages))
}

fn reduce_pdf_with_bookmarks(pdf_bytes: &[u8], task: VerifyTask) -> Result<ReducedPdf, String> {
    let (page_spans, _) = relevant_page_spans_for_pdf(pdf_bytes, task)?;
    let input_pdf = NamedTempFile::new().map_err(|error| format!("Tempfile error: {error}"))?;
    fs::write(input_pdf.path(), pdf_bytes)
        .map_err(|error| format!("Failed to write temporary input PDF: {error}"))?;
    let output_pdf = NamedTempFile::new().map_err(|error| format!("Tempfile error: {error}"))?;

    let source = QPdf::read(input_pdf.path())
        .map_err(|error| format!("qpdf failed to read datasheet: {error}"))?;
    let pages = source
        .get_pages()
        .map_err(|error| format!("qpdf failed to enumerate pages: {error}"))?;
    let sink = QPdf::empty();

    for span in &page_spans {
        for page_number in span.start..=span.end {
            let page = pages
                .get((page_number - 1) as usize)
                .ok_or_else(|| format!("Selected page {page_number} is out of range"))?;
            sink.add_page(page, false)
                .map_err(|error| format!("qpdf failed to add page {page_number}: {error}"))?;
        }
    }

    sink.writer()
        .preserve_unreferenced_objects(false)
        .object_stream_mode(ObjectStreamMode::Preserve)
        .stream_data_mode(StreamDataMode::Preserve)
        .compress_streams(true)
        .write(output_pdf.path())
        .map_err(|error| format!("qpdf failed to write reduced PDF: {error}"))?;

    let bytes = fs::read(output_pdf.path())
        .map_err(|error| format!("Failed to read reduced PDF: {error}"))?;

    Ok(ReducedPdf { bytes, page_spans })
}

fn render_target_pixels(points: f32) -> i32 {
    ((points / 72.0) * RENDER_DPI).round().max(1.0) as i32
}

fn render_pages_to_pngs(
    pdf_bytes: &[u8],
    page_spans: &[PageSpan],
) -> Result<Vec<RenderedPageImage>, String> {
    let temp_pdf = NamedTempFile::new().map_err(|error| format!("Tempfile error: {error}"))?;
    fs::write(temp_pdf.path(), pdf_bytes)
        .map_err(|error| format!("Failed to write temporary PDF: {error}"))?;

    let pdfium = bind_pdfium_silent().map_err(|error| format!("Failed to bind PDFium: {error}"))?;
    let document = pdfium
        .load_pdf_from_file(temp_pdf.path(), None)
        .map_err(|error| format!("Failed to open PDF for rendering: {error}"))?;

    let mut rendered = Vec::new();
    for span in page_spans {
        for page_number in span.start..=span.end {
            let page = document
                .pages()
                .get(
                    u16::try_from(page_number - 1)
                        .map_err(|_| format!("Page index {page_number} does not fit in u16"))?,
                )
                .map_err(|error| format!("Failed to access page {page_number}: {error}"))?;
            let render_config = PdfRenderConfig::new()
                .set_target_width(render_target_pixels(page.width().value))
                .set_target_height(render_target_pixels(page.height().value));
            let image = page
                .render_with_config(&render_config)
                .map_err(|error| format!("Failed to render page {page_number}: {error}"))?
                .as_image();

            let mut cursor = Cursor::new(Vec::new());
            image
                .write_to(&mut cursor, ImageFormat::Png)
                .map_err(|error| format!("Failed to encode page {page_number} as PNG: {error}"))?;

            rendered.push(RenderedPageImage {
                page_number,
                bytes: cursor.into_inner(),
            });
        }
    }

    Ok(rendered)
}

fn should_fallback_openai_to_images(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("context_length_exceeded")
        || lower.contains("input_file")
        || lower.contains("application/pdf")
        || lower.contains("unsupported")
        || lower.contains("invalid file")
        || lower.contains("no text in openai response")
}

fn describe_page_spans(page_spans: &[PageSpan]) -> String {
    page_spans
        .iter()
        .map(|span| {
            if span.start == span.end {
                span.start.to_string()
            } else {
                format!("{}-{}", span.start, span.end)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn normalize_openai_output_value(value: &Value) -> Value {
    if value
        .get("packages")
        .map(|packages| packages.is_array())
        .unwrap_or(false)
    {
        normalize_structured_verification_output(value)
    } else {
        value.clone()
    }
}

fn normalize_openai_output_text(text: &str) -> Result<String, String> {
    let parsed: Value = serde_json::from_str(text)
        .map_err(|error| format!("OpenAI structured JSON parse error: {error}"))?;
    serde_json::to_string(&normalize_openai_output_value(&parsed))
        .map_err(|error| format!("Failed to serialize normalized OpenAI output: {error}"))
}

fn extract_openai_status_error(result: &Value) -> Option<String> {
    match result.get("status").and_then(|value| value.as_str()) {
        Some("incomplete") => {
            let reason = result
                .get("incomplete_details")
                .and_then(|value| value.get("reason"))
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            Some(format!("OpenAI response incomplete: {reason}"))
        }
        Some("failed") => Some(format!(
            "OpenAI response failed: {}",
            result
                .get("error")
                .map(|value| value.to_string())
                .unwrap_or_else(|| result.to_string())
        )),
        _ => None,
    }
}

fn extract_openai_text(result: &Value) -> Result<String, String> {
    if let Some(error) = extract_openai_status_error(result) {
        return Err(error);
    }

    let mut text_parts: Vec<String> = Vec::new();
    let mut refusal_parts: Vec<String> = Vec::new();
    if let Some(output) = result.get("output").and_then(|v| v.as_array()) {
        for item in output {
            if item.get("type").and_then(|v| v.as_str()) != Some("message") {
                continue;
            }
            if let Some(content) = item.get("content").and_then(|v| v.as_array()) {
                for block in content {
                    match block.get("type").and_then(|v| v.as_str()) {
                        Some("output_text") => {
                            if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                                text_parts.push(text.to_string());
                            }
                        }
                        Some("refusal") => {
                            if let Some(refusal) = block.get("refusal").and_then(|v| v.as_str()) {
                                refusal_parts.push(refusal.to_string());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    if text_parts.is_empty() {
        if !refusal_parts.is_empty() {
            return Err(format!("OpenAI refusal: {}", refusal_parts.join("\n")));
        }
        return Err(format!("No text in OpenAI response: {}", result));
    }

    normalize_openai_output_text(&text_parts.join("\n"))
}

fn process_openai_stream_event(
    event_name: Option<&str>,
    data: &str,
    state: &mut OpenAiStreamState,
) -> Result<(), String> {
    let trimmed = data.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    if trimmed == "[DONE]" {
        state.terminal_event_seen = true;
        return Ok(());
    }

    let event: Value = serde_json::from_str(trimmed)
        .map_err(|error| format!("OpenAI SSE JSON parse error: {error}; payload={trimmed}"))?;
    let event_type = event
        .get("type")
        .and_then(|value| value.as_str())
        .or(event_name)
        .unwrap_or("");
    state.event_count += 1;

    match event_type {
        "response.output_text.delta" => {
            if let Some(delta) = event.get("delta").and_then(|value| value.as_str()) {
                state.output_text.push_str(delta);
            }
        }
        "response.output_text.done" => {
            if state.output_text.is_empty() {
                if let Some(text) = event.get("text").and_then(|value| value.as_str()) {
                    state.output_text.push_str(text);
                }
            }
        }
        "response.refusal.delta" => {
            if let Some(delta) = event.get("delta").and_then(|value| value.as_str()) {
                state.refusal_text.push_str(delta);
            }
        }
        "response.refusal.done" => {
            if let Some(refusal) = event.get("refusal").and_then(|value| value.as_str()) {
                state.refusal_text.push_str(refusal);
            }
        }
        "response.error" => {
            state.last_error = Some(
                event
                    .get("error")
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| event.to_string()),
            );
        }
        "response.completed" | "response.failed" | "response.incomplete" => {
            state.terminal_event_seen = true;
            state.final_response = Some(
                event
                    .get("response")
                    .cloned()
                    .unwrap_or_else(|| event.clone()),
            );
        }
        _ => {}
    }

    Ok(())
}

fn finalize_openai_stream(state: &OpenAiStreamState) -> Result<String, String> {
    if let Some(error) = state.last_error.as_ref() {
        return Err(format!("OpenAI stream error: {error}"));
    }

    if let Some(final_response) = state.final_response.as_ref() {
        log_openai_response_metadata(final_response);
        if let Some(error) = extract_openai_status_error(final_response) {
            return Err(error);
        }
    }

    if !state.refusal_text.is_empty() {
        return Err(format!("OpenAI refusal: {}", state.refusal_text));
    }

    if !state.output_text.is_empty() {
        let normalized = normalize_openai_output_text(&state.output_text)?;
        log::info!(
            "verify_pinout: OpenAI streaming capture completed events={} output_chars={}",
            state.event_count,
            state.output_text.chars().count()
        );
        return Ok(normalized);
    }

    if let Some(final_response) = state.final_response.as_ref() {
        return extract_openai_text(final_response);
    }

    Err("OpenAI stream ended without any output_text or completed response".to_string())
}

fn parse_openai_stream_reader<R: BufRead>(mut reader: R) -> Result<String, String> {
    let mut event_name: Option<String> = None;
    let mut data_lines: Vec<String> = Vec::new();
    let mut line = String::new();
    let mut state = OpenAiStreamState::default();

    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .map_err(|error| format!("OpenAI stream read error: {error}"))?;
        if bytes_read == 0 {
            break;
        }

        let trimmed_line = line.trim_end_matches(['\r', '\n']);
        if trimmed_line.is_empty() {
            if !data_lines.is_empty() || event_name.is_some() {
                let payload = data_lines.join("\n");
                process_openai_stream_event(event_name.as_deref(), &payload, &mut state)?;
                if state.terminal_event_seen {
                    return finalize_openai_stream(&state);
                }
            }
            event_name = None;
            data_lines.clear();
            continue;
        }

        if trimmed_line.starts_with(':') {
            continue;
        }

        if let Some(name) = trimmed_line.strip_prefix("event:") {
            event_name = Some(name.trim().to_string());
            continue;
        }

        if let Some(data) = trimmed_line.strip_prefix("data:") {
            data_lines.push(data.trim_start().to_string());
        }
    }

    if !data_lines.is_empty() || event_name.is_some() {
        let payload = data_lines.join("\n");
        process_openai_stream_event(event_name.as_deref(), &payload, &mut state)?;
        if state.terminal_event_seen {
            return finalize_openai_stream(&state);
        }
    }

    finalize_openai_stream(&state)
}

fn read_openai_stream(resp: reqwest::blocking::Response) -> Result<String, String> {
    parse_openai_stream_reader(BufReader::new(resp))
}

fn openai_client(timeout_secs: u64) -> Result<reqwest::blocking::Client, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    Ok(client)
}

fn anthropic_client(timeout_secs: u64) -> Result<reqwest::blocking::Client, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(|error| format!("HTTP client error: {error}"))?;

    Ok(client)
}

fn upload_anthropic_file(
    client: &reqwest::blocking::Client,
    api_key: &str,
    bytes: &[u8],
    filename: &str,
    media_type: &str,
) -> Result<String, String> {
    let start = Instant::now();
    let file_part = reqwest::blocking::multipart::Part::bytes(bytes.to_vec())
        .file_name(filename.to_string())
        .mime_str(media_type)
        .map_err(|error| format!("Failed to prepare Anthropic upload part: {error}"))?;
    let form = reqwest::blocking::multipart::Form::new().part("file", file_part);

    let resp = client
        .post(ANTHROPIC_FILES_API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_API_VERSION)
        .header("anthropic-beta", ANTHROPIC_FILES_API_BETA)
        .multipart(form)
        .send()
        .map_err(|error| format!("Anthropic file upload error: {error}"))?;
    let elapsed_ms = start.elapsed().as_millis();
    log::info!(
        "verify_pinout: Anthropic file upload completed filename={} size_bytes={} media_type={} status={} elapsed_ms={}",
        filename,
        bytes.len(),
        media_type,
        resp.status(),
        elapsed_ms
    );

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("Anthropic file upload error {}: {}", status, body));
    }

    let result: Value = resp
        .json()
        .map_err(|error| format!("Anthropic upload JSON parse error: {error}"))?;
    result
        .get("id")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .ok_or_else(|| format!("No file id in Anthropic upload response: {}", result))
}

fn delete_anthropic_files_best_effort(
    client: &reqwest::blocking::Client,
    api_key: &str,
    file_ids: &[String],
) {
    for file_id in file_ids {
        let start = Instant::now();
        match client
            .delete(format!("{}/{}", ANTHROPIC_FILES_API_URL, file_id))
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_API_VERSION)
            .header("anthropic-beta", ANTHROPIC_FILES_API_BETA)
            .send()
        {
            Ok(response) if response.status().is_success() => {
                log::info!(
                    "verify_pinout: deleted Anthropic upload {} status={} elapsed_ms={}",
                    file_id,
                    response.status(),
                    start.elapsed().as_millis()
                );
            }
            Ok(response) => {
                let status = response.status();
                let body = response.text().unwrap_or_default();
                log::warn!(
                    "verify_pinout: failed to delete Anthropic upload {} ({}): {}",
                    file_id,
                    status,
                    body
                );
            }
            Err(error) => {
                log::warn!(
                    "verify_pinout: failed to delete Anthropic upload {}: {}",
                    file_id,
                    error
                );
            }
        }
    }
}

fn upload_openai_file(
    client: &reqwest::blocking::Client,
    api_key: &str,
    bytes: &[u8],
    filename: &str,
    purpose: &str,
    media_type: &str,
) -> Result<String, String> {
    let start = Instant::now();
    let file_part = reqwest::blocking::multipart::Part::bytes(bytes.to_vec())
        .file_name(filename.to_string())
        .mime_str(media_type)
        .map_err(|error| format!("Failed to prepare upload part: {error}"))?;
    let form = reqwest::blocking::multipart::Form::new()
        .text("purpose", purpose.to_string())
        .part("file", file_part);

    let resp = client
        .post(OPENAI_FILES_API_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .multipart(form)
        .send()
        .map_err(|error| format!("File upload error: {error}"))?;
    let elapsed_ms = start.elapsed().as_millis();
    log::info!(
        "verify_pinout: OpenAI file upload completed filename={} size_bytes={} purpose={} media_type={} status={} elapsed_ms={}",
        filename,
        bytes.len(),
        purpose,
        media_type,
        resp.status(),
        elapsed_ms
    );

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("OpenAI file upload error {}: {}", status, body));
    }

    let result: Value = resp
        .json()
        .map_err(|error| format!("Upload JSON parse error: {error}"))?;
    result
        .get("id")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .ok_or_else(|| format!("No file id in upload response: {}", result))
}

fn log_openai_response_metadata(result: &Value) {
    log::info!(
        "verify_pinout: OpenAI response id={} status={} usage={}",
        result
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or("?"),
        result
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or("?"),
        result
            .get("usage")
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_string())
    );
}

fn send_openai_request(
    client: &reqwest::blocking::Client,
    payload: Value,
    api_key: &str,
) -> Result<String, String> {
    let start = Instant::now();
    let body = serde_json::to_vec(&payload).unwrap();
    let model_name = payload
        .get("model")
        .and_then(|value| value.as_str())
        .unwrap_or(OPENAI_MODEL);
    let reasoning_effort = payload
        .get("reasoning")
        .and_then(|value| value.get("effort"))
        .and_then(|value| value.as_str())
        .unwrap_or("default");
    log::info!(
        "verify_pinout: OpenAI request starting model={} reasoning={} request_bytes={}",
        model_name,
        reasoning_effort,
        body.len()
    );
    let resp = client
        .post(OPENAI_API_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("content-type", "application/json")
        .header("accept", "text/event-stream")
        .body(body)
        .send()
        .map_err(|e| format!("API request error: {e}"))?;
    let elapsed_ms = start.elapsed().as_millis();
    log::info!(
        "verify_pinout: OpenAI response stream opened status={} elapsed_ms={}",
        resp.status(),
        elapsed_ms
    );

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("OpenAI API error {}: {}", status, body));
    }

    let stream_result = read_openai_stream(resp);
    match &stream_result {
        Ok(_) => {
            log::info!(
                "verify_pinout: OpenAI response stream finished total_elapsed_ms={}",
                start.elapsed().as_millis()
            );
        }
        Err(error) => {
            log::warn!(
                "verify_pinout: OpenAI response stream failed total_elapsed_ms={} error={}",
                start.elapsed().as_millis(),
                error
            );
        }
    }
    stream_result
}

fn send_anthropic_request(
    client: &reqwest::blocking::Client,
    payload: Value,
    api_key: &str,
) -> Result<Value, String> {
    let start = Instant::now();
    let resp = client
        .post(ANTHROPIC_API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_API_VERSION)
        .header("anthropic-beta", ANTHROPIC_FILES_API_BETA)
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&payload).unwrap())
        .send()
        .map_err(|error| format!("API request error: {error}"))?;
    let elapsed_ms = start.elapsed().as_millis();
    log::info!(
        "verify_pinout: Anthropic messages call completed status={} elapsed_ms={}",
        resp.status(),
        elapsed_ms
    );

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("Anthropic API error {}: {}", status, body));
    }

    resp.json()
        .map_err(|error| format!("JSON parse error: {error}"))
}

fn verification_output_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "packages": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "package_name": { "type": "string" },
                        "pin_count": { "type": "integer" },
                        "pins": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "pin_number": { "type": "integer" },
                                    "pad_name": { "type": "string" }
                                },
                                "required": ["pin_number", "pad_name"],
                                "additionalProperties": false
                            }
                        },
                        "pin_functions": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "pad_name": { "type": "string" },
                                    "functions": {
                                        "type": "array",
                                        "items": { "type": "string" }
                                    }
                                },
                                "required": ["pad_name", "functions"],
                                "additionalProperties": false
                            }
                        }
                    },
                    "required": ["package_name", "pin_count", "pins", "pin_functions"],
                    "additionalProperties": false
                }
            },
            "corrections": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "pin_position": { "type": "integer" },
                        "package": { "type": "string" },
                        "current_pad": { "type": "string" },
                        "datasheet_pad": { "type": "string" },
                        "type": { "type": "string" },
                        "note": { "type": "string" }
                    },
                    "required": ["pin_position", "package", "current_pad", "datasheet_pad", "type", "note"],
                    "additionalProperties": false
                }
            },
            "clc_input_sources": {
                "type": "array",
                "items": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            },
            "notes": {
                "type": "array",
                "items": { "type": "string" }
            }
        },
        "required": ["packages", "corrections", "clc_input_sources", "notes"],
        "additionalProperties": false
    })
}

fn anthropic_verification_tool() -> Value {
    serde_json::json!({
        "name": ANTHROPIC_VERIFICATION_TOOL_NAME,
        "description": "Return the extracted package pin tables, discrepancy list, optional CLC input sources, and notes as structured verification data.",
        "strict": true,
        "input_schema": verification_output_schema()
    })
}

fn normalize_structured_verification_output(input: &Value) -> Value {
    let mut normalized = serde_json::Map::new();

    let mut packages_out = serde_json::Map::new();
    if let Some(packages) = input.get("packages").and_then(|value| value.as_array()) {
        for package in packages {
            let Some(package_name) = package.get("package_name").and_then(|value| value.as_str())
            else {
                continue;
            };

            let pin_count = package
                .get("pin_count")
                .cloned()
                .unwrap_or_else(|| Value::Number(0u64.into()));

            let mut pins = serde_json::Map::new();
            if let Some(pin_items) = package.get("pins").and_then(|value| value.as_array()) {
                for pin in pin_items {
                    let Some(pin_number) = pin.get("pin_number").and_then(|value| value.as_u64())
                    else {
                        continue;
                    };
                    let Some(pad_name) = pin.get("pad_name").and_then(|value| value.as_str())
                    else {
                        continue;
                    };
                    pins.insert(pin_number.to_string(), Value::String(pad_name.to_string()));
                }
            }

            let mut pin_functions = serde_json::Map::new();
            if let Some(function_items) = package
                .get("pin_functions")
                .and_then(|value| value.as_array())
            {
                for function_item in function_items {
                    let Some(pad_name) = function_item
                        .get("pad_name")
                        .and_then(|value| value.as_str())
                    else {
                        continue;
                    };
                    let functions = function_item
                        .get("functions")
                        .and_then(|value| value.as_array())
                        .cloned()
                        .unwrap_or_default();
                    pin_functions.insert(pad_name.to_string(), Value::Array(functions));
                }
            }

            packages_out.insert(
                package_name.to_string(),
                Value::Object(
                    [
                        ("pin_count".to_string(), pin_count),
                        ("pins".to_string(), Value::Object(pins)),
                        ("pin_functions".to_string(), Value::Object(pin_functions)),
                    ]
                    .into_iter()
                    .collect(),
                ),
            );
        }
    }
    normalized.insert("packages".to_string(), Value::Object(packages_out));

    normalized.insert(
        "corrections".to_string(),
        input
            .get("corrections")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new())),
    );
    normalized.insert(
        "notes".to_string(),
        input
            .get("notes")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new())),
    );

    normalized.insert(
        "clc_input_sources".to_string(),
        input
            .get("clc_input_sources")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new())),
    );

    Value::Object(normalized)
}

fn openai_verification_text_format() -> Value {
    serde_json::json!({
        "format": {
            "type": "json_schema",
            "name": "pinout_verification",
            "schema": verification_output_schema(),
            "strict": true
        }
    })
}

fn extract_anthropic_response_text(result: &Value) -> String {
    let mut text_parts: Vec<String> = Vec::new();
    if let Some(content) = result.get("content").and_then(|v: &Value| v.as_array()) {
        for block in content {
            if block.get("type").and_then(|v: &Value| v.as_str()) == Some("text") {
                if let Some(text) = block.get("text").and_then(|v: &Value| v.as_str()) {
                    text_parts.push(text.to_string());
                }
            }
        }
    }
    text_parts.join("\n")
}

fn extract_anthropic_verification_output(result: &Value) -> Result<String, String> {
    if let Some(content) = result.get("content").and_then(|value| value.as_array()) {
        for block in content {
            if block.get("type").and_then(|value| value.as_str()) != Some("tool_use") {
                continue;
            }
            if block.get("name").and_then(|value| value.as_str())
                != Some(ANTHROPIC_VERIFICATION_TOOL_NAME)
            {
                continue;
            }
            if let Some(input) = block.get("input") {
                return serde_json::to_string(&normalize_structured_verification_output(input))
                    .map_err(|error| {
                        format!("Failed to serialize Anthropic tool output: {error}")
                    });
            }
        }
    }

    let text = extract_anthropic_response_text(result);
    if text.is_empty() {
        Err(format!(
            "Anthropic response contained neither tool output nor text: {}",
            result
        ))
    } else {
        Ok(text)
    }
}

fn call_anthropic_api(
    pdf_bytes: &[u8],
    page_spans: &[PageSpan],
    task: VerifyTask,
    prompt: &str,
    api_key: &str,
    part_number: &str,
    progress: Option<&ProgressCallback>,
) -> Result<String, String> {
    let trimmed = prepare_pdf(pdf_bytes);
    let client = anthropic_client(300)?;
    let filename = format!("{}-verification.pdf", part_number.to_uppercase());
    emit_progress(
        progress,
        VerifyProgressUpdate::new(
            "provider.upload",
            0.58,
            format!(
                "Uploading reduced {} to {}",
                task_sections_label(task),
                provider_name(Provider::Anthropic)
            ),
        )
        .detail(format!(
            "Uploading {} page(s) from the selected datasheet ranges: {}.",
            page_spans
                .iter()
                .map(|span| span.end.saturating_sub(span.start) + 1)
                .sum::<u32>(),
            describe_page_spans(page_spans)
        ))
        .provider(Provider::Anthropic),
    );
    let file_id = upload_anthropic_file(&client, api_key, &trimmed, &filename, "application/pdf")?;
    let uploaded_file_ids = vec![file_id.clone()];
    emit_progress(
        progress,
        VerifyProgressUpdate::new(
            "provider.analyze",
            0.76,
            format!(
                "Anthropic is analyzing the reduced {}",
                task_sections_label(task)
            ),
        )
        .detail(provider_analysis_hint(Provider::Anthropic, task))
        .indeterminate(true)
        .provider(Provider::Anthropic),
    );

    let payload = serde_json::json!({
        "model": anthropic_model(),
        "max_tokens": MAX_TOKENS,
        "tools": [anthropic_verification_tool()],
        "tool_choice": {
            "type": "tool",
            "name": ANTHROPIC_VERIFICATION_TOOL_NAME
        },
        "messages": [{
            "role": "user",
            "content": [
                {
                    "type": "text",
                    "text": format!(
                        "{prompt}\n\nThe attached PDF was reduced to the relevant datasheet sections only. Included page ranges: {}.",
                        describe_page_spans(page_spans)
                    )
                },
                {
                    "type": "document",
                    "source": {
                        "type": "file",
                        "file_id": file_id
                    }
                }
            ]
        }]
    });

    let response = send_anthropic_request(&client, payload, api_key)
        .and_then(|result| extract_anthropic_verification_output(&result));
    delete_anthropic_files_best_effort(&client, api_key, &uploaded_file_ids);
    response
}

fn call_anthropic_image_api(
    pdf_bytes: &[u8],
    page_spans: &[PageSpan],
    task: VerifyTask,
    prompt: &str,
    api_key: &str,
    progress: Option<&ProgressCallback>,
) -> Result<String, String> {
    emit_progress(
        progress,
        VerifyProgressUpdate::new(
            "provider.render",
            0.62,
            format!(
                "Rendering selected {} as 300 DPI PNGs",
                task_sections_label(task)
            ),
        )
        .detail("Retrying with images because the reduced-PDF path was unavailable.")
        .provider(Provider::Anthropic),
    );
    let rendered_images = render_pages_to_pngs(pdf_bytes, page_spans)?;
    log::info!(
        "verify_pinout: rendered {} fallback PNG pages at 300 DPI for Anthropic ({})",
        rendered_images.len(),
        describe_page_spans(page_spans)
    );

    let client = anthropic_client(300)?;
    emit_progress(
        progress,
        VerifyProgressUpdate::new(
            "provider.upload",
            0.68,
            format!(
                "Uploading rendered {} to Anthropic",
                task_sections_label(task)
            ),
        )
        .detail(format!(
            "Retrying with {} PNG page(s) from the reduced datasheet ranges: {}.",
            rendered_images.len(),
            describe_page_spans(page_spans)
        ))
        .provider(Provider::Anthropic),
    );
    let mut content = Vec::with_capacity(rendered_images.len() + 1);
    content.push(serde_json::json!({
        "type": "text",
        "text": format!(
            "{prompt}\n\nThe datasheet was rendered as 300 DPI PNG images for these page ranges: {}.",
            describe_page_spans(page_spans)
        )
    }));

    let mut uploaded_file_ids = Vec::with_capacity(rendered_images.len());
    for image in rendered_images {
        let file_id = match upload_anthropic_file(
            &client,
            api_key,
            &image.bytes,
            &format!("datasheet-page-{:04}.png", image.page_number),
            PNG_IMAGE_MEDIA_TYPE,
        ) {
            Ok(file_id) => file_id,
            Err(error) => {
                delete_anthropic_files_best_effort(&client, api_key, &uploaded_file_ids);
                return Err(error);
            }
        };
        uploaded_file_ids.push(file_id.clone());
        content.push(serde_json::json!({
            "type": "image",
            "source": {
                "type": "file",
                "file_id": file_id
            }
        }));
    }

    let payload = serde_json::json!({
        "model": anthropic_model(),
        "max_tokens": MAX_TOKENS,
        "tools": [anthropic_verification_tool()],
        "tool_choice": {
            "type": "tool",
            "name": ANTHROPIC_VERIFICATION_TOOL_NAME
        },
        "messages": [{
            "role": "user",
            "content": content
        }]
    });
    emit_progress(
        progress,
        VerifyProgressUpdate::new(
            "provider.analyze",
            0.8,
            format!(
                "Anthropic is analyzing the rendered {}",
                task_sections_label(task)
            ),
        )
        .detail(provider_analysis_hint(Provider::Anthropic, task))
        .indeterminate(true)
        .provider(Provider::Anthropic),
    );

    let response = send_anthropic_request(&client, payload, api_key)
        .and_then(|result| extract_anthropic_verification_output(&result));
    delete_anthropic_files_best_effort(&client, api_key, &uploaded_file_ids);
    response
}

fn call_openai_image_api(
    pdf_bytes: &[u8],
    page_spans: &[PageSpan],
    task: VerifyTask,
    prompt: &str,
    api_key: &str,
    part_number: &str,
    progress: Option<&ProgressCallback>,
) -> Result<String, String> {
    let model_name = openai_model();
    let reasoning_effort = openai_reasoning_effort();
    emit_progress(
        progress,
        VerifyProgressUpdate::new(
            "provider.render",
            0.62,
            format!(
                "Rendering selected {} as 300 DPI PNGs",
                task_sections_label(task)
            ),
        )
        .detail("Retrying with images because the reduced-PDF path was unavailable.")
        .provider(Provider::OpenAI),
    );
    let rendered_images = render_pages_to_pngs(pdf_bytes, page_spans)?;
    let total_png_bytes: usize = rendered_images.iter().map(|image| image.bytes.len()).sum();
    log::info!(
        "verify_pinout: rendered {} fallback PNG pages at 300 DPI for OpenAI ({}) total_png_mb={:.1}",
        rendered_images.len(),
        describe_page_spans(page_spans),
        total_png_bytes as f64 / 1_048_576.0
    );

    let client = openai_client(600)?;
    emit_progress(
        progress,
        VerifyProgressUpdate::new(
            "provider.upload",
            0.68,
            format!(
                "Uploading rendered {} to OpenAI",
                task_sections_label(task)
            ),
        )
        .detail(format!(
            "Retrying with {} PNG page(s) from the reduced datasheet ranges: {}.",
            rendered_images.len(),
            describe_page_spans(page_spans)
        ))
        .provider(Provider::OpenAI),
    );
    let mut content = Vec::with_capacity(rendered_images.len() + 1);
    content.push(serde_json::json!({
        "type": "input_text",
        "text": format!(
            "{prompt}\n\nThe datasheet was rendered as 300 DPI PNG images for these page ranges: {}.",
            describe_page_spans(page_spans)
        )
    }));

    for image in rendered_images {
        let file_id = upload_openai_file(
            &client,
            api_key,
            &image.bytes,
            &format!(
                "{}-page-{:04}.png",
                part_number.to_uppercase(),
                image.page_number
            ),
            "vision",
            PNG_IMAGE_MEDIA_TYPE,
        )?;
        content.push(serde_json::json!({
            "type": "input_image",
            "file_id": file_id
        }));
    }

    let payload = serde_json::json!({
        "model": model_name,
        "instructions": openai_image_instructions(task),
        "input": [
            {
                "role": "user",
                "content": content
            }
        ],
        "text": openai_verification_text_format(),
        "reasoning": { "effort": reasoning_effort },
        "max_output_tokens": OPENAI_MAX_OUTPUT_TOKENS,
        "stream": true
    });
    emit_progress(
        progress,
        VerifyProgressUpdate::new(
            "provider.analyze",
            0.8,
            format!(
                "OpenAI is analyzing the rendered {}",
                task_sections_label(task)
            ),
        )
        .detail(provider_analysis_hint(Provider::OpenAI, task))
        .indeterminate(true)
        .provider(Provider::OpenAI),
    );

    send_openai_request(&client, payload, api_key)
}

fn call_openai_api(
    pdf_bytes: &[u8],
    datasheet_text: Option<&str>,
    task: VerifyTask,
    prompt: &str,
    api_key: &str,
    part_number: &str,
    progress: Option<&ProgressCallback>,
) -> Result<String, String> {
    let model_name = openai_model();
    let reasoning_effort = openai_reasoning_effort();
    if pdf_bytes.is_empty() {
        let detail = if datasheet_text.is_some() {
            " Text-only datasheet fallback is disabled."
        } else {
            ""
        };
        return Err(format!(
            "Verification requires a datasheet PDF so pickle can send a reduced PDF or rendered page images.{detail}"
        ));
    }

    emit_progress(
        progress,
        VerifyProgressUpdate::new(
            "datasheet.reduce",
            0.44,
            task_reduce_progress_label(task),
        )
        .detail(task_reduce_progress_detail(task)),
    );
    let (page_spans, source_pages) = relevant_page_spans_for_pdf(pdf_bytes, task)?;
    let range_description = describe_page_spans(&page_spans);

    let reduced_pdf = match reduce_pdf_with_bookmarks(pdf_bytes, task) {
        Ok(reduced_pdf) => reduced_pdf,
        Err(error) => {
            log::warn!(
                "verify_pinout: reduced {} PDF generation failed for OpenAI, falling back to rendered page images: {}",
                task_label(task),
                error
            );
            return call_openai_image_api(
                pdf_bytes,
                &page_spans,
                task,
                prompt,
                api_key,
                part_number,
                progress,
            );
        }
    };

    log::info!(
        "verify_pinout: reduced {} PDF from {} pages / {:.1} MB to {} pages / {:.1} MB using bookmarks ({})",
        task_label(task),
        source_pages,
        pdf_bytes.len() as f64 / 1_048_576.0,
        reduced_pdf.selected_pages(),
        reduced_pdf.bytes.len() as f64 / 1_048_576.0,
        range_description
    );

    if reduced_pdf.bytes.len() > OPENAI_FILE_LIMIT_BYTES {
        log::warn!(
            "verify_pinout: reduced {} PDF is still {:.1} MB, falling back to rendered page images for OpenAI",
            task_label(task),
            reduced_pdf.bytes.len() as f64 / 1_048_576.0
        );
        return call_openai_image_api(
            pdf_bytes,
            &page_spans,
            task,
            prompt,
            api_key,
            part_number,
            progress,
        );
    }

    let client = openai_client(600)?;
    let filename = format!("{}-verification.pdf", part_number.to_uppercase());
    emit_progress(
        progress,
        VerifyProgressUpdate::new(
            "provider.upload",
            0.58,
            format!(
                "Uploading reduced {} to {}",
                task_sections_label(task),
                provider_name(Provider::OpenAI)
            ),
        )
        .detail(format!(
            "Reduced {} pages / {:.1} MB to {} pages / {:.1} MB ({})",
            source_pages,
            pdf_bytes.len() as f64 / 1_048_576.0,
            reduced_pdf.selected_pages(),
            reduced_pdf.bytes.len() as f64 / 1_048_576.0,
            range_description
        ))
        .provider(Provider::OpenAI),
    );
    let file_id = match upload_openai_file(
        &client,
        api_key,
        &reduced_pdf.bytes,
        &filename,
        "user_data",
        "application/pdf",
    ) {
        Ok(file_id) => file_id,
        Err(error) => {
            if should_fallback_openai_to_images(&error) {
                log::warn!(
                    "verify_pinout: OpenAI reduced PDF upload failed with file/path-specific error, falling back to rendered page images: {}",
                    error
                );
                return call_openai_image_api(
                    pdf_bytes,
                    &page_spans,
                    task,
                    prompt,
                    api_key,
                    part_number,
                    progress,
                );
            }
            return Err(error);
        }
    };

    let prompt_with_context = format!(
        "{prompt}\n\nThe attached PDF was reduced to the relevant datasheet sections only. Included page ranges: {}.",
        range_description
    );
    log::info!(
        "verify_pinout: OpenAI prompt stats model={} reasoning={} prompt_chars={} prompt_lines={}",
        model_name,
        reasoning_effort,
        prompt_with_context.chars().count(),
        prompt_with_context.lines().count()
    );
    let payload = serde_json::json!({
        "model": model_name,
        "instructions": openai_instructions(task),
        "input": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "input_file",
                        "file_id": file_id
                    },
                    {
                        "type": "input_text",
                        "text": prompt_with_context
                    }
                ]
            }
        ],
        "text": openai_verification_text_format(),
        "reasoning": { "effort": reasoning_effort },
        "max_output_tokens": OPENAI_MAX_OUTPUT_TOKENS,
        "stream": true
    });
    emit_progress(
        progress,
        VerifyProgressUpdate::new(
            "provider.analyze",
            0.76,
            format!(
                "OpenAI is analyzing the reduced {}",
                task_sections_label(task)
            ),
        )
        .detail(provider_analysis_hint(Provider::OpenAI, task))
        .indeterminate(true)
        .provider(Provider::OpenAI),
    );

    match send_openai_request(&client, payload, api_key) {
        Ok(response) => Ok(response),
        Err(error) => {
            if should_fallback_openai_to_images(&error) {
                log::warn!(
                    "verify_pinout: OpenAI reduced PDF request failed with file/path-specific error, falling back to rendered page images: {}",
                    error
                );
                call_openai_image_api(
                    pdf_bytes,
                    &page_spans,
                    task,
                    prompt,
                    api_key,
                    part_number,
                    progress,
                )
            } else {
                Err(error)
            }
        }
    }
}

/// Dispatch to the appropriate API based on provider.
fn call_llm_api(
    provider: Provider,
    pdf_bytes: &[u8],
    datasheet_text: Option<&str>,
    task: VerifyTask,
    prompt: &str,
    api_key: &str,
    part_number: &str,
    progress: Option<&ProgressCallback>,
) -> Result<String, String> {
    match provider {
        Provider::Anthropic => {
            if pdf_bytes.is_empty() {
                let detail = if datasheet_text.is_some() {
                    " Text-only datasheet fallback is disabled."
                } else {
                    ""
                };
                Err(format!(
                    "Verification requires a datasheet PDF so pickle can send a reduced PDF or rendered page images.{detail}"
                ))
            } else {
                emit_progress(
                    progress,
                    VerifyProgressUpdate::new(
                        "datasheet.reduce",
                        0.44,
                        task_reduce_progress_label(task),
                    )
                    .detail(task_reduce_progress_detail(task)),
                );
                let (page_spans, source_pages) = relevant_page_spans_for_pdf(pdf_bytes, task)?;
                let range_description = describe_page_spans(&page_spans);

                match reduce_pdf_with_bookmarks(pdf_bytes, task) {
                    Ok(reduced_pdf) => {
                        log::info!(
                            "verify_pinout: reduced {} PDF from {} pages / {:.1} MB to {} pages / {:.1} MB for Anthropic ({})",
                            task_label(task),
                            source_pages,
                            pdf_bytes.len() as f64 / 1_048_576.0,
                            reduced_pdf.selected_pages(),
                            reduced_pdf.bytes.len() as f64 / 1_048_576.0,
                            range_description
                        );
                        match call_anthropic_api(
                            &reduced_pdf.bytes,
                            &page_spans,
                            task,
                            prompt,
                            api_key,
                            part_number,
                            progress,
                        ) {
                            Ok(response) => Ok(response),
                            Err(error) => {
                                log::warn!(
                                    "verify_pinout: Anthropic reduced PDF request failed, falling back to rendered page images: {}",
                                    error
                                );
                                call_anthropic_image_api(
                                    pdf_bytes,
                                    &page_spans,
                                    task,
                                    prompt,
                                    api_key,
                                    progress,
                                )
                            }
                        }
                    }
                    Err(error) => {
                        log::warn!(
                            "verify_pinout: reduced PDF generation failed for Anthropic, falling back to rendered page images: {}",
                            error
                        );
                        call_anthropic_image_api(
                            pdf_bytes,
                            &page_spans,
                            task,
                            prompt,
                            api_key,
                            progress,
                        )
                    }
                }
            }
        }
        Provider::OpenAI => call_openai_api(
            pdf_bytes,
            datasheet_text,
            task,
            prompt,
            api_key,
            part_number,
            progress,
        ),
    }
}

/// Collapse package-specific rail aliases like `VDD_2` back to the canonical
/// pad name so overlay/datasheet comparisons ignore duplicated suffixes.
fn normalize_pad(name: &str) -> String {
    let re = Regex::new(r"_\d+$").unwrap();
    let upper = name.to_uppercase();
    re.replace(upper.trim(), "").to_string()
}

fn parse_verifier_response(raw: &str, device_data: &Value) -> VerifyResult {
    let part = device_data
        .get("part_number")
        .and_then(|v| v.as_str())
        .unwrap_or("UNKNOWN")
        .to_string();

    let mut result = VerifyResult {
        part_number: part,
        packages: HashMap::new(),
        notes: Vec::new(),
        clc_input_sources: None,
        raw_response: raw.to_string(),
    };

    // Extract JSON (handle markdown fencing)
    let mut json_str = raw.trim().to_string();
    if json_str.starts_with("```") {
        let lines: Vec<&str> = json_str.lines().collect();
        let start = lines
            .iter()
            .position(|l| l.trim().starts_with('{'))
            .unwrap_or(1);
        let end = lines
            .iter()
            .rposition(|l| l.trim().starts_with('}'))
            .unwrap_or(lines.len() - 1);
        json_str = lines[start..=end].join("\n");
    }

    let data: Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => {
            result
                .notes
                .push(format!("Failed to parse verifier response as JSON: {}", e));
            return result;
        }
    };

    // Build current pins map for match scoring
    let current_pins: HashMap<u64, &Value> = device_data
        .get("pins")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    p.get("position")
                        .and_then(|v| v.as_u64())
                        .map(|pos| (pos, p))
                })
                .collect()
        })
        .unwrap_or_default();

    if let Some(packages) = data.get("packages").and_then(|v| v.as_object()) {
        for (pkg_name, pkg_data) in packages {
            let mut pins: HashMap<u32, String> = HashMap::new();
            if let Some(pin_obj) = pkg_data.get("pins").and_then(|v| v.as_object()) {
                for (pos_str, pad) in pin_obj {
                    if let (Ok(pos), Some(pad_str)) = (pos_str.parse::<u32>(), pad.as_str()) {
                        pins.insert(pos, pad_str.to_string());
                    }
                }
            }

            let pin_functions: HashMap<String, Vec<String>> = pkg_data
                .get("pin_functions")
                .and_then(|v| v.as_object())
                .map(|obj| {
                    obj.iter()
                        .map(|(k, v)| {
                            let funcs = v
                                .as_array()
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|s| s.as_str().map(|s| s.to_string()))
                                        .collect()
                                })
                                .unwrap_or_default();
                            (k.clone(), funcs)
                        })
                        .collect()
                })
                .unwrap_or_default();

            let pin_count = pkg_data
                .get("pin_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(pins.len() as u64) as u32;

            // Calculate match score
            let mut matches = 0u32;
            let mut total = 0u32;
            for (pos, pad) in &pins {
                if let Some(current) = current_pins.get(&(*pos as u64)) {
                    total += 1;
                    let current_pad = current
                        .get("pad_name")
                        .or_else(|| current.get("pad"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if normalize_pad(pad) == normalize_pad(current_pad) {
                        matches += 1;
                    }
                }
            }
            let match_score = if total > 0 {
                matches as f64 / total as f64
            } else {
                0.0
            };

            result.packages.insert(
                pkg_name.clone(),
                PackageResult {
                    package_name: pkg_name.clone(),
                    pin_count,
                    pins,
                    pin_functions,
                    corrections: Vec::new(),
                    match_score,
                },
            );
        }
    }

    // Parse corrections
    if let Some(corrections) = data.get("corrections").and_then(|v| v.as_array()) {
        for corr in corrections {
            let pkg_name = corr
                .get("package")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if let Some(pkg) = result.packages.get_mut(&pkg_name) {
                pkg.corrections.push(PinCorrection {
                    pin_position: corr
                        .get("pin_position")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32,
                    current_pad: corr
                        .get("current_pad")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    datasheet_pad: corr
                        .get("datasheet_pad")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    current_functions: Vec::new(),
                    datasheet_functions: Vec::new(),
                    correction_type: corr
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    note: corr
                        .get("note")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                });
            }
        }
    }

    if let Some(notes) = data.get("notes").and_then(|v| v.as_array()) {
        result.notes = notes
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
    }

    // Parse CLC input source mapping if the LLM found the CLCxSEL register
    if let Some(clc_arr) = data.get("clc_input_sources").and_then(|v| v.as_array()) {
        let sources: Vec<Vec<String>> = clc_arr
            .iter()
            .filter_map(|group| {
                group.as_array().map(|g| {
                    g.iter()
                        .filter_map(|s| s.as_str().map(|s| s.to_string()))
                        .collect()
                })
            })
            .collect();
        // Validate shape: must be exactly 4 groups of 8 sources
        if sources.len() == 4 && sources.iter().all(|g| g.len() == 8) {
            result.clc_input_sources = Some(sources);
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Verification result cache — keyed by PDF content hash plus request scope so
// provider/prompt variants do not leak across different device views.
// ---------------------------------------------------------------------------

fn verify_cache_dir() -> PathBuf {
    let d = dfp_cache_dir().join("verify_cache");
    let _ = fs::create_dir_all(&d);
    d
}

/// Simple hash of PDF bytes and request scope for cache key.
fn pdf_cache_key(pdf_bytes: &[u8], scope: &str) -> String {
    // Use a simple FNV-style hash — no crypto needed, just deduplication.
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in pdf_bytes.iter().take(65536) {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    // Also mix in the length for uniqueness
    h ^= pdf_bytes.len() as u64;
    h = h.wrapping_mul(0x100000001b3);
    for &b in scope.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h ^= scope.len() as u64;
    h = h.wrapping_mul(0x100000001b3);
    format!("{:016x}", h)
}

fn load_cached_verify(pdf_bytes: &[u8], scope: &str) -> Option<Value> {
    let key = pdf_cache_key(pdf_bytes, scope);
    let path = verify_cache_dir().join(format!("{}.json", key));
    let text = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_cached_verify(pdf_bytes: &[u8], scope: &str, raw_json: &Value) {
    let key = pdf_cache_key(pdf_bytes, scope);
    let path = verify_cache_dir().join(format!("{}.json", key));
    if let Ok(text) = serde_json::to_string_pretty(raw_json) {
        let _ = fs::write(&path, text);
    }
}

fn verify_with_task(
    task: VerifyTask,
    pdf_bytes: &[u8],
    datasheet_text: Option<&str>,
    device_data: &Value,
    api_key: Option<&str>,
    progress: Option<&ProgressCallback>,
) -> Result<VerifyResult, String> {
    let (provider, key) = resolve_provider(api_key)?;
    emit_progress(
        progress,
        VerifyProgressUpdate::new(
            "provider.select",
            0.36,
            format!(
                "Using {} for {} verification",
                provider_name(provider),
                task_label(task)
            ),
        )
        .detail(provider_analysis_hint(provider, task))
        .provider(provider),
    );

    let prompt = build_task_prompt(provider, task, device_data);
    let cache_scope = format!("provider={}|prompt={}", provider_name(provider), prompt);

    if !pdf_bytes.is_empty() && !verify_cache_disabled() {
        if let Some(cached_json) = load_cached_verify(pdf_bytes, &cache_scope) {
            log::info!("verify_pinout: using cached LLM result");
            emit_progress(
                progress,
                VerifyProgressUpdate::new("result.cached", 0.95, "Using cached verification result")
                    .detail("This datasheet was already verified earlier for the same provider and request scope, so no provider call was needed."),
            );
            return Ok(parse_verifier_response(
                &serde_json::to_string(&cached_json).unwrap_or_default(),
                device_data,
            ));
        }
    } else if !pdf_bytes.is_empty() && verify_cache_disabled() {
        log::info!("verify_pinout: verify cache disabled via PICKLE_DISABLE_VERIFY_CACHE");
    }

    let part_number = device_data
        .get("part_number")
        .and_then(|v| v.as_str())
        .unwrap_or("UNKNOWN");

    let raw_response = call_llm_api(
        provider,
        pdf_bytes,
        datasheet_text,
        task,
        &prompt,
        &key,
        part_number,
        progress,
    )?;
    emit_progress(
        progress,
        VerifyProgressUpdate::new(
            "result.process",
            0.94,
            "Processing the structured verification result",
        )
        .detail("Comparing extracted package data against the loaded device and preparing the overlay view."),
    );

    // Cache the raw JSON response for future reuse
    if !pdf_bytes.is_empty() {
        if let Ok(parsed) = serde_json::from_str::<Value>(&raw_response) {
            save_cached_verify(pdf_bytes, &cache_scope, &parsed);
        } else {
            // Try stripping markdown fencing before caching
            let cleaned = raw_response.trim();
            if cleaned.starts_with("```") {
                let lines: Vec<&str> = cleaned.lines().collect();
                let start = lines
                    .iter()
                    .position(|l| l.trim().starts_with('{'))
                    .unwrap_or(1);
                let end = lines
                    .iter()
                    .rposition(|l| l.trim().starts_with('}'))
                    .unwrap_or(lines.len() - 1);
                let json_str = lines[start..=end].join("\n");
                if let Ok(parsed) = serde_json::from_str::<Value>(&json_str) {
                    save_cached_verify(pdf_bytes, &cache_scope, &parsed);
                }
            }
        }
    }

    Ok(parse_verifier_response(&raw_response, device_data))
}

pub fn verify_pinout(
    pdf_bytes: &[u8],
    datasheet_text: Option<&str>,
    device_data: &Value,
    api_key: Option<&str>,
    progress: Option<&ProgressCallback>,
) -> Result<VerifyResult, String> {
    verify_with_task(
        VerifyTask::Pinout,
        pdf_bytes,
        datasheet_text,
        device_data,
        api_key,
        progress,
    )
}

pub fn verify_clc(
    pdf_bytes: &[u8],
    datasheet_text: Option<&str>,
    device_data: &Value,
    api_key: Option<&str>,
    progress: Option<&ProgressCallback>,
) -> Result<VerifyResult, String> {
    match verify_with_task(
        VerifyTask::Clc,
        pdf_bytes,
        datasheet_text,
        device_data,
        api_key,
        progress,
    ) {
        Ok(result) => Ok(result),
        Err(error) if error.contains("No bookmark or text ranges found for the CLC section") => {
            Ok(VerifyResult {
                part_number: device_data
                    .get("part_number")
                    .and_then(|value| value.as_str())
                    .unwrap_or("UNKNOWN")
                    .to_string(),
                packages: HashMap::new(),
                notes: vec![
                    "No CLC section could be located in this datasheet by bookmark or page-text scan, so no background CLC extraction was run."
                        .to_string(),
                ],
                clc_input_sources: None,
                raw_response: String::new(),
            })
        }
        Err(error) => Err(error),
    }
}

pub fn save_overlay(
    part_number: &str,
    verify_result: &VerifyResult,
    selected_packages: Option<&[String]>,
) -> Result<PathBuf, String> {
    let dir = pinouts_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Cannot create pinouts dir: {e}"))?;
    let overlay_path = dir.join(format!("{}.json", part_number.to_uppercase()));

    let mut existing: Value = if overlay_path.exists() {
        let text = fs::read_to_string(&overlay_path).unwrap_or_else(|_| "{}".to_string());
        serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if existing.get("packages").is_none() {
        existing["packages"] = serde_json::json!({});
    }

    let overlay_data = verify_result.to_overlay_json();
    if let Some(new_packages) = overlay_data.get("packages").and_then(|v| v.as_object()) {
        for (pkg_name, pkg_data) in new_packages {
            if let Some(selected) = selected_packages {
                if !selected.contains(pkg_name) {
                    continue;
                }
            }
            existing["packages"][pkg_name] = pkg_data.clone();
        }
    }

    let json = serde_json::to_string_pretty(&existing)
        .map_err(|e| format!("JSON serialize error: {e}"))?;
    fs::write(&overlay_path, json).map_err(|e| format!("Write error: {e}"))?;

    if let Some(ref clc_sources) = verify_result.clc_input_sources {
        let _ = save_clc_sources(part_number, clc_sources);
    }

    Ok(overlay_path)
}

pub fn save_clc_sources(part_number: &str, clc_sources: &[Vec<String>]) -> Result<PathBuf, String> {
    let clc_dir = crate::parser::dfp_manager::clc_sources_dir();
    fs::create_dir_all(&clc_dir).map_err(|error| format!("Cannot create clc_sources dir: {error}"))?;
    let clc_path = clc_dir.join(format!("{}.json", part_number.to_uppercase()));
    let clc_json = serde_json::to_string_pretty(clc_sources)
        .map_err(|error| format!("CLC JSON serialize error: {error}"))?;
    fs::write(&clc_path, clc_json).map_err(|error| format!("CLC write error: {error}"))?;
    Ok(clc_path)
}

#[cfg(test)]
mod tests {
    use super::{
        extract_openai_text, normalize_openai_output_text, parse_openai_stream_reader,
        select_clc_page_spans, select_clc_page_spans_from_text_hits, select_pinout_page_spans,
        BookmarkEntry, PageSpan,
    };
    use serde_json::{json, Value};

    fn bookmark(title: &str, page: u32, depth: usize) -> BookmarkEntry {
        BookmarkEntry {
            title: title.to_string(),
            page,
            depth,
        }
    }

    #[test]
    fn pinout_bookmark_ranges_keep_pin_sections_through_table_of_contents() {
        let bookmarks = vec![
            bookmark("dsPIC33AK128MC106 Product Family", 7, 0),
            bookmark("Pin Diagrams", 9, 0),
            bookmark("Pinout I/O Descriptions", 16, 0),
            bookmark("Terminology Cross Reference", 21, 0),
            bookmark("Table of Contents", 22, 0),
            bookmark("1. Device Overview", 28, 0),
            bookmark("Configurable Logic Cell (CLC)", 1292, 0),
            bookmark("Peripheral Trigger Generator (PTG)", 1311, 0),
        ];

        let spans = select_pinout_page_spans(&bookmarks, 1528);

        assert_eq!(spans, vec![PageSpan { start: 9, end: 21 }]);
    }

    #[test]
    fn clc_bookmark_ranges_keep_only_clc_chapter() {
        let bookmarks = vec![
            bookmark("dsPIC33AK128MC106 Product Family", 7, 0),
            bookmark("Pin Diagrams", 9, 0),
            bookmark("Pinout I/O Descriptions", 16, 0),
            bookmark("Table of Contents", 22, 0),
            bookmark("1. Device Overview", 28, 0),
            bookmark("Configurable Logic Cell (CLC)", 1292, 0),
            bookmark("Peripheral Trigger Generator (PTG)", 1311, 0),
        ];

        let spans = select_clc_page_spans(&bookmarks, 1528);

        assert_eq!(spans, vec![PageSpan { start: 1292, end: 1310 }]);
    }

    #[test]
    fn clc_text_hits_expand_to_a_small_contiguous_span() {
        let spans = select_clc_page_spans_from_text_hits(&[198, 199, 201, 203, 204], 260);

        assert_eq!(spans, vec![PageSpan { start: 197, end: 205 }]);
    }

    #[test]
    fn bookmark_ranges_fall_back_to_first_numbered_chapter_when_toc_is_missing() {
        let bookmarks = vec![
            bookmark("Pin Diagrams", 9, 0),
            bookmark("Pinout I/O Descriptions", 16, 0),
            bookmark("1. Device Overview", 28, 0),
        ];

        let spans = select_pinout_page_spans(&bookmarks, 300);

        assert_eq!(spans, vec![PageSpan { start: 9, end: 27 }]);
    }

    #[test]
    fn nested_bookmark_ranges_include_pinout_descriptions_after_pin_diagrams() {
        let bookmarks = vec![
            bookmark(
                "dsPIC33EPXXXGP50X, dsPIC33EPXXXMC20X/50X and PIC24EPXXXGP/MC20X Product Families",
                2,
                0,
            ),
            bookmark("Pin Diagrams", 5, 1),
            bookmark("Pin Diagrams (Continued)", 6, 1),
            bookmark("Table of Contents", 24, 1),
            bookmark("1.0 Device Overview", 27, 0),
            bookmark("TABLE 1-1: Pinout I/O Descriptions", 28, 1),
            bookmark(
                "2.0 Guidelines for Getting Started with 16-Bit Digital Signal Controllers and Microcontrollers",
                31,
                0,
            ),
        ];

        let spans = select_pinout_page_spans(&bookmarks, 546);

        assert_eq!(
            spans,
            vec![
                PageSpan { start: 5, end: 23 },
                PageSpan { start: 28, end: 30 }
            ]
        );
    }

    #[test]
    fn nested_clc_bookmark_ends_at_next_sibling_or_parent_section() {
        let bookmarks = vec![
            bookmark("Specialized Peripherals", 190, 0),
            bookmark("Configurable Logic Cell (CLC)", 200, 1),
            bookmark("Register 1-1: CLC1CON", 201, 2),
            bookmark("Comparator", 220, 1),
        ];

        let spans = select_clc_page_spans(&bookmarks, 260);

        assert_eq!(
            spans,
            vec![PageSpan {
                start: 200,
                end: 219
            }]
        );
    }

    #[test]
    fn openai_array_output_normalizes_to_internal_object_shape() {
        let raw = json!({
            "packages": [
                {
                    "package_name": "64-PIN VQFN-TQFP",
                    "pin_count": 64,
                    "pins": [
                        { "pin_number": 1, "pad_name": "RA0" },
                        { "pin_number": 2, "pad_name": "RA1" }
                    ],
                    "pin_functions": [
                        { "pad_name": "RA0", "functions": ["RA0", "AN0"] },
                        { "pad_name": "RA1", "functions": ["RA1", "AN1"] }
                    ]
                }
            ],
            "corrections": [],
            "notes": ["ok"]
        })
        .to_string();

        let normalized = normalize_openai_output_text(&raw).unwrap();
        let value: Value = serde_json::from_str(&normalized).unwrap();

        assert_eq!(value["packages"]["64-PIN VQFN-TQFP"]["pin_count"], 64);
        assert_eq!(value["packages"]["64-PIN VQFN-TQFP"]["pins"]["1"], "RA0");
        assert_eq!(
            value["packages"]["64-PIN VQFN-TQFP"]["pin_functions"]["RA1"][1],
            "AN1"
        );
    }

    #[test]
    fn openai_incomplete_response_is_reported_explicitly() {
        let result = json!({
            "id": "resp_test",
            "status": "incomplete",
            "incomplete_details": {
                "reason": "max_output_tokens"
            },
            "output": []
        });

        let error = extract_openai_text(&result).unwrap_err();
        assert!(error.contains("OpenAI response incomplete"));
        assert!(error.contains("max_output_tokens"));
    }

    #[test]
    fn openai_streaming_capture_assembles_and_normalizes_long_json() {
        let raw_json = json!({
            "packages": [
                {
                    "package_name": "28-PIN SPDIP",
                    "pin_count": 28,
                    "pins": [
                        { "pin_number": 1, "pad_name": "MCLR" },
                        { "pin_number": 2, "pad_name": "RA0" }
                    ],
                    "pin_functions": [
                        { "pad_name": "MCLR", "functions": ["MCLR"] },
                        { "pad_name": "RA0", "functions": ["RA0", "AN0"] }
                    ]
                }
            ],
            "corrections": [],
            "notes": ["stream ok"]
        })
        .to_string();
        let split = raw_json.len() / 2;
        let first = &raw_json[..split];
        let second = &raw_json[split..];
        let sse = format!(
            "event: response.output_text.delta\n\
data: {{\"type\":\"response.output_text.delta\",\"delta\":{first:?}}}\n\n\
event: response.output_text.delta\n\
data: {{\"type\":\"response.output_text.delta\",\"delta\":{second:?}}}\n\n\
event: response.completed\n\
data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"resp_test\",\"status\":\"completed\",\"usage\":{{}}}}}}\n\n"
        );

        let normalized =
            parse_openai_stream_reader(std::io::Cursor::new(sse.into_bytes())).unwrap();
        let value: Value = serde_json::from_str(&normalized).unwrap();

        assert_eq!(value["packages"]["28-PIN SPDIP"]["pins"]["2"], "RA0");
        assert_eq!(value["notes"][0], "stream ok");
    }
}
