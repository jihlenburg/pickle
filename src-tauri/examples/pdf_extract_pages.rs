use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use lopdf::{Dictionary, Document, Object, ObjectId};

const INHERITABLE_PAGE_KEYS: [&[u8]; 7] = [
    b"Resources",
    b"MediaBox",
    b"CropBox",
    b"Rotate",
    b"BleedBox",
    b"TrimBox",
    b"ArtBox",
];

const CATALOG_KEYS_TO_DROP: [&[u8]; 7] = [
    b"Outlines",
    b"Names",
    b"OpenAction",
    b"StructTreeRoot",
    b"PageLabels",
    b"Metadata",
    b"AcroForm",
];

fn usage(program: &str) -> String {
    format!(
        "Usage: {program} <input.pdf> <start-page> <end-page> [output.pdf]\n\
         Example: {program} datasheet.pdf 9 20 datasheet-pages-9-20.pdf"
    )
}

fn default_output_path(input: &Path, start: u32, end: u32) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("output");
    let file_name = format!("{stem}-pages-{start}-{end}.pdf");
    input.with_file_name(file_name)
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

fn collect_inherited_page_attributes(
    document: &Document,
    page_id: ObjectId,
) -> Result<BTreeMap<Vec<u8>, Object>, String> {
    let mut needed_keys: BTreeSet<Vec<u8>> = INHERITABLE_PAGE_KEYS
        .iter()
        .map(|key| key.to_vec())
        .collect();
    let mut inherited = BTreeMap::new();

    {
        let page = document
            .get_dictionary(page_id)
            .map_err(|error| format!("Failed to inspect page {page_id:?}: {error}"))?;
        for key in INHERITABLE_PAGE_KEYS {
            if page.has(key) {
                needed_keys.remove(key);
            }
        }
    }

    let mut current_parent = document
        .get_dictionary(page_id)
        .map_err(|error| format!("Failed to inspect page {page_id:?}: {error}"))?
        .get(b"Parent")
        .and_then(Object::as_reference)
        .ok();

    while let Some(parent_id) = current_parent {
        let parent = document
            .get_dictionary(parent_id)
            .map_err(|error| format!("Failed to inspect page tree {parent_id:?}: {error}"))?;

        let remaining: Vec<Vec<u8>> = needed_keys.iter().cloned().collect();
        for key in remaining {
            if let Ok(value) = parent.get(&key) {
                inherited.insert(key.clone(), value.clone());
                needed_keys.remove(&key);
            }
        }

        if needed_keys.is_empty() {
            break;
        }

        current_parent = parent.get(b"Parent").and_then(Object::as_reference).ok();
    }

    Ok(inherited)
}

fn build_trimmed_pages_root(
    original_root: &Dictionary,
    kids: Vec<Object>,
    count: u32,
) -> Dictionary {
    let mut root = original_root.clone();
    root.set("Type", "Pages");
    root.set("Kids", kids);
    root.set("Count", count as i64);
    root.remove(b"Parent");
    root
}

fn trim_document_to_page_range(
    document: &mut Document,
    start: u32,
    end: u32,
) -> Result<u32, String> {
    let pages = document.get_pages();
    let total_pages = pages.len() as u32;

    if total_pages == 0 {
        return Err("Document contains no pages".to_string());
    }
    if end > total_pages {
        return Err(format!(
            "Requested end-page {end} exceeds total pages {total_pages}"
        ));
    }

    let selected_page_ids: Vec<ObjectId> = (start..=end)
        .map(|page_number| {
            pages.get(&page_number).copied().ok_or_else(|| {
                format!("Could not resolve page {page_number} in document page tree")
            })
        })
        .collect::<Result<_, _>>()?;

    let inherited_attrs: Vec<(ObjectId, BTreeMap<Vec<u8>, Object>)> = selected_page_ids
        .iter()
        .copied()
        .map(|page_id| {
            collect_inherited_page_attributes(document, page_id).map(|attrs| (page_id, attrs))
        })
        .collect::<Result<_, _>>()?;

    let pages_root_id = document
        .catalog()
        .and_then(|catalog| catalog.get(b"Pages"))
        .and_then(Object::as_reference)
        .map_err(|error| format!("Failed to resolve document Pages root: {error}"))?;

    let original_root = document
        .get_dictionary(pages_root_id)
        .map_err(|error| format!("Failed to read page tree root: {error}"))?
        .clone();

    let new_root_id = document.new_object_id();
    let kids: Vec<Object> = selected_page_ids
        .iter()
        .copied()
        .map(Object::Reference)
        .collect();
    let new_root = build_trimmed_pages_root(&original_root, kids, selected_page_ids.len() as u32);
    document
        .objects
        .insert(new_root_id, Object::Dictionary(new_root));

    for (page_id, attrs) in inherited_attrs {
        let page = document
            .get_dictionary_mut(page_id)
            .map_err(|error| format!("Failed to update page {page_id:?}: {error}"))?;
        for (key, value) in attrs {
            if !page.has(&key) {
                page.set(key, value);
            }
        }
        page.set("Parent", Object::Reference(new_root_id));
    }

    {
        let catalog = document
            .catalog_mut()
            .map_err(|error| format!("Failed to update document catalog: {error}"))?;
        catalog.set("Pages", Object::Reference(new_root_id));
        for key in CATALOG_KEYS_TO_DROP {
            catalog.remove(key);
        }
    }

    document.prune_objects();

    Ok(total_pages)
}

fn main() -> Result<(), String> {
    let args: Vec<OsString> = env::args_os().collect();
    let program = args
        .first()
        .and_then(|arg| arg.to_str())
        .unwrap_or("pdf_extract_pages");

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

    let mut document = Document::load(&input_path)
        .map_err(|error| format!("Failed to load PDF {}: {error}", input_path.display()))?;
    let total_pages = trim_document_to_page_range(&mut document, start, end)?;

    document.save(&output_path).map_err(|error| {
        format!(
            "Failed to save extracted PDF {}: {error}",
            output_path.display()
        )
    })?;

    let extracted = Document::load(&output_path).map_err(|error| {
        format!(
            "Extracted PDF {} could not be re-opened for verification: {error}",
            output_path.display()
        )
    })?;
    let extracted_pages = extracted.get_pages().len() as u32;

    println!(
        "Extracted pages {start}-{end} of {total_pages} from {} into {} ({extracted_pages} pages)",
        input_path.display(),
        output_path.display()
    );

    Ok(())
}
