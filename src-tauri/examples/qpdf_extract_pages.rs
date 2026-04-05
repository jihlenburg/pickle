use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Instant;

use qpdf::{ObjectStreamMode, QPdf, StreamDataMode};

fn usage(program: &str) -> String {
    format!(
        "Usage: {program} <input.pdf> <start-page> <end-page> [output.pdf]\n\
         Example: {program} datasheet.pdf 9 20 datasheet-pages-9-20-qpdf.pdf"
    )
}

fn default_output_path(input: &Path, start: u32, end: u32) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("output");
    input.with_file_name(format!("{stem}-pages-{start}-{end}-qpdf.pdf"))
}

fn parse_page(value: &str, label: &str) -> Result<u32, Box<dyn Error>> {
    let page = value
        .parse::<u32>()
        .map_err(|error| format!("Invalid {label}: {value} ({error})"))?;
    if page == 0 {
        return Err(format!("{label} must be >= 1").into());
    }
    Ok(page)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<OsString> = env::args_os().collect();
    let program = args
        .first()
        .and_then(|arg| arg.to_str())
        .unwrap_or("qpdf_extract_pages");

    if args.len() < 4 || args.len() > 5 {
        eprintln!("{}", usage(program));
        std::process::exit(2);
    }

    let input_path = PathBuf::from(&args[1]);
    let start = parse_page(args[2].to_str().unwrap_or_default(), "start-page")?;
    let end = parse_page(args[3].to_str().unwrap_or_default(), "end-page")?;
    if start > end {
        return Err(format!("start-page ({start}) must be <= end-page ({end})").into());
    }

    let output_path = if let Some(path) = args.get(4) {
        PathBuf::from(path)
    } else {
        default_output_path(&input_path, start, end)
    };

    let started = Instant::now();
    let source = QPdf::read(&input_path)?;
    let total_pages = source.get_num_pages()?;
    if total_pages == 0 {
        return Err(format!("{} contains no pages", input_path.display()).into());
    }
    if end > total_pages {
        return Err(format!("Requested end-page {end} exceeds total pages {total_pages}").into());
    }

    let sink = QPdf::empty();
    let pages = source.get_pages()?;
    for page_index in (start - 1)..=(end - 1) {
        sink.add_page(&pages[page_index as usize], false)?;
    }

    sink.writer()
        .preserve_unreferenced_objects(false)
        .object_stream_mode(ObjectStreamMode::Preserve)
        .stream_data_mode(StreamDataMode::Preserve)
        .compress_streams(true)
        .write(&output_path)?;

    println!(
        "qpdf extracted pages {start}-{end} of {total_pages} into {} in {:.2?}",
        output_path.display(),
        started.elapsed()
    );

    Ok(())
}
