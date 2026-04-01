use std::env;
use std::fs;
use std::path::Path;

fn read_version_file(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
        .trim()
        .to_string()
}

fn read_tauri_version(path: &Path) -> String {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    let json: serde_json::Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));
    json.get("version")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("missing top-level version in {}", path.display()))
        .to_string()
}

fn main() {
    let version_file = Path::new("../VERSION");
    let tauri_conf = Path::new("tauri.conf.json");

    println!("cargo:rerun-if-changed={}", version_file.display());
    println!("cargo:rerun-if-changed={}", tauri_conf.display());

    let canonical_version = read_version_file(version_file);
    let cargo_version = env::var("CARGO_PKG_VERSION").expect("missing CARGO_PKG_VERSION");
    let tauri_version = read_tauri_version(tauri_conf);

    assert_eq!(
        canonical_version, cargo_version,
        "VERSION ({canonical_version}) must match Cargo.toml package.version ({cargo_version})"
    );
    assert_eq!(
        canonical_version, tauri_version,
        "VERSION ({canonical_version}) must match tauri.conf.json version ({tauri_version})"
    );

    tauri_build::build()
}
