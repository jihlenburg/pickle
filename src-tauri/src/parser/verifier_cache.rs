//! Verification-result cache keyed by datasheet bytes plus request scope.
//!
//! This is intentionally separate from the verifier implementation so prompt
//! orchestration does not also own file-system cache hashing and persistence.

use serde_json::Value;
use std::fs;
use std::path::PathBuf;

use crate::parser::dfp_manager::dfp_cache_dir;

fn verify_cache_dir() -> PathBuf {
    let dir = dfp_cache_dir().join("verify_cache");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn pdf_cache_key(pdf_bytes: &[u8], scope: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in pdf_bytes.iter().take(65_536) {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash ^= pdf_bytes.len() as u64;
    hash = hash.wrapping_mul(0x100000001b3);
    for &byte in scope.as_bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash ^= scope.len() as u64;
    hash = hash.wrapping_mul(0x100000001b3);
    format!("{hash:016x}")
}

pub fn verify_cache_disabled() -> bool {
    matches!(
        std::env::var("PICKLE_DISABLE_VERIFY_CACHE")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase()),
        Some(ref value) if value == "1" || value == "true" || value == "yes"
    )
}

pub fn load_cached_verify(pdf_bytes: &[u8], scope: &str) -> Option<Value> {
    let key = pdf_cache_key(pdf_bytes, scope);
    let path = verify_cache_dir().join(format!("{key}.json"));
    let text = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_cached_verify(pdf_bytes: &[u8], scope: &str, raw_json: &Value) {
    let key = pdf_cache_key(pdf_bytes, scope);
    let path = verify_cache_dir().join(format!("{key}.json"));
    if let Ok(text) = serde_json::to_string_pretty(raw_json) {
        let _ = fs::write(&path, text);
    }
}

#[cfg(test)]
mod tests {
    use super::pdf_cache_key;

    #[test]
    fn cache_key_changes_when_scope_changes() {
        let pdf = b"%PDF-test-data";
        assert_ne!(
            pdf_cache_key(pdf, "provider=OpenAI"),
            pdf_cache_key(pdf, "provider=Anthropic")
        );
    }

    #[test]
    fn cache_key_changes_when_pdf_changes() {
        assert_ne!(
            pdf_cache_key(b"%PDF-one", "scope"),
            pdf_cache_key(b"%PDF-two", "scope")
        );
    }
}
