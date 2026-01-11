mod debug_console;
mod depot_runner;
mod job_finalization;
mod job_metadata;
mod job_staging;
mod login_store;
mod manifest_preflight;
mod output_conflict;
mod output_dir;
mod appimage_integration;
mod steam_api;
mod steamdb_api;
mod template_metadata;
mod template_renderer;
mod template_store;
mod zip_runner;

use debug_console::{debug_console_enabled, debug_console_log, DebugConsoleState};
use depot_runner::{
    cancel_depotdownloader, run_depotdownloader, submit_steam_guard_code, DepotRunnerState,
};
use job_staging::cleanup_orphaned_staging;
use login_store::{delete_login_data, load_login_data, save_login_data};
use output_conflict::{resolve_output_conflict, OutputConflictState};
use output_dir::{get_output_folder, open_output_folder};
use template_metadata::{get_template_metadata, TemplateMetadataState};
use template_store::{load_template_data, save_template_data};
use zip_runner::{cancel_7zip, run_7zip, SevenZipRunnerState};
use std::sync::OnceLock;
use tauri::Manager;

fn load_window_icon() -> Option<tauri::image::Image<'static>> {
    static ICON: OnceLock<Option<tauri::image::Image<'static>>> = OnceLock::new();
    ICON.get_or_init(|| {
        let icon_bytes = include_bytes!("../icons/icon-512.png");
        tauri::image::Image::from_bytes(icon_bytes).ok()
    })
    .clone()
}

fn debug_console_from_args() -> bool {
    std::env::args().any(|arg| arg == "--debug")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let debug_console_flag = debug_console_from_args();
    tauri::Builder::default()
        .manage(DepotRunnerState::new())
        .manage(SevenZipRunnerState::new())
        .manage(TemplateMetadataState::default())
        .manage(OutputConflictState::new())
        .manage(DebugConsoleState::new(debug_console_flag))
        .setup(|app| {
            let app_handle = app.handle();
            match cleanup_orphaned_staging(&app_handle) {
                Ok(count) => {
                    if count > 0 {
                        eprintln!("Cleaned up {count} orphaned staging entries.");
                    }
                }
                Err(err) => {
                    eprintln!("Failed to clean staging directory on startup: {err}");
                }
            }
            appimage_integration::maybe_install_appimage_integration(&app_handle);
            if let Some(icon) = load_window_icon() {
                // Set icon on all windows
                for (_, window) in app.webview_windows() {
                    if let Err(err) = window.set_icon(icon.clone()) {
                        eprintln!("Failed to set window icon: {err}");
                    }
                }
            }
            Ok(())
        })
        .on_page_load(|webview, _| {
            if let Some(icon) = load_window_icon() {
                if let Err(err) = webview.window().set_icon(icon) {
                    eprintln!("Failed to set window icon on page load: {err}");
                }
            }
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            debug_console_enabled,
            debug_console_log,
            run_depotdownloader,
            cancel_depotdownloader,
            submit_steam_guard_code,
            run_7zip,
            cancel_7zip,
            open_output_folder,
            get_output_folder,
            save_login_data,
            load_login_data,
            delete_login_data,
            get_template_metadata,
            save_template_data,
            load_template_data,
            resolve_output_conflict
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
