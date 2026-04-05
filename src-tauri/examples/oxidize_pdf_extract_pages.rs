use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Instant;

use oxidize_pdf::operations::{extract_page_range_to_file, PageRange};

fn usage(program: &str) -> String {
    format!(
        "Usage: {program} <input.pdf> <start-page> <end-page> [output.pdf]\n\
         Example: {program} datasheet.pdf 9 20 datasheet-pages-9-20-oxidize-pdf.pdf"
    )
}

fn default_output_path(input: &Path, start: u32, end: u32) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("output");
    input.with_file_name(format!("{stem}-pages-{start}-{end}-oxidize-pdf.pdf"))
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
        .unwrap_or("oxidize_pdf_extract_pages");

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

    let range = PageRange::Range((start - 1) as usize, (end - 1) as usize);
    let started = Instant::now();
    extract_page_range_to_file(&input_path, &range, &output_path)
        .map_err(|error| format!("oxidize-pdf extraction failed: {error}"))?;

    println!(
        "oxidize-pdf extracted pages {start}-{end} into {} in {:.2?}",
        output_path.display(),
        started.elapsed()
    );

    Ok(())
}
