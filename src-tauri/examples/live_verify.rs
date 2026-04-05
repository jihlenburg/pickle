use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use log::{Level, LevelFilter, Log, Metadata, Record};
use serde_json::{json, Value};

use pickle_lib::parser::dfp_manager;
use pickle_lib::parser::pinout_verifier;

struct StderrLogger;

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("log={} {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

static LOGGER: StderrLogger = StderrLogger;

fn device_packages(device: &pickle_lib::parser::edc_parser::DeviceData) -> HashMap<String, Value> {
    device
        .pinouts
        .iter()
        .map(|(name, pinout)| {
            (
                name.clone(),
                json!({
                    "pin_count": pinout.pin_count,
                    "source": pinout.source,
                }),
            )
        })
        .collect()
}

fn read_key_from_dotenv(var_name: &str) -> Result<String, String> {
    let candidate_paths = [
        PathBuf::from(".env"),
        PathBuf::from("..").join(".env"),
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("pickle")
            .join(".env"),
    ];

    for path in candidate_paths {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim();
            if let Some(value) = line.strip_prefix(&format!("{var_name}=")) {
                let key = value.trim().to_string();
                if !key.is_empty() {
                    return Ok(key);
                }
            }
        }
    }

    Err(format!("could not find {var_name} in .env"))
}

fn main() -> Result<(), String> {
    log::set_logger(&LOGGER).map_err(|error| format!("logger init failed: {error}"))?;
    log::set_max_level(LevelFilter::Info);

    let mut args: Vec<String> = env::args().skip(1).collect();
    let mut api_key: Option<String> = None;
    if let Some(arg) = args.first().cloned() {
        if let Some(provider) = arg.strip_prefix("--provider=") {
            let var_name = match provider {
                "anthropic" => "ANTHROPIC_API_KEY",
                "openai" => "OPENAI_API_KEY",
                other => return Err(format!("unsupported provider override: {other}")),
            };
            api_key = Some(read_key_from_dotenv(var_name)?);
            args.remove(0);
        }
    }
    let mut args = args.into_iter();
    let pdf_path = args
        .next()
        .ok_or_else(|| "usage: cargo run --example live_verify -- <pdf-path> <part-number> [package]".to_string())?;
    let part_number = args
        .next()
        .ok_or_else(|| "missing part number".to_string())?;
    let package = args.next();

    eprintln!("stage=read_pdf path={pdf_path}");
    let pdf_bytes =
        fs::read(&pdf_path).map_err(|error| format!("failed to read {pdf_path}: {error}"))?;
    eprintln!("stage=read_pdf_done bytes={}", pdf_bytes.len());

    eprintln!("stage=load_device part_number={part_number}");
    let device = dfp_manager::load_device(&part_number)
        .ok_or_else(|| format!("device {part_number} not found"))?;
    eprintln!("stage=load_device_done default_package={}", device.default_pinout);

    let package_name = package.as_deref().unwrap_or(&device.default_pinout);
    let resolved_pins = device.resolve_pins(Some(package_name));
    let pinout = device.get_pinout(Some(package_name));

    let device_dict = json!({
        "part_number": device.part_number,
        "selected_package": package_name,
        "packages": device_packages(&device),
        "pin_count": pinout.pin_count,
        "pins": resolved_pins,
    });

    let start = Instant::now();
    eprintln!("stage=verify_start package={package_name}");
    let progress = |update: pinout_verifier::VerifyProgressUpdate| {
        eprintln!(
            "progress stage={} progress={:.2} label={}{}",
            update.stage,
            update.progress,
            update.label,
            update
                .detail
                .as_ref()
                .map(|detail| format!(" detail={detail}"))
                .unwrap_or_default()
        );
    };
    let result = pinout_verifier::verify_pinout(
        &pdf_bytes,
        None,
        &device_dict,
        api_key.as_deref(),
        Some(&progress),
    )?;
    eprintln!("stage=verify_done elapsed_ms={}", start.elapsed().as_millis());

    println!("part_number={}", result.part_number);
    println!("package_count={}", result.packages.len());
    println!("packages={}", {
        let mut names: Vec<_> = result.packages.keys().cloned().collect();
        names.sort();
        names.join(",")
    });
    println!("notes_count={}", result.notes.len());
    for note in &result.notes {
        println!("note={note}");
    }
    match &result.clc_input_sources {
        Some(groups) => {
            println!("clc_groups={}", groups.len());
            for (index, group) in groups.iter().enumerate() {
                println!("clc_ds{}={}", index + 1, group.join("|"));
            }
        }
        None => println!("clc_groups=0"),
    }

    if result.packages.is_empty() {
        println!("raw_response_start");
        println!("{}", result.raw_response);
        println!("raw_response_end");
    }

    Ok(())
}
