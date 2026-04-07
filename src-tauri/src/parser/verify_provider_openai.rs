//! OpenAI-specific verification transport.
//!
//! This module owns OpenAI file uploads, Responses API streaming requests, and
//! PDF-to-image fallback policy so the generic provider dispatcher stays small.

use serde_json::Value;
use std::time::Instant;

use crate::parser::verify_openai_stream::read_openai_stream;
use crate::parser::verify_pdf::{
    describe_page_spans, reduce_pdf_with_bookmarks, relevant_page_spans_for_pdf,
    render_pages_to_pngs,
};
use crate::parser::verify_progress::{emit_progress, ProgressCallback, VerifyProgressUpdate};
use crate::parser::verify_prompt::{
    openai_image_instructions, openai_instructions, provider_analysis_hint, provider_name,
    task_label, task_reduce_progress_detail, task_reduce_progress_label, task_sections_label,
    Provider, VerifyTask,
};
use crate::parser::verify_provider_schema::openai_verification_text_format;

const OPENAI_API_URL: &str = "https://api.openai.com/v1/responses";
const OPENAI_FILES_API_URL: &str = "https://api.openai.com/v1/files";
const OPENAI_MODEL: &str = "gpt-5.4";
const OPENAI_REASONING_EFFORT: &str = "high";
const OPENAI_MAX_OUTPUT_TOKENS: u32 = 65_536;
const OPENAI_FILE_LIMIT_BYTES: usize = 50 * 1024 * 1024;
const PNG_IMAGE_MEDIA_TYPE: &str = "image/png";

fn get_optional_env_value(var_name: &str) -> Option<String> {
    std::env::var(var_name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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

fn openai_client(timeout_secs: u64) -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(|error| format!("HTTP client error: {error}"))
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
        .map_err(|error| format!("API request error: {error}"))?;
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

fn should_fallback_openai_to_images(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("context_length_exceeded")
        || lower.contains("input_file")
        || lower.contains("application/pdf")
        || lower.contains("unsupported")
        || lower.contains("invalid file")
        || lower.contains("no text in openai response")
}

fn call_openai_image_api(
    pdf_bytes: &[u8],
    page_spans: &[crate::parser::verify_pdf::PageSpan],
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
        .detail("Retrying with page images instead of PDF upload.")
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
            format!("Uploading rendered {} to OpenAI", task_sections_label(task)),
        )
        .detail(format!(
            "Retrying with {} PNG page(s) from datasheet pages: {}.",
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

pub(crate) fn call_openai_api(
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
            "Verification requires a datasheet PDF so pickle can send selected pages or rendered page images.{detail}"
        ));
    }

    emit_progress(
        progress,
        VerifyProgressUpdate::new("datasheet.reduce", 0.44, task_reduce_progress_label(task))
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
                "Uploading {} to {}",
                task_sections_label(task),
                provider_name(Provider::OpenAI)
            ),
        )
        .detail(format!(
            "Selected {} of {} pages ({:.1} MB → {:.1} MB, {})",
            reduced_pdf.selected_pages(),
            source_pages,
            pdf_bytes.len() as f64 / 1_048_576.0,
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
        "{prompt}\n\nThe attached PDF contains only the relevant datasheet sections. Included page ranges: {}.",
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
            format!("OpenAI is analyzing the {}", task_sections_label(task)),
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
