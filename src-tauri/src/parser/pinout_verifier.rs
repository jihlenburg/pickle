//! Pinout Verifier: cross-check parsed EDC pinout data against the device datasheet
//! using an LLM API (Anthropic or OpenAI). Provider is auto-selected based on which
//! API key is available, with OpenAI preferred when both are present.

use base64::Engine;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::parser::dfp_manager::{dfp_cache_dir, pinouts_dir};

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_API_VERSION: &str = "2023-06-01";
const ANTHROPIC_MODEL: &str = "anthropic-sonnet-4-6";

const OPENAI_API_URL: &str = "https://api.openai.com/v1/responses";
const OPENAI_MODEL: &str = "gpt-5.4";

const MAX_TOKENS: u32 = 16384;

#[derive(Debug, Clone, Copy)]
enum Provider {
    Anthropic,
    OpenAI,
}

fn get_env_key(var_name: &str) -> Option<String> {
    if let Ok(key) = std::env::var(var_name) {
        if !key.is_empty() {
            return Some(key);
        }
    }

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

pub fn get_api_key() -> Option<String> {
    // Return whichever key we have (OpenAI or Anthropic)
    get_env_key("OPENAI_API_KEY").or_else(|| get_env_key("ANTHROPIC_API_KEY"))
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

    // Try OpenAI first, then Anthropic
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

const VERIFY_PROMPT: &str = r#"You are analyzing a Microchip dsPIC33/PIC24 datasheet PDF to extract pin mapping data and CLC input source mappings.

## Where to Look

**Pinout data:** Located between the "Pin Diagrams" section and the Table of Contents. Focus on
the pin function tables (e.g., "28-PIN SSOP COMPLETE PIN FUNCTION DESCRIPTIONS").

**CLC data:** Located in the "Configurable Logic Cell (CLC)" chapter. Find the **CLCxSEL register**
definition — it defines what signals each Data Selection MUX (DS1–DS4) can select from. Each
DSx[2:0] field lists 8 signal sources (values 000–111).

## Task

This datasheet may cover multiple devices in the same family with different pin counts.
Extract ALL package pinout tables you find — the results will be cached and filtered per-device later.

1. Find ALL package pinout tables in this datasheet (e.g., SPDIP, SOIC, SSOP, QFN, TQFP, UQFN, etc.)
2. For each package, extract the COMPLETE pin-to-pad mapping (every pin number → pad name)
3. For each pad, extract ALL listed functions/alternate names
4. Compare against the current parsed data (provided below) and identify any discrepancies
5. Find the CLC chapter and extract the CLCxSEL register's DS1–DS4 input source mappings

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
  "clc_input_sources": [
    ["<DS1 source 0>", "<DS1 source 1>", ..., "<DS1 source 7>"],
    ["<DS2 source 0>", "<DS2 source 1>", ..., "<DS2 source 7>"],
    ["<DS3 source 0>", "<DS3 source 1>", ..., "<DS3 source 7>"],
    ["<DS4 source 0>", "<DS4 source 1>", ..., "<DS4 source 7>"]
  ],
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
- For CLC sources: use short signal names (e.g., "CLCINA", "Fcy", "CLC3OUT", "CMP1", "UART1 TX")
- For CLC sources marked "Reserved" in the register, use the string "Reserved"
- If the datasheet has no CLC chapter or CLCxSEL register, omit the clc_input_sources field entirely
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

/// The VERIFY_PROMPT instructs the LLM to focus on the "Pin Diagrams" section,
/// so we send the full PDF — no client-side page extraction needed.
/// GPT-5.4 and Anthropic both handle large PDFs within their context limits.
fn prepare_pdf(pdf_bytes: &[u8]) -> Vec<u8> {
    pdf_bytes.to_vec()
}

fn call_anthropic_api(pdf_bytes: &[u8], prompt: &str, api_key: &str) -> Result<String, String> {
    let trimmed = prepare_pdf(pdf_bytes);
    let pdf_b64 = base64::engine::general_purpose::STANDARD.encode(&trimmed);

    let payload = serde_json::json!({
        "model": ANTHROPIC_MODEL,
        "max_tokens": MAX_TOKENS,
        "messages": [{
            "role": "user",
            "content": [
                {
                    "type": "document",
                    "source": {
                        "type": "base64",
                        "media_type": "application/pdf",
                        "data": pdf_b64
                    }
                },
                {
                    "type": "text",
                    "text": prompt
                }
            ]
        }]
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp = client
        .post(ANTHROPIC_API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_API_VERSION)
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&payload).unwrap())
        .send()
        .map_err(|e| format!("API request error: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("Anthropic API error {}: {}", status, body));
    }

    let result: Value = resp.json().map_err(|e| format!("JSON parse error: {e}"))?;

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

    Ok(text_parts.join("\n"))
}

fn call_openai_api(pdf_bytes: &[u8], prompt: &str, api_key: &str) -> Result<String, String> {
    let trimmed = prepare_pdf(pdf_bytes);
    let pdf_b64 = base64::engine::general_purpose::STANDARD.encode(&trimmed);
    let file_data = format!("data:application/pdf;base64,{}", pdf_b64);

    // OpenAI Responses API with inline file input and high reasoning effort
    // for complex pinout extraction from datasheet PDFs.
    let payload = serde_json::json!({
        "model": OPENAI_MODEL,
        "instructions": "You are analyzing a Microchip dsPIC33/PIC24 datasheet to extract and verify pin mapping data. Return only valid JSON.",
        "input": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "input_file",
                        "filename": "datasheet.pdf",
                        "file_data": file_data,
                    },
                    {
                        "type": "input_text",
                        "text": prompt
                    }
                ]
            }
        ],
        "reasoning": { "effort": "high" }
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp = client
        .post(OPENAI_API_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&payload).unwrap())
        .send()
        .map_err(|e| format!("API request error: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("OpenAI API error {}: {}", status, body));
    }

    let result: Value = resp.json().map_err(|e| format!("JSON parse error: {e}"))?;

    // Responses API: output[] contains "reasoning" and "message" items.
    // We only want text from "message" items → content[].output_text.text
    let mut text_parts: Vec<String> = Vec::new();
    if let Some(output) = result.get("output").and_then(|v| v.as_array()) {
        for item in output {
            if item.get("type").and_then(|v| v.as_str()) != Some("message") {
                continue;
            }
            if let Some(content) = item.get("content").and_then(|v| v.as_array()) {
                for block in content {
                    if block.get("type").and_then(|v| v.as_str()) == Some("output_text") {
                        if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                            text_parts.push(text.to_string());
                        }
                    }
                }
            }
        }
    }

    if text_parts.is_empty() {
        return Err(format!("No text in OpenAI response: {}", result));
    }

    Ok(text_parts.join("\n"))
}

/// Dispatch to the appropriate API based on provider.
fn call_llm_api(
    provider: Provider,
    pdf_bytes: &[u8],
    prompt: &str,
    api_key: &str,
) -> Result<String, String> {
    match provider {
        Provider::Anthropic => call_anthropic_api(pdf_bytes, prompt, api_key),
        Provider::OpenAI => call_openai_api(pdf_bytes, prompt, api_key),
    }
}

/// Collapse package-specific rail aliases like `VDD_2` back to the canonical
/// pad name so overlay/datasheet comparisons ignore duplicated suffixes.
fn normalize_pad(name: &str) -> String {
    let re = Regex::new(r"_\d+$").unwrap();
    let upper = name.to_uppercase();
    re.replace(upper.trim(), "").to_string()
}

fn parse_anthropic_response(raw: &str, device_data: &Value) -> VerifyResult {
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
                .push(format!("Failed to parse Anthropic response as JSON: {}", e));
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
// Verification result cache — keyed by PDF content hash so the same datasheet
// (which may cover a whole device family) reuses results across devices.
// ---------------------------------------------------------------------------

fn verify_cache_dir() -> PathBuf {
    let d = dfp_cache_dir().join("verify_cache");
    let _ = fs::create_dir_all(&d);
    d
}

/// Simple hash of PDF bytes for cache key (first 8 bytes of a basic hash).
fn pdf_cache_key(pdf_bytes: &[u8]) -> String {
    // Use a simple FNV-style hash — no crypto needed, just deduplication.
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in pdf_bytes.iter().take(65536) {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    // Also mix in the length for uniqueness
    h ^= pdf_bytes.len() as u64;
    h = h.wrapping_mul(0x100000001b3);
    format!("{:016x}", h)
}

fn load_cached_verify(pdf_bytes: &[u8]) -> Option<Value> {
    let key = pdf_cache_key(pdf_bytes);
    let path = verify_cache_dir().join(format!("{}.json", key));
    let text = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_cached_verify(pdf_bytes: &[u8], raw_json: &Value) {
    let key = pdf_cache_key(pdf_bytes);
    let path = verify_cache_dir().join(format!("{}.json", key));
    if let Ok(text) = serde_json::to_string_pretty(raw_json) {
        let _ = fs::write(&path, text);
    }
}

pub fn verify_pinout(
    pdf_bytes: &[u8],
    device_data: &Value,
    api_key: Option<&str>,
) -> Result<VerifyResult, String> {
    // Check cache first — the same datasheet PDF produces the same packages
    // regardless of which family device we're comparing against.
    if let Some(cached_json) = load_cached_verify(pdf_bytes) {
        log::info!("verify_pinout: using cached LLM result");
        return Ok(parse_anthropic_response(
            &serde_json::to_string(&cached_json).unwrap_or_default(),
            device_data,
        ));
    }

    let (provider, key) = resolve_provider(api_key)?;

    let current_summary = build_current_data_summary(device_data);
    let prompt = VERIFY_PROMPT.replace("{current_data}", &current_summary);

    let raw_response = call_llm_api(provider, pdf_bytes, &prompt, &key)?;

    // Cache the raw JSON response for future reuse
    if let Ok(parsed) = serde_json::from_str::<Value>(&raw_response) {
        save_cached_verify(pdf_bytes, &parsed);
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
                save_cached_verify(pdf_bytes, &parsed);
            }
        }
    }

    Ok(parse_anthropic_response(&raw_response, device_data))
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

    // Save CLC input sources alongside the overlay when the LLM extracted them
    if let Some(ref clc_sources) = verify_result.clc_input_sources {
        let clc_dir = crate::parser::dfp_manager::clc_sources_dir();
        let _ = fs::create_dir_all(&clc_dir);
        let clc_path = clc_dir.join(format!("{}.json", part_number.to_uppercase()));
        if let Ok(clc_json) = serde_json::to_string_pretty(clc_sources) {
            let _ = fs::write(&clc_path, clc_json);
        }
    }

    Ok(overlay_path)
}
