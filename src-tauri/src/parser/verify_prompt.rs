//! Verification prompt and task-scope helpers.
//!
//! Prompt wording, task labels, provider labels, and cache-scope decisions are
//! kept separate from provider transport so prompt churn does not leak into the
//! request/response orchestration layer.

use serde_json::Value;

const PINOUT_EXTRACTION_CACHE_SCHEMA: &str = "pinout-extraction-v3";
const CLC_EXTRACTION_CACHE_SCHEMA: &str = "clc-extraction-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Provider {
    Anthropic,
    OpenAI,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerifyTask {
    Pinout,
    Clc,
}

pub(crate) fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Anthropic => "Anthropic",
        Provider::OpenAI => "OpenAI",
    }
}

pub(crate) fn provider_analysis_hint(provider: Provider, task: VerifyTask) -> &'static str {
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

pub(crate) fn task_label(task: VerifyTask) -> &'static str {
    match task {
        VerifyTask::Pinout => "pinout",
        VerifyTask::Clc => "CLC",
    }
}

pub(crate) fn task_sections_label(task: VerifyTask) -> &'static str {
    match task {
        VerifyTask::Pinout => "pinout pages",
        VerifyTask::Clc => "CLC chapter pages",
    }
}

pub(crate) fn task_reduce_progress_label(task: VerifyTask) -> &'static str {
    match task {
        VerifyTask::Pinout => "Trimming the datasheet to pinout pages",
        VerifyTask::Clc => "Trimming the datasheet to CLC pages",
    }
}

pub(crate) fn task_reduce_progress_detail(task: VerifyTask) -> &'static str {
    match task {
        VerifyTask::Pinout => {
            "pickle uploads only the relevant pinout pages instead of the entire family datasheet."
        }
        VerifyTask::Clc => {
            "pickle uploads only the CLC chapter pages for the background CLC lookup."
        }
    }
}

pub(crate) fn openai_instructions(task: VerifyTask) -> &'static str {
    match task {
        VerifyTask::Pinout => {
            "You are analyzing a Microchip dsPIC33/PIC24 datasheet PDF to extract and verify pin mapping data. Return only valid JSON."
        }
        VerifyTask::Clc => {
            "You are analyzing a Microchip dsPIC33/PIC24 datasheet PDF to extract CLC input source mappings. Return only valid JSON."
        }
    }
}

pub(crate) fn openai_image_instructions(task: VerifyTask) -> &'static str {
    match task {
        VerifyTask::Pinout => {
            "You are analyzing rendered datasheet page images to extract and verify pin mapping data. Return only valid JSON."
        }
        VerifyTask::Clc => {
            "You are analyzing rendered datasheet page images to extract CLC input source mappings. Return only valid JSON."
        }
    }
}

const PINOUT_VERIFY_PROMPT: &str = r#"You are analyzing a Microchip dsPIC33/PIC24 datasheet PDF to extract pin mapping data.

## Where to Look

**Pinout data:** Located between the "Pin Diagrams" section and the Table of Contents. Focus on
the pin function tables (e.g., "28-PIN SSOP COMPLETE PIN FUNCTION DESCRIPTIONS").

## Task

This datasheet may cover multiple devices in the same family with different pin counts.
Extract the relevant package pinout tables only. The results will be cached and filtered per-device later.

1. Find package pinout tables in this datasheet whose package pin count matches the target pin count provided below
2. For each package, extract the COMPLETE pin-to-pad mapping (every pin number → pad name)
3. For each pad, extract ALL listed functions/alternate names
4. Return extracted package data only. Do not compare against any current parser data locally held by the app.
5. If the datasheet includes multiple same-pin-count tables for sibling device branches or combined package headings, include all of those same-pin-count tables in this extraction pass.

## Extraction Scope

{current_data}

## Output Format

Return a JSON object with this exact structure (no markdown fencing, just raw JSON):

{{
  "packages": [
    {{
      "package_name": "<PackageName>",
      "pin_count": <int>,
      "pins": [
        {{ "pin_number": <int>, "pad_name": "<pad_name>" }},
        ...
      ],
      "pin_functions": [
        {{ "pad_name": "<pad_name>", "functions": ["func1", "func2", ...] }},
        ...
      ]
    }}
  ],
  "corrections": [],
  "clc_input_sources": [],
  "notes": ["<any general observations about data quality>"]
}}

## Important Guidelines

- Use UPPERCASE for pad names (e.g., "RA0", "RB5", "MCLR", "VDD", "VSS", "AVDD")
- Return `packages` as an ARRAY of package objects, not an object keyed by package name
- Pin numbers must be INTEGER values in each `pins` entry's `pin_number` field
- Include ALL pins including power (VDD, VSS, AVDD, AVSS, VCAP) and special (MCLR)
- For pads with numbered duplicates (multiple VDD pins), use suffixes: VDD, VDD_2, VDD_3, etc.
- Functions should include the primary I/O name (e.g., "RA0"), analog channel (e.g., "AN0"), and any fixed peripheral functions
- If the datasheet shows a package not in the current data, include it as a new entry
- Do not generate discrepancy analysis. Always return "corrections": [] for this extraction-only pass.
- For this pinout-only pass, always return "clc_input_sources": []
- If you found one or more same-pin-count package tables on the supplied pages, `packages` must not be empty
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

fn build_pinout_extraction_scope(device_data: &Value) -> String {
    let mut lines = Vec::new();
    lines.push("Extraction mode: package-table extraction only".to_string());
    let pin_count = device_data
        .get("pin_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    lines.push(format!("Target package pin count: {}", pin_count));
    lines.push(
        "Extract every table on the supplied pages that matches this pin count, even when multiple same-pin-count package headings or sibling device branches appear in the same family datasheet."
            .to_string(),
    );

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

pub(crate) fn build_task_prompt(
    provider: Provider,
    task: VerifyTask,
    device_data: &Value,
) -> String {
    match task {
        VerifyTask::Pinout => {
            let base_prompt = PINOUT_VERIFY_PROMPT.replace(
                "{current_data}",
                &build_pinout_extraction_scope(device_data),
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
            let base_prompt = CLC_VERIFY_PROMPT
                .replace("{device_summary}", &build_clc_device_summary(device_data));
            match provider {
                Provider::Anthropic => base_prompt,
                Provider::OpenAI => format!(
                    "{base_prompt}\n\nOpenAI-specific scope reduction:\n- This is the CLC-only pass, so always return \"packages\": [] and \"corrections\": [].\n- The API enforces a structured response schema. Populate every required field and do not add extra keys."
                ),
            }
        }
    }
}

/// Keep verifier cache scope stable across prompt wording tweaks so pinout
/// extraction results can be reused across sibling parts that share the same
/// datasheet PDF and pin count.
pub(crate) fn verification_cache_scope(
    provider: Provider,
    task: VerifyTask,
    device_data: &Value,
) -> String {
    match task {
        VerifyTask::Pinout => {
            let pin_count = device_data
                .get("pin_count")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            format!(
                "provider={}|schema={}|pin_count={}",
                provider_name(provider),
                PINOUT_EXTRACTION_CACHE_SCHEMA,
                pin_count
            )
        }
        VerifyTask::Clc => format!(
            "provider={}|schema={}",
            provider_name(provider),
            CLC_EXTRACTION_CACHE_SCHEMA
        ),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{build_task_prompt, verification_cache_scope, Provider, VerifyTask};

    #[test]
    fn openai_pinout_prompt_matches_structured_schema_shape() {
        let prompt = build_task_prompt(
            Provider::OpenAI,
            VerifyTask::Pinout,
            &json!({
                "part_number": "DSPIC33AK256MPS205",
                "pin_count": 48
            }),
        );

        assert!(prompt.contains("\"package_name\": \"<PackageName>\""));
        assert!(prompt.contains("\"pin_number\": <int>"));
        assert!(prompt.contains("Return `packages` as an ARRAY of package objects"));
        assert!(!prompt.contains("\"packages\": {{"));
        assert!(!prompt.contains("integers (as strings in JSON keys)"));
    }

    #[test]
    fn pinout_cache_scope_version_changed_for_prompt_contract_fix() {
        let scope = verification_cache_scope(
            Provider::OpenAI,
            VerifyTask::Pinout,
            &json!({
                "part_number": "DSPIC33AK256MPS205",
                "pin_count": 48
            }),
        );

        assert!(scope.contains("schema=pinout-extraction-v3"));
    }
}
