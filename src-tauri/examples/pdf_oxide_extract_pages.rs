use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Instant;

use pdf_oxide::editor::{DocumentEditor, EditableDocument};
use pdf_oxide::PdfDocument;

fn usage(program: &str) -> String {
    format!(
        "Usage: {program} <input.pdf> <start-page> <end-page> [output.pdf]\n\
         Example: {program} datasheet.pdf 9 20 datasheet-pages-9-20-pdf-oxide.pdf"
    )
}

fn default_output_path(input: &Path, start: u32, end: u32) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("output");
    input.with_file_name(format!("{stem}-pages-{start}-{end}-pdf-oxide.pdf"))
}

fn parse_page(value: &str, label: &str) -> Result<u32, String> {
    let page = value
        .parse::<u32>()
        .map_err(|_| format!("Invalid {label}: {value}"))?;
    if page == 0 {
        return Err(format!("{label} must be >= 1"));
    }
    Ok(page)
}

fn main() -> Result<(), String> {
    let args: Vec<OsString> = env::args_os().collect();
    let program = args
        .first()
        .and_then(|arg| arg.to_str())
        .unwrap_or("pdf_oxide_extract_pages");

    if args.len() < 4 || args.len() > 5 {
        return Err(usage(program));
    }

    let input_path = PathBuf::from(&args[1]);
    let start = parse_page(
        args[2]
            .to_str()
            .ok_or_else(|| "start-page must be valid UTF-8".to_string())?,
        "start-page",
    )?;
    let end = parse_page(
        args[3]
            .to_str()
            .ok_or_else(|| "end-page must be valid UTF-8".to_string())?,
        "end-page",
    )?;
    if start > end {
        return Err(format!("start-page ({start}) must be <= end-page ({end})"));
    }

    let output_path = if let Some(path) = args.get(4) {
        PathBuf::from(path)
    } else {
        default_output_path(&input_path, start, end)
    };

    let started = Instant::now();
    let mut editor = DocumentEditor::open(&input_path)
        .map_err(|error| format!("Failed to open PDF {}: {error}", input_path.display()))?;

    let total_pages = editor.current_page_count();
    if total_pages == 0 {
        return Err(format!("{} contains no pages", input_path.display()));
    }
    if end as usize > total_pages {
        return Err(format!(
            "Requested end-page {end} exceeds total pages {total_pages}"
        ));
    }

    let start_index = (start - 1) as usize;
    let end_index = (end - 1) as usize;

    for page_index in (0..total_pages).rev() {
        if page_index < start_index || page_index > end_index {
            editor
                .remove_page(page_index)
                .map_err(|error| format!("Failed removing page {}: {error}", page_index + 1))?;
        }
    }

    editor
        .save(&output_path)
        .map_err(|error| format!("Failed to save {}: {error}", output_path.display()))?;

    let mut extracted = PdfDocument::open(&output_path)
        .map_err(|error| format!("Failed to re-open {}: {error}", output_path.display()))?;
    let extracted_pages = extracted.page_count().map_err(|error| {
        format!(
            "Failed to count pages in {}: {error}",
            output_path.display()
        )
    })?;

    println!(
        "pdf_oxide extracted pages {start}-{end} of {total_pages} into {} ({extracted_pages} pages) in {:.2?}",
        output_path.display(),
        started.elapsed()
    );

    Ok(())
}
