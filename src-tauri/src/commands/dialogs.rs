//! Native dialog-backed file I/O commands.
//!
//! Keeps the Tauri dialog API and filesystem write/read plumbing out of the
//! higher-level device/codegen command handlers.

use std::fs;
use std::path::PathBuf;

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

use super::{
    apply_dialog_filters, encode_base64, file_name_or, resolve_dialog_path, write_text_file,
    ExportFilesRequest, ExportFilesResponse, OpenBinaryFileResponse, OpenFileDialogRequest,
    OpenTextFileResponse, SaveTextFileRequest, SavedPathResponse,
};

#[tauri::command]
pub async fn open_text_file_dialog(
    app: AppHandle,
    request: OpenFileDialogRequest,
) -> Result<Option<OpenTextFileResponse>, String> {
    let mut dialog = app.dialog().file();
    if let Some(title) = request.title {
        dialog = dialog.set_title(title);
    }
    dialog = apply_dialog_filters(dialog, &request.filters);

    let Some(file_path) = dialog.blocking_pick_file() else {
        return Ok(None);
    };

    let path = resolve_dialog_path(file_path)?;
    let contents =
        fs::read_to_string(&path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;

    Ok(Some(OpenTextFileResponse {
        path: path.display().to_string(),
        contents,
    }))
}

#[tauri::command]
pub async fn open_binary_file_dialog(
    app: AppHandle,
    request: OpenFileDialogRequest,
) -> Result<Option<OpenBinaryFileResponse>, String> {
    let mut dialog = app.dialog().file();
    if let Some(title) = request.title {
        dialog = dialog.set_title(title);
    }
    dialog = apply_dialog_filters(dialog, &request.filters);

    let Some(file_path) = dialog.blocking_pick_file() else {
        return Ok(None);
    };

    let path = resolve_dialog_path(file_path)?;
    let bytes = fs::read(&path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let name = file_name_or(&path, "selected-file");

    Ok(Some(OpenBinaryFileResponse {
        path: path.display().to_string(),
        name,
        base64: encode_base64(&bytes),
    }))
}

#[tauri::command]
pub async fn save_text_file_dialog(
    app: AppHandle,
    request: SaveTextFileRequest,
) -> Result<Option<SavedPathResponse>, String> {
    let mut dialog = app.dialog().file().set_file_name(request.suggested_name);
    if let Some(title) = request.title {
        dialog = dialog.set_title(title);
    }
    dialog = apply_dialog_filters(dialog, &request.filters);

    let Some(file_path) = dialog.blocking_save_file() else {
        return Ok(None);
    };

    let path = resolve_dialog_path(file_path)?;
    write_text_file(&path, &request.contents)?;

    Ok(Some(SavedPathResponse {
        path: path.display().to_string(),
    }))
}

#[tauri::command]
pub async fn write_text_file_path(
    path: String,
    contents: String,
) -> Result<SavedPathResponse, String> {
    let path = PathBuf::from(path);
    write_text_file(&path, &contents)?;

    Ok(SavedPathResponse {
        path: path.display().to_string(),
    })
}

#[tauri::command]
pub async fn delete_file_path(path: String) -> Result<(), String> {
    let path = PathBuf::from(path);
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(&path).map_err(|e| format!("Cannot delete {}: {e}", path.display()))
}

#[tauri::command]
pub async fn export_generated_files_dialog(
    app: AppHandle,
    request: ExportFilesRequest,
) -> Result<Option<ExportFilesResponse>, String> {
    if request.files.is_empty() {
        return Err("No files available to export".to_string());
    }

    let mut dialog = app.dialog().file();
    if let Some(title) = request.title {
        dialog = dialog.set_title(title);
    }

    let Some(folder_path) = dialog.blocking_pick_folder() else {
        return Ok(None);
    };

    let directory = resolve_dialog_path(folder_path)?;
    fs::create_dir_all(&directory).map_err(|e| {
        format!(
            "Cannot create export directory {}: {e}",
            directory.display()
        )
    })?;

    let mut written_files = Vec::new();
    for (filename, contents) in request.files {
        let path = directory.join(&filename);
        write_text_file(&path, &contents)?;
        written_files.push(path.display().to_string());
    }
    written_files.sort();

    Ok(Some(ExportFilesResponse {
        directory: directory.display().to_string(),
        written_files,
    }))
}
