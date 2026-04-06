//! Persisted frontend-behavior settings commands.

use crate::settings;

use super::AppSettingsResponse;

#[tauri::command]
pub fn load_app_settings() -> Result<AppSettingsResponse, String> {
    let settings = settings::load()?;
    Ok(AppSettingsResponse {
        path: settings::settings_path().display().to_string(),
        settings,
    })
}

#[tauri::command]
pub fn set_theme_mode(theme: String) -> Result<(), String> {
    let mut settings = settings::load()?;
    settings.appearance.theme = theme;
    settings::save(&settings.normalized())
}

#[tauri::command]
pub fn set_verify_provider(provider: String) -> Result<(), String> {
    let mut settings = settings::load()?;
    settings.verification.provider = provider;
    settings::save(&settings.normalized())
}

#[tauri::command]
pub fn remember_last_used_device(
    part_number: String,
    package: Option<String>,
) -> Result<(), String> {
    let mut settings = settings::load()?;
    settings.last_used.part_number = part_number;
    settings.last_used.package = package.unwrap_or_default();
    settings::save(&settings.normalized())
}
