//! Verification provider selection and dispatch.
//!
//! This module keeps provider choice, API-key lookup, and transport dispatch in
//! one place so the verifier runner can stay focused on cache scope and local
//! comparison. Provider-specific HTTP payloads live in dedicated submodules.

use std::fs;

use crate::parser::verify_progress::ProgressCallback;
use crate::parser::verify_prompt::{Provider, VerifyTask};
use crate::parser::{verify_provider_anthropic, verify_provider_openai};

fn get_env_key(var_name: &str) -> Option<String> {
    // Prefer the OS keychain because the Settings dialog writes there and users
    // expect it to override ad-hoc shell environment state.
    let provider = match var_name {
        "OPENAI_API_KEY" => Some("openai"),
        "ANTHROPIC_API_KEY" => Some("anthropic"),
        _ => None,
    };
    if let Some(provider) = provider {
        if let Some(key) = crate::commands::keychain::get_keychain_key(provider) {
            return Some(key);
        }
    }

    if let Ok(key) = std::env::var(var_name) {
        if !key.is_empty() {
            return Some(key);
        }
    }

    // Fall back to repo/app-data `.env` files so CLI-driven development can
    // keep working without having to seed the native keychain first.
    for root in crate::parser::dfp_manager::read_roots() {
        let env_path = root.join(".env");
        if !env_path.exists() {
            continue;
        }
        if let Ok(text) = fs::read_to_string(&env_path) {
            for line in text.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix(&format!("{}=", var_name)) {
                    let key = rest.trim();
                    if !key.is_empty() {
                        return Some(key.to_string());
                    }
                }
            }
        }
    }

    None
}

fn preferred_provider_setting() -> String {
    crate::settings::load()
        .map(|settings| settings.verification.provider)
        .unwrap_or_else(|_| crate::settings::default_verification_provider())
}

/// Determine which provider to use and return `(Provider, api_key)`.
pub(crate) fn resolve_provider(override_key: Option<&str>) -> Result<(Provider, String), String> {
    if let Some(key) = override_key.filter(|value| !value.is_empty()) {
        let provider = if key.starts_with("sk-proj-") || key.starts_with("sk-org-") {
            Provider::OpenAI
        } else {
            Provider::Anthropic
        };
        return Ok((provider, key.to_string()));
    }

    match preferred_provider_setting().as_str() {
        "openai" => {
            return get_env_key("OPENAI_API_KEY")
                .map(|key| (Provider::OpenAI, key))
                .ok_or_else(|| {
                    "Verification provider is set to OpenAI, but no OpenAI API key is configured."
                        .to_string()
                });
        }
        "anthropic" => {
            return get_env_key("ANTHROPIC_API_KEY")
                .map(|key| (Provider::Anthropic, key))
                .ok_or_else(|| {
                    "Verification provider is set to Anthropic, but no Anthropic API key is configured."
                        .to_string()
                });
        }
        _ => {}
    }

    if let Some(key) = get_env_key("OPENAI_API_KEY") {
        return Ok((Provider::OpenAI, key));
    }
    if let Some(key) = get_env_key("ANTHROPIC_API_KEY") {
        return Ok((Provider::Anthropic, key));
    }

    Err("No API key configured. Set OPENAI_API_KEY or ANTHROPIC_API_KEY in .env".to_string())
}

pub fn get_api_key() -> Option<String> {
    resolve_provider(None).ok().map(|(_, key)| key)
}

pub(crate) struct VerifyRequest<'a> {
    pub pdf_bytes: &'a [u8],
    pub datasheet_text: Option<&'a str>,
    pub task: VerifyTask,
    pub prompt: &'a str,
    pub api_key: &'a str,
    pub part_number: &'a str,
    pub progress: Option<&'a ProgressCallback>,
}

pub(crate) fn call_llm_api(provider: Provider, req: VerifyRequest<'_>) -> Result<String, String> {
    if req.pdf_bytes.is_empty() {
        let detail = if req.datasheet_text.is_some() {
            " Text-only datasheet fallback is disabled."
        } else {
            ""
        };
        return Err(format!(
            "Verification requires a datasheet PDF so pickle can send selected pages or rendered page images.{detail}"
        ));
    }

    match provider {
        Provider::Anthropic => verify_provider_anthropic::call_anthropic_api(
            req.pdf_bytes,
            req.task,
            req.prompt,
            req.api_key,
            req.part_number,
            req.progress,
        ),
        Provider::OpenAI => verify_provider_openai::call_openai_api(
            req.pdf_bytes,
            req.datasheet_text,
            req.task,
            req.prompt,
            req.api_key,
            req.part_number,
            req.progress,
        ),
    }
}
