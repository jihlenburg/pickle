use std::env;
use std::time::Instant;

use pickle_lib::parser::datasheet_fetcher;

fn main() -> Result<(), String> {
    let part_number = env::args()
        .nth(1)
        .ok_or_else(|| "usage: cargo run --example resolve_datasheet -- <part-number>".to_string())?;

    let start = Instant::now();
    let resolved = datasheet_fetcher::resolve(&part_number)?;
    println!("part_number={}", resolved.part_number);
    println!("datasheet_number={}", resolved.datasheet_number);
    println!("datasheet_revision={}", resolved.datasheet_revision);
    println!("pdf_url={}", resolved.pdf_url);
    println!("elapsed_ms={}", start.elapsed().as_millis());
    Ok(())
}
