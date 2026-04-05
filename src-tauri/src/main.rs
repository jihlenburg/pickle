// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri desktop bootstrap for pickle.
//!
//! Owns plugin registration, native menu construction, startup logging, and
//! the IPC command surface exposed to the frontend webview.

use log::info;
use pickle_lib::commands;
use tauri::menu::{Menu, MenuBuilder, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{App, AppHandle, Emitter, Runtime};
use tauri_plugin_log::{Target, TargetKind, TimezoneStrategy};

const FORWARDED_MENU_ACTIONS: &[&str] = &[
    "open",
    "save",
    "save_as",
    "rename",
    "export",
    "undo",
    "redo",
    "generate",
    "copy_code",
    "about",
];

fn build_file_menu<R: Runtime>(app: &App<R>) -> tauri::Result<Submenu<R>> {
    Submenu::with_items(
        app,
        "File",
        true,
        &[
            &MenuItem::with_id(app, "open", "Open...", true, Some("CmdOrCtrl+O"))?,
            &MenuItem::with_id(app, "save", "Save", true, Some("CmdOrCtrl+S"))?,
            &MenuItem::with_id(
                app,
                "save_as",
                "Save As...",
                true,
                Some("CmdOrCtrl+Shift+S"),
            )?,
            &MenuItem::with_id(app, "rename", "Rename...", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(
                app,
                "export",
                "Export C Files...",
                true,
                Some("CmdOrCtrl+E"),
            )?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )
}

fn build_edit_menu<R: Runtime>(app: &App<R>) -> tauri::Result<Submenu<R>> {
    Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &MenuItem::with_id(app, "undo", "Undo", true, Some("CmdOrCtrl+Z"))?,
            &MenuItem::with_id(app, "redo", "Redo", true, Some("CmdOrCtrl+Shift+Z"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )
}

fn build_view_menu<R: Runtime>(app: &App<R>) -> tauri::Result<Submenu<R>> {
    Submenu::with_items(
        app,
        "View",
        true,
        &[
            &MenuItem::with_id(app, "generate", "Generate Code", true, Some("CmdOrCtrl+G"))?,
            &MenuItem::with_id(
                app,
                "copy_code",
                "Copy Code",
                true,
                Some("CmdOrCtrl+Shift+C"),
            )?,
        ],
    )
}

fn build_help_menu<R: Runtime>(app: &App<R>) -> tauri::Result<Submenu<R>> {
    Submenu::with_items(
        app,
        "Help",
        true,
        &[&MenuItem::with_id(app, "about", "About pickle", true, None::<&str>)?],
    )
}

fn build_menu<R: Runtime>(app: &App<R>) -> tauri::Result<Menu<R>> {
    let file_menu = build_file_menu(app)?;
    let edit_menu = build_edit_menu(app)?;
    let view_menu = build_view_menu(app)?;
    let help_menu = build_help_menu(app)?;

    MenuBuilder::new(app)
        .items(&[&file_menu, &edit_menu, &view_menu, &help_menu])
        .build()
}

fn log_runtime_paths() {
    info!(
        "app-data fallback dir: {:?}",
        pickle_lib::parser::dfp_manager::base_dir()
    );
    info!(
        "data read roots: {:?}",
        pickle_lib::parser::dfp_manager::read_roots()
    );
    info!(
        "device cache dir: {:?}",
        pickle_lib::parser::dfp_manager::devices_dir()
    );
    info!(
        "dfp cache dir: {:?}",
        pickle_lib::parser::dfp_manager::dfp_cache_dir()
    );
    info!(
        "pinouts dir: {:?}",
        pickle_lib::parser::dfp_manager::pinouts_dir()
    );
}

fn forward_menu_action<R: Runtime>(app: &AppHandle<R>, id: &str) {
    if FORWARDED_MENU_ACTIONS.contains(&id) {
        let _ = app.emit("menu-action", id);
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir {
                        file_name: Some("pickle".into()),
                    }),
                    Target::new(TargetKind::Webview),
                ])
                .level(log::LevelFilter::Info)
                .timezone_strategy(TimezoneStrategy::UseLocal)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            info!("pickle v{} starting", env!("CARGO_PKG_VERSION"));
            log_runtime_paths();
            app.set_menu(build_menu(app)?)?;
            Ok(())
        })
        .on_menu_event(|app, event| forward_menu_action(app, event.id().as_ref()))
        .invoke_handler(tauri::generate_handler![
            commands::catalog::list_devices,
            commands::catalog::refresh_index,
            commands::catalog::index_status,
            commands::settings_state::load_app_settings,
            commands::settings_state::set_theme_mode,
            commands::settings_state::remember_last_used_device,
            commands::devices::load_device,
            commands::dialogs::open_text_file_dialog,
            commands::dialogs::open_binary_file_dialog,
            commands::dialogs::save_text_file_dialog,
            commands::dialogs::write_text_file_path,
            commands::dialogs::delete_file_path,
            commands::dialogs::export_generated_files_dialog,
            commands::devices::generate_code,
            commands::toolchain::compiler_info,
            commands::toolchain::compile_check,
            commands::verification::find_datasheet,
            commands::verification::verify_pinout,
            commands::verification::apply_overlay,
            commands::verification::api_key_status,
        ])
        .run(tauri::generate_context!())
        .expect("error running tauri application");
}
