//! Verification progress event types and helpers.
//!
//! Progress payloads are shared by command handlers and provider orchestration,
//! so they live outside the main verifier runner to avoid making unrelated
//! modules depend on `pinout_verifier.rs`.

use serde::Serialize;

use crate::parser::verify_prompt::{provider_name, Provider};

fn is_false(value: &bool) -> bool {
    !*value
}

pub type ProgressCallback = dyn Fn(VerifyProgressUpdate) + Send + Sync;

#[derive(Debug, Clone, Serialize)]
pub struct VerifyProgressUpdate {
    pub stage: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub progress: f64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub indeterminate: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

impl VerifyProgressUpdate {
    pub fn new(stage: &str, progress: f64, label: impl Into<String>) -> Self {
        Self {
            stage: stage.to_string(),
            label: label.into(),
            detail: None,
            progress: progress.clamp(0.0, 1.0),
            indeterminate: false,
            provider: None,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn indeterminate(mut self, indeterminate: bool) -> Self {
        self.indeterminate = indeterminate;
        self
    }

    pub(crate) fn provider(mut self, provider: Provider) -> Self {
        self.provider = Some(provider_name(provider).to_string());
        self
    }
}

pub fn emit_progress(progress: Option<&ProgressCallback>, update: VerifyProgressUpdate) {
    if let Some(progress) = progress {
        progress(update);
    }
}
