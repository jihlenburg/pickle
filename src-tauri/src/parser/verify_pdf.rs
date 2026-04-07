//! Datasheet bookmark scanning, page-span selection, reduction, and rendering.
//!
//! The verifier narrows family datasheets to the relevant pinout or CLC pages
//! before provider upload. Keeping PDF analysis here isolates PDFium/qpdf work
//! from provider transport and cache/orchestration code.

use image::ImageFormat;
use pdfium_auto::bind_pdfium_silent;
use pdfium_render::prelude::PdfRenderConfig;
use qpdf::{ObjectStreamMode, QPdf, StreamDataMode};
use std::fs;
use std::io::Cursor;
use tempfile::NamedTempFile;

use crate::parser::verify_prompt::VerifyTask;

const RENDER_DPI: f32 = 300.0;

#[derive(Debug, Clone)]
pub(crate) struct BookmarkEntry {
    pub(crate) title: String,
    pub(crate) page: u32,
    pub(crate) depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageSpan {
    pub(crate) start: u32,
    pub(crate) end: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct ReducedPdf {
    pub(crate) bytes: Vec<u8>,
    pub(crate) page_spans: Vec<PageSpan>,
}

impl ReducedPdf {
    pub(crate) fn selected_pages(&self) -> u32 {
        self.page_spans
            .iter()
            .map(|span| span.end.saturating_sub(span.start) + 1)
            .sum()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RenderedPageImage {
    pub(crate) page_number: u32,
    pub(crate) bytes: Vec<u8>,
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

fn text_matches_pinout_section(text: &str) -> bool {
    title_matches_pin_diagrams(text) || title_matches_pinout_descriptions(text)
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

pub(crate) fn select_pinout_page_spans(
    bookmarks: &[BookmarkEntry],
    total_pages: u32,
) -> Vec<PageSpan> {
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

pub(crate) fn select_clc_page_spans(
    bookmarks: &[BookmarkEntry],
    total_pages: u32,
) -> Vec<PageSpan> {
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

pub(crate) fn select_clc_page_spans_from_text_hits(
    hits: &[u32],
    total_pages: u32,
) -> Vec<PageSpan> {
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

pub(crate) fn select_pinout_page_spans_from_text_hits(
    section_hits: &[u32],
    toc_hits: &[u32],
    total_pages: u32,
) -> Vec<PageSpan> {
    if section_hits.is_empty() {
        return Vec::new();
    }

    let start_hit = section_hits[0];
    let mut last_relevant_hit = section_hits[0];

    for &page in section_hits.iter().skip(1) {
        if page <= start_hit.saturating_add(40) {
            last_relevant_hit = page;
        } else {
            break;
        }
    }

    let mut end_hit = (last_relevant_hit + 12).min(total_pages);

    if let Some(toc_page) = toc_hits.iter().copied().find(|&page| page > start_hit) {
        end_hit = end_hit.min(toc_page.saturating_sub(1));
    }

    if end_hit < start_hit {
        return Vec::new();
    }

    vec![PageSpan {
        start: start_hit,
        end: end_hit,
    }]
}

fn task_page_spans(
    bookmarks: &[BookmarkEntry],
    total_pages: u32,
    task: VerifyTask,
) -> Vec<PageSpan> {
    match task {
        VerifyTask::Pinout => select_pinout_page_spans(bookmarks, total_pages),
        VerifyTask::Clc => select_clc_page_spans(bookmarks, total_pages),
    }
}

fn bookmark_page_number(bookmark: &pdfium_render::prelude::PdfBookmark<'_>) -> Option<u32> {
    // Some Microchip datasheets store outline navigation as a local GoTo action
    // instead of a direct /Dest entry, so accept both encodings when resolving
    // the bookmark target page.
    bookmark
        .action()
        .and_then(|action| {
            action.as_local_destination_action().and_then(|local| {
                local
                    .destination()
                    .ok()
                    .and_then(|destination| destination.page_index().ok())
            })
        })
        .or_else(|| {
            bookmark
                .destination()
                .and_then(|destination| destination.page_index().ok())
        })
        .map(|page_index| page_index as u32 + 1)
}

pub(crate) fn collect_pdf_bookmarks(pdf_bytes: &[u8]) -> Result<(Vec<BookmarkEntry>, u32), String> {
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
        let Some(page) = bookmark_page_number(&bookmark) else {
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

fn describe_bookmark_sample(bookmarks: &[BookmarkEntry]) -> String {
    if bookmarks.is_empty() {
        return "none".to_string();
    }

    bookmarks
        .iter()
        .take(8)
        .map(|bookmark| format!("{}@{}#{}", bookmark.title, bookmark.page, bookmark.depth))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn find_pinout_page_spans_from_text(pdf_bytes: &[u8]) -> Result<(Vec<PageSpan>, u32), String> {
    let temp_pdf = NamedTempFile::new().map_err(|error| format!("Tempfile error: {error}"))?;
    fs::write(temp_pdf.path(), pdf_bytes)
        .map_err(|error| format!("Failed to write temporary PDF: {error}"))?;

    let pdfium = bind_pdfium_silent().map_err(|error| format!("Failed to bind PDFium: {error}"))?;
    let document = pdfium
        .load_pdf_from_file(temp_pdf.path(), None)
        .map_err(|error| format!("Failed to open PDF for pinout text scan: {error}"))?;

    let total_pages = document.pages().len() as u32;
    let mut section_hits = Vec::new();
    let mut toc_hits = Vec::new();

    for page_number in 1..=total_pages {
        let page = document
            .pages()
            .get((page_number - 1) as u16)
            .map_err(|error| format!("Failed to read PDF page {page_number}: {error}"))?;
        let page_text = page
            .text()
            .map_err(|error| {
                format!("Failed to extract text from PDF page {page_number}: {error}")
            })?
            .all();

        let is_toc = title_matches_table_of_contents(&page_text);
        if is_toc {
            toc_hits.push(page_number);
        }

        if !is_toc && text_matches_pinout_section(&page_text) {
            section_hits.push(page_number);
        }
    }

    let spans = select_pinout_page_spans_from_text_hits(&section_hits, &toc_hits, total_pages);
    Ok((spans, total_pages))
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
            .map_err(|error| {
                format!("Failed to extract text from PDF page {page_number}: {error}")
            })?
            .all();

        if text_matches_clc_section(&page_text) {
            hits.push(page_number);
        }
    }

    let spans = select_clc_page_spans_from_text_hits(&hits, total_pages);
    Ok((spans, total_pages))
}

pub(crate) fn describe_page_spans(page_spans: &[PageSpan]) -> String {
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

pub(crate) fn relevant_page_spans_for_pdf(
    pdf_bytes: &[u8],
    task: VerifyTask,
) -> Result<(Vec<PageSpan>, u32), String> {
    let (bookmarks, total_pages) = collect_pdf_bookmarks(pdf_bytes)?;
    let page_spans = task_page_spans(&bookmarks, total_pages, task);
    if page_spans.is_empty() {
        log::warn!(
            "verify_pinout: no {} bookmark ranges found in PDF with {} pages; bookmarks_collected={} sample={}",
            match task {
                VerifyTask::Pinout => "pinout",
                VerifyTask::Clc => "CLC",
            },
            total_pages,
            bookmarks.len(),
            describe_bookmark_sample(&bookmarks)
        );

        match task {
            VerifyTask::Pinout => {
                let (fallback_spans, total_pages) = find_pinout_page_spans_from_text(pdf_bytes)?;
                if !fallback_spans.is_empty() {
                    log::info!(
                        "verify_pinout: using PDF text fallback to locate pinout pages ({})",
                        describe_page_spans(&fallback_spans)
                    );
                    return Ok((fallback_spans, total_pages));
                }
            }
            VerifyTask::Clc => {
                let (fallback_spans, total_pages) = find_clc_page_spans_from_text(pdf_bytes)?;
                if !fallback_spans.is_empty() {
                    log::info!(
                        "verify_pinout: using PDF text fallback to locate CLC pages ({})",
                        describe_page_spans(&fallback_spans)
                    );
                    return Ok((fallback_spans, total_pages));
                }
            }
        }

        return Err(match task {
            VerifyTask::Pinout => {
                "No bookmark or text ranges found for pinout sections".to_string()
            }
            VerifyTask::Clc => "No bookmark or text ranges found for the CLC section".to_string(),
        });
    }

    Ok((page_spans, total_pages))
}

pub(crate) fn reduce_pdf_with_bookmarks(
    pdf_bytes: &[u8],
    task: VerifyTask,
) -> Result<ReducedPdf, String> {
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

pub(crate) fn render_pages_to_pngs(
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
