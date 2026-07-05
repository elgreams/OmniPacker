mod acf_generator;
pub(crate) mod debug_console;
mod debug_log;
mod depot_runner;
mod job_finalization;
mod job_metadata;
mod job_staging;
mod login_store;
mod manifest_preflight;
mod output_conflict;
pub(crate) mod output_dir;
mod output_override;
mod shared_depots;
mod appimage_integration;
mod steam_api;
mod steamcmd_api;
mod steamdb_api;
mod template_metadata;
mod template_renderer;
mod template_store;
mod update_check;
mod zip_runner;

use debug_console::{debug_console_enabled, debug_console_log, DebugConsoleState};
use depot_runner::{
    cancel_depotdownloader, run_depotdownloader, submit_steam_guard_code, DepotRunnerState,
};
use job_staging::cleanup_orphaned_staging;
use login_store::{
    delete_login_data, load_compression_password, load_login_data, save_compression_password,
    save_login_data,
};
use output_conflict::{resolve_output_conflict, OutputConflictState};
use output_dir::{get_output_folder, open_output_folder};
use output_override::{
    clear_output_override, get_output_override, pick_output_folder, set_output_override,
};
use template_metadata::{get_template_metadata, TemplateMetadataState};
use template_store::{
    delete_profile, list_profiles, open_profiles_folder, save_profile,
};
use update_check::{check_for_update, get_app_version, open_external_url};
use zip_runner::{cancel_7zip, SevenZipRunnerState};
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

/// Design size the UI is laid out for. Matches the defaults in tauri.conf.json.
const DESIGN_WIDTH: f64 = 1140.0;
const DESIGN_HEIGHT: f64 = 760.0;
/// On startup, shrink the window to fit the monitor's usable area if the
/// design size is too tall/wide for the current display. The frontend's
/// zoom-to-fit then scales the UI to whatever size the window ends up at.
fn fit_window_to_monitor(window: &tauri::WebviewWindow) {
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };

    let scale = monitor.scale_factor();
    let work = monitor.work_area();
    let avail_w = work.size.width as f64 / scale;
    let avail_h = work.size.height as f64 / scale;

    // Leave a small margin so the window doesn't butt against screen edges.
    let target_w = DESIGN_WIDTH.min(avail_w - 32.0);
    let target_h = DESIGN_HEIGHT.min(avail_h - 64.0);

    if target_w < DESIGN_WIDTH || target_h < DESIGN_HEIGHT {
        let _ = window.set_size(tauri::LogicalSize::new(
            target_w.max(570.0),
            target_h.max(380.0),
        ));
        let _ = window.center();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // WebKitGTK's DMA-BUF renderer fails on some driver/compositor combos
    // (NVIDIA proprietary, VMs, certain Wayland setups) with
    // "Could not create default EGL display: EGL_BAD_PARAMETER" and a grey,
    // never-painted window. Force the shared-memory rendering path instead;
    // the visual result is identical for a UI like this. Respect an explicit
    // user override, and set it before any GTK/WebKit initialization.
    // See tauri-apps/tauri#9304.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    let debug_console_flag = debug_console_from_args();
    tauri::Builder::default()
        // Must be the first plugin so a second launch exits before any other
        // initialization runs. Without this, instance B's startup staging
        // sweep (cleanup_orphaned_staging below) would delete instance A's
        // live download staging out from under DepotDownloader, and the two
        // instances would race on shared config files (login.dat, output
        // override, DD account tokens).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // A second launch attempt lands here in the FIRST instance:
            // surface the existing window instead.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
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
            if let Some(window) = app.get_webview_window("main") {
                fit_window_to_monitor(&window);
            }
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
            cancel_7zip,
            open_output_folder,
            get_output_folder,
            get_output_override,
            set_output_override,
            clear_output_override,
            pick_output_folder,
            save_login_data,
            load_login_data,
            delete_login_data,
            save_compression_password,
            load_compression_password,
            get_template_metadata,
            list_profiles,
            save_profile,
            delete_profile,
            open_profiles_folder,
            resolve_output_conflict,
            check_for_update,
            get_app_version,
            open_external_url
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // Kill any spawned sidecar children (DepotDownloader, 7-Zip) before
            // the app exits. On Windows, closing the window does not terminate
            // child processes, and a compressing 7-Zip child writes to a file
            // rather than the broken parent pipe, so it would otherwise keep
            // running orphaned. This fires synchronously on exit, unlike the
            // Drop impls which are not guaranteed to run on an abrupt teardown.
            if let tauri::RunEvent::ExitRequested { .. } = event {
                app_handle.state::<DepotRunnerState>().kill_child();
                app_handle.state::<SevenZipRunnerState>().kill_child();
            }
        });
}
