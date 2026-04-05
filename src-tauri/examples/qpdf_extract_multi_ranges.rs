use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Instant;

use qpdf::{ObjectStreamMode, QPdf, StreamDataMode};

fn usage(program: &str) -> String {
    format!(
        "Usage: {program} <input.pdf> <output.pdf> <start-end> [<start-end> ...]\n\
         Example: {program} datasheet.pdf reduced.pdf 5-23 28-30"
    )
}

fn parse_range(value: &str) -> Result<(u32, u32), Box<dyn Error>> {
    let (start, end) = value
        .split_once('-')
        .ok_or_else(|| format!("Invalid range {value:?}, expected start-end"))?;
    let start = start
        .parse::<u32>()
        .map_err(|error| format!("Invalid range start {start:?}: {error}"))?;
    let end = end
        .parse::<u32>()
        .map_err(|error| format!("Invalid range end {end:?}: {error}"))?;
    if start == 0 || end == 0 {
        return Err("Page numbers must be >= 1".into());
    }
    if start > end {
        return Err(format!("Range start {start} must be <= end {end}").into());
    }
    Ok((start, end))
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<OsString> = env::args_os().collect();
    let program = args
        .first()
        .and_then(|arg| arg.to_str())
        .unwrap_or("qpdf_extract_multi_ranges");

    if args.len() < 4 {
        eprintln!("{}", usage(program));
        std::process::exit(2);
    }

    let input_path = PathBuf::from(&args[1]);
    let output_path = PathBuf::from(&args[2]);
    let ranges: Vec<(u32, u32)> = args[3..]
        .iter()
        .map(|arg| parse_range(arg.to_str().unwrap_or_default()))
        .collect::<Result<_, _>>()?;

    let started = Instant::now();
    let source = QPdf::read(&input_path)?;
    let pages = source.get_pages()?;
    let total_pages = pages.len() as u32;
    let sink = QPdf::empty();

    let mut selected_pages = 0u32;
    for (start, end) in &ranges {
        if *end > total_pages {
            return Err(format!(
                "Requested range {}-{} exceeds total pages {}",
                start, end, total_pages
            )
            .into());
        }
        for page_number in *start..=*end {
            sink.add_page(&pages[(page_number - 1) as usize], false)?;
            selected_pages += 1;
        }
    }

    sink.writer()
        .preserve_unreferenced_objects(false)
        .object_stream_mode(ObjectStreamMode::Preserve)
        .stream_data_mode(StreamDataMode::Preserve)
        .compress_streams(true)
        .write(&output_path)?;

    println!(
        "qpdf extracted {} pages from {} into {} in {:.2?}",
        selected_pages,
        input_path.display(),
        output_path.display(),
        started.elapsed()
    );

    Ok(())
}
