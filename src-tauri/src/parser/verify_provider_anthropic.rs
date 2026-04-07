//! Anthropic-specific verification transport.
//!
//! This module owns Anthropic file uploads, tool-call payloads, and PNG/PDF
//! fallback handling so the generic provider dispatcher does not need to know
//! Anthropic API details.

use serde_json::Value;
use std::time::Instant;

use crate::parser::verify_pdf::{
    describe_page_spans, reduce_pdf_with_bookmarks, relevant_page_spans_for_pdf,
    render_pages_to_pngs, PageSpan,
};
use crate::parser::verify_progress::{emit_progress, ProgressCallback, VerifyProgressUpdate};
use crate::parser::verify_prompt::{
    provider_analysis_hint, provider_name, task_label, task_reduce_progress_detail,
    task_reduce_progress_label, task_sections_label, Provider, VerifyTask,
};
use crate::parser::verify_provider_schema::{
    anthropic_verification_tool, normalize_structured_verification_output,
};

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_FILES_API_URL: &str = "https://api.anthropic.com/v1/files";
const ANTHROPIC_API_VERSION: &str = "2023-06-01";
const ANTHROPIC_FILES_API_BETA: &str = "files-api-2025-04-14";
const ANTHROPIC_MODEL_BYTES: &[u8] = &[
    99, 108, 97, 117, 100, 101, 45, 115, 111, 110, 110, 101, 116, 45, 52, 45, 54,
];
const MAX_TOKENS: u32 = 16384;
const PNG_IMAGE_MEDIA_TYPE: &str = "image/png";

fn anthropic_model() -> &'static str {
    std::str::from_utf8(ANTHROPIC_MODEL_BYTES).expect("valid Anthropic model identifier")
}

fn anthropic_client(timeout_secs: u64) -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(|error| format!("HTTP client error: {error}"))
}

fn prepare_pdf(pdf_bytes: &[u8]) -> Vec<u8> {
    pdf_bytes.to_vec()
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

fn extract_anthropic_response_text(result: &Value) -> String {
    let mut text_parts: Vec<String> = Vec::new();
    if let Some(content) = result.get("content").and_then(|value| value.as_array()) {
        for block in content {
            if block.get("type").and_then(|value| value.as_str()) == Some("text") {
                if let Some(text) = block.get("text").and_then(|value| value.as_str()) {
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
            if block.get("name").and_then(|value| value.as_str()) != Some("submit_verification") {
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

fn call_anthropic_reduced_pdf_api(
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
                "Uploading {} to {}",
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
            format!("Anthropic is analyzing the {}", task_sections_label(task)),
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
            "name": "submit_verification"
        },
        "messages": [{
            "role": "user",
            "content": [
                {
                    "type": "text",
                    "text": format!(
                        "{prompt}\n\nThe attached PDF contains only the relevant datasheet sections. Included page ranges: {}.",
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
        .detail("Retrying with page images instead of PDF upload.")
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
            "Retrying with {} PNG page(s) from datasheet pages: {}.",
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
            "name": "submit_verification"
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

pub(crate) fn call_anthropic_api(
    pdf_bytes: &[u8],
    task: VerifyTask,
    prompt: &str,
    api_key: &str,
    part_number: &str,
    progress: Option<&ProgressCallback>,
) -> Result<String, String> {
    emit_progress(
        progress,
        VerifyProgressUpdate::new("datasheet.reduce", 0.44, task_reduce_progress_label(task))
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
            match call_anthropic_reduced_pdf_api(
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
            call_anthropic_image_api(pdf_bytes, &page_spans, task, prompt, api_key, progress)
        }
    }
}
