//! OpenAI Responses API streaming normalization helpers.
//!
//! The verifier receives SSE fragments from the Responses API and needs to
//! normalize them into the same JSON object shape used by the local comparison
//! pass. Keeping that logic here avoids mixing provider transport with JSON
//! repair and stream assembly details.

use serde_json::Value;
use std::io::{BufRead, BufReader};

use crate::parser::verify_provider_schema::normalize_structured_verification_output;

#[derive(Debug, Default)]
struct OpenAiStreamState {
    output_text: String,
    refusal_text: String,
    final_response: Option<Value>,
    last_error: Option<String>,
    event_count: usize,
    terminal_event_seen: bool,
}

fn normalize_openai_output_value(value: &Value) -> Value {
    if value
        .get("packages")
        .map(|packages| packages.is_array())
        .unwrap_or(false)
    {
        normalize_structured_verification_output(value)
    } else if let Some(array) = value.as_array() {
        let packages = array
            .iter()
            .filter_map(|item| {
                let package_name = item.get("package_name").and_then(|v| v.as_str())?;
                Some((package_name.to_string(), item.clone()))
            })
            .collect::<serde_json::Map<String, Value>>();

        serde_json::json!({
            "packages": packages,
            "corrections": [],
            "clc_input_sources": [],
            "notes": [],
        })
    } else {
        value.clone()
    }
}

pub(crate) fn normalize_openai_output_text(text: &str) -> Result<String, String> {
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

pub(crate) fn extract_openai_text(result: &Value) -> Result<String, String> {
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

pub(crate) fn parse_openai_stream_reader<R: BufRead>(mut reader: R) -> Result<String, String> {
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

pub(crate) fn read_openai_stream(resp: reqwest::blocking::Response) -> Result<String, String> {
    parse_openai_stream_reader(BufReader::new(resp))
}
