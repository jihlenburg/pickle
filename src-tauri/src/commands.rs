//! Tauri command module root.
//!
//! This file now acts as a facade over the concrete command implementations,
//! shared IPC request/response types, and small cross-command helpers. Keeping
//! the root thin makes it easier to evolve individual command areas without
//! turning `commands.rs` back into a monolith.

pub mod catalog;
pub mod devices;
pub mod dialogs;
pub mod keychain;
pub mod settings_state;
pub(crate) mod support;
pub mod toolchain;
pub mod types;
pub mod verification;
pub(crate) mod verification_support;

pub use catalog::{index_status, list_devices, refresh_index};
pub use devices::{generate_code, load_device};
pub use dialogs::{
    delete_file_path, export_generated_files_dialog, open_binary_file_dialog,
    open_text_file_dialog, save_text_file_dialog, write_text_file_path,
};
pub use keychain::{api_key_details, delete_api_key, save_api_key};
pub use settings_state::{
    load_app_settings, remember_last_used_device, set_theme_mode, set_verify_provider,
    set_welcome_intro_seen,
};
pub use toolchain::{compile_check, compiler_info};
pub use types::{
    ApiKeyStatusResponse, AppSettingsResponse, ApplyOverlayRequest, AssignmentRequest,
    CodegenRequest, CompileCheckRequest, CompileCheckResponse, CompilerResponse,
    DeleteOverlayPackageRequest, DeviceListResponse, DialogFilterRequest, ExportFilesRequest,
    ExportFilesResponse, FuseRequest, IndexStatusResponse, OpenBinaryFileResponse,
    OpenFileDialogRequest, OpenTextFileResponse, OscRequest, RefreshResponse,
    RenameOverlayPackageRequest, SaveTextFileRequest, SavedPathResponse,
    SetPackageDisplayNameRequest,
};
pub use verification::{
    api_key_status, apply_overlay, delete_overlay_package, find_datasheet, rename_overlay_package,
    set_package_display_name, verify_clc, verify_pinout,
};

pub(crate) use support::{
    apply_dialog_filters, device_packages, encode_base64, file_name_or, parse_u32_keyed_map,
    resolve_dialog_path, round_tenths, selected_package, write_text_file,
};
