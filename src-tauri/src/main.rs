// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use log::info;
use pickle_lib::commands;
use tauri::menu::{AboutMetadataBuilder, MenuBuilder, MenuItem, PredefinedMenuItem, Submenu};
use tauri::Emitter;
use tauri_plugin_log::{Target, TargetKind, TimezoneStrategy};

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
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            info!("pickle v{} starting", env!("CARGO_PKG_VERSION"));
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
            let about_meta = AboutMetadataBuilder::new()
                .name(Some("pickle"))
                .version(Some(env!("CARGO_PKG_VERSION")))
                .comments(Some("Pin configurator for Microchip dsPIC33"))
                .build();

            let file_menu = Submenu::with_items(
                app,
                "File",
                true,
                &[
                    &MenuItem::with_id(app, "open", "Open...", true, Some("CmdOrCtrl+O"))?,
                    &MenuItem::with_id(app, "save", "Save...", true, Some("CmdOrCtrl+S"))?,
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
            )?;

            let edit_menu = Submenu::with_items(
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
            )?;

            let view_menu = Submenu::with_items(
                app,
                "View",
                true,
                &[
                    &MenuItem::with_id(
                        app,
                        "generate",
                        "Generate Code",
                        true,
                        Some("CmdOrCtrl+G"),
                    )?,
                    &MenuItem::with_id(
                        app,
                        "copy_code",
                        "Copy Code",
                        true,
                        Some("CmdOrCtrl+Shift+C"),
                    )?,
                ],
            )?;

            let help_menu = Submenu::with_items(
                app,
                "Help",
                true,
                &[&PredefinedMenuItem::about(app, None, Some(about_meta))?],
            )?;

            let menu = MenuBuilder::new(app)
                .items(&[&file_menu, &edit_menu, &view_menu, &help_menu])
                .build()?;

            app.set_menu(menu)?;
            Ok(())
        })
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "open" | "save" | "export" | "undo" | "redo" | "generate" | "copy_code" => {
                    let _ = app.emit("menu-action", id);
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_devices,
            commands::refresh_index,
            commands::index_status,
            commands::load_device,
            commands::open_text_file_dialog,
            commands::open_binary_file_dialog,
            commands::save_text_file_dialog,
            commands::export_generated_files_dialog,
            commands::generate_code,
            commands::compiler_info,
            commands::compile_check,
            commands::find_datasheet,
            commands::verify_pinout,
            commands::apply_overlay,
            commands::api_key_status,
        ])
        .run(tauri::generate_context!())
        .expect("error running tauri application");
}
