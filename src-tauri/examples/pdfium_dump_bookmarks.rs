use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

use pdfium_auto::bind_pdfium_silent;

fn usage(program: &str) -> String {
    format!(
        "Usage: {program} <input.pdf> [bookmark title]\n\
         Example: {program} datasheet.pdf \"Pin Diagrams\""
    )
}

fn main() -> Result<(), String> {
    let args: Vec<OsString> = env::args_os().collect();
    let program = args
        .first()
        .and_then(|arg| arg.to_str())
        .unwrap_or("pdfium_dump_bookmarks");

    if args.len() < 2 || args.len() > 3 {
        return Err(usage(program));
    }

    let input_path = PathBuf::from(&args[1]);
    let title_filter = args.get(2).and_then(|arg| arg.to_str());

    let pdfium = bind_pdfium_silent().map_err(|error| format!("Failed to bind PDFium: {error}"))?;
    let document = pdfium
        .load_pdf_from_file(&input_path, None)
        .map_err(|error| format!("Failed to open {}: {error}", input_path.display()))?;

    let mut matches = 0usize;

    for bookmark in document.bookmarks().iter() {
        let Some(title) = bookmark.title() else {
            continue;
        };

        if let Some(filter) = title_filter {
            if title != filter {
                continue;
            }
        }

        let page_display = bookmark
            .destination()
            .and_then(|destination| destination.page_index().ok())
            .map(|page_index| (page_index + 1).to_string())
            .unwrap_or_else(|| "?".to_string());

        let mut depth = 0usize;
        let mut current = bookmark.parent();
        while let Some(parent) = current {
            depth += 1;
            current = parent.parent();
        }
        let indent = "  ".repeat(depth);

        println!("{indent}{title} -> page {page_display}");
        matches += 1;
    }

    if matches == 0 {
        if let Some(filter) = title_filter {
            return Err(format!("No bookmark titled {filter:?} found"));
        }
        return Err("Document has no readable bookmarks".to_string());
    }
    Ok(())
}
