//! Secure API-key storage via the OS credential store.
//!
//! Keys are stored in the platform-native keychain (macOS Keychain, Windows
//! Credential Manager, Linux Secret Service).  Each provider gets its own
//! entry under the service name `pickle`.
//!
//! Resolution order when the app needs an API key:
//!   1. OS keychain  (this module)
//!   2. Environment variable  (`OPENAI_API_KEY` / `ANTHROPIC_API_KEY`)
//!   3. `.env` file in a data root
//!
//! The functions here are exposed as Tauri commands so the frontend Settings
//! dialog can manage keys without touching the filesystem.

use keyring::Entry;
use serde::Serialize;

/// Keyring service name shared by all pickle credentials.
const SERVICE: &str = "pickle";

/// Translate a provider name (`"openai"` | `"anthropic"`) to the keyring
/// username used for storage.
fn keyring_user(provider: &str) -> Result<&'static str, String> {
    match provider {
        "openai" => Ok("openai-api-key"),
        "anthropic" => Ok("anthropic-api-key"),
        _ => Err(format!("Unknown provider: {provider}")),
    }
}

/// Build a [`keyring::Entry`] for the given provider.
fn entry(provider: &str) -> Result<Entry, String> {
    let user = keyring_user(provider)?;
    Entry::new(SERVICE, user).map_err(|e| format!("Keychain error: {e}"))
}

// ── Tauri commands ──────────────────────────────────────────────────────

/// Store an API key in the OS keychain.
#[tauri::command]
pub fn save_api_key(provider: String, key: String) -> Result<(), String> {
    let e = entry(&provider)?;
    e.set_password(&key)
        .map_err(|e| format!("Failed to save key: {e}"))
}

/// Remove an API key from the OS keychain.
#[tauri::command]
pub fn delete_api_key(provider: String) -> Result<(), String> {
    let e = entry(&provider)?;
    match e.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()), // already absent
        Err(e) => Err(format!("Failed to delete key: {e}")),
    }
}

/// Per-provider status returned by [`api_key_details`].
#[derive(Serialize, Clone)]
pub struct ProviderKeyStatus {
    pub configured: bool,
    /// Where the key was found: `"keychain"`, `"env"`, `"dotenv"`, or `null`.
    pub source: Option<String>,
    /// Last four characters of the key (e.g. `"...aB1x"`).
    pub hint: Option<String>,
}

/// Combined status for all supported providers.
#[derive(Serialize)]
pub struct ApiKeyDetailsResponse {
    pub openai: ProviderKeyStatus,
    pub anthropic: ProviderKeyStatus,
}

/// Return per-provider key status for the Settings dialog.
///
/// Checks each source in priority order and reports which one matched.
#[tauri::command]
pub fn api_key_details() -> Result<ApiKeyDetailsResponse, String> {
    Ok(ApiKeyDetailsResponse {
        openai: probe_provider("openai", "OPENAI_API_KEY"),
        anthropic: probe_provider("anthropic", "ANTHROPIC_API_KEY"),
    })
}

/// Check keychain → env → .env for a single provider and return its status.
fn probe_provider(provider: &str, env_var: &str) -> ProviderKeyStatus {
    // 1. Keychain
    if let Ok(e) = entry(provider) {
        if let Ok(key) = e.get_password() {
            if !key.is_empty() {
                return ProviderKeyStatus {
                    configured: true,
                    source: Some("keychain".into()),
                    hint: Some(hint(&key)),
                };
            }
        }
    }

    // 2. Environment variable
    if let Ok(key) = std::env::var(env_var) {
        if !key.is_empty() {
            return ProviderKeyStatus {
                configured: true,
                source: Some("env".into()),
                hint: Some(hint(&key)),
            };
        }
    }

    // 3. .env files in data roots
    for root in crate::parser::dfp_manager::read_roots() {
        let env_path = root.join(".env");
        if let Ok(text) = std::fs::read_to_string(&env_path) {
            for line in text.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix(&format!("{env_var}=")) {
                    let key = rest.trim();
                    if !key.is_empty() {
                        return ProviderKeyStatus {
                            configured: true,
                            source: Some("dotenv".into()),
                            hint: Some(hint(key)),
                        };
                    }
                }
            }
        }
    }

    ProviderKeyStatus {
        configured: false,
        source: None,
        hint: None,
    }
}

/// Return the last four characters of a key, prefixed with `"..."`.
fn hint(key: &str) -> String {
    format!("...{}", &key[key.len().saturating_sub(4)..])
}

// ── Public helper for pinout_verifier ───────────────────────────────────

/// Try to read a key from the OS keychain.  Returns `None` if not stored
/// or if the keychain is unavailable (e.g. headless Linux without a
/// secret-service daemon).
pub fn get_keychain_key(provider: &str) -> Option<String> {
    let e = entry(provider).ok()?;
    e.get_password().ok().filter(|k| !k.is_empty())
}
