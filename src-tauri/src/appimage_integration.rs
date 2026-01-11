#[cfg(target_os = "linux")]
use std::env;
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::io;
#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::process::Command;
#[cfg(target_os = "linux")]
use tauri::Manager;

#[cfg(target_os = "linux")]
const DESKTOP_FILE_NAME: &str = "omnipacker.desktop";
#[cfg(target_os = "linux")]
const ICON_NAME: &str = "omnipacker";
#[cfg(target_os = "linux")]
const APP_NAME: &str = "OmniPacker";
#[cfg(target_os = "linux")]
const APP_COMMENT: &str = "DepotDownloader frontend";
#[cfg(target_os = "linux")]
const STARTUP_WM_CLASS: &str = "OmniPacker";
#[cfg(target_os = "linux")]
const MARKER_FILE_NAME: &str = "appimage_integration.txt";

#[cfg(target_os = "linux")]
const ICON_16: &[u8] = include_bytes!("../icons/icon-16.png");
#[cfg(target_os = "linux")]
const ICON_32: &[u8] = include_bytes!("../icons/icon-32.png");
#[cfg(target_os = "linux")]
const ICON_48: &[u8] = include_bytes!("../icons/icon-48.png");
#[cfg(target_os = "linux")]
const ICON_64: &[u8] = include_bytes!("../icons/icon-64.png");
#[cfg(target_os = "linux")]
const ICON_128: &[u8] = include_bytes!("../icons/icon-128.png");
#[cfg(target_os = "linux")]
const ICON_256: &[u8] = include_bytes!("../icons/icon-256.png");
#[cfg(target_os = "linux")]
const ICON_512: &[u8] = include_bytes!("../icons/icon-512.png");

#[cfg(target_os = "linux")]
const ICONS: &[(u32, &[u8])] = &[
    (16, ICON_16),
    (32, ICON_32),
    (48, ICON_48),
    (64, ICON_64),
    (128, ICON_128),
    (256, ICON_256),
    (512, ICON_512),
];

#[cfg(target_os = "linux")]
pub fn maybe_install_appimage_integration(app_handle: &tauri::AppHandle) {
    if let Err(err) = install_appimage_integration(app_handle) {
        eprintln!("AppImage desktop integration failed: {err}");
    }
}

#[cfg(not(target_os = "linux"))]
pub fn maybe_install_appimage_integration(_app_handle: &tauri::AppHandle) {}

#[cfg(target_os = "linux")]
fn install_appimage_integration(app_handle: &tauri::AppHandle) -> Result<(), String> {
    let appimage_path = match env::var_os("APPIMAGE") {
        Some(value) => PathBuf::from(value),
        None => return Ok(()),
    };
    if !appimage_path.is_file() {
        return Ok(());
    }
    let appimage_path = appimage_path.canonicalize().unwrap_or(appimage_path);
    let appimage_string = appimage_path.to_string_lossy().to_string();

    let home_dir = env::var_os("HOME").ok_or_else(|| "HOME is not set.".to_string())?;
    let home_dir = PathBuf::from(home_dir);
    let applications_dir = home_dir.join(".local/share/applications");
    let hicolor_dir = home_dir.join(".local/share/icons/hicolor");
    let desktop_entry_path = applications_dir.join(DESKTOP_FILE_NAME);
    let icon_probe_path = hicolor_dir
        .join("256x256")
        .join("apps")
        .join(format!("{ICON_NAME}.png"));

    let marker_path = marker_path(app_handle)?;
    if integration_is_current(
        &marker_path,
        &appimage_string,
        &desktop_entry_path,
        &icon_probe_path,
    ) {
        return Ok(());
    }

    fs::create_dir_all(&applications_dir).map_err(|e| {
        format!(
            "Failed to create applications directory {}: {}",
            applications_dir.display(),
            e
        )
    })?;

    let exec_entry = escape_desktop_exec(&appimage_path);
    let desktop_entry = format!(
        "[Desktop Entry]\nType=Application\nName={APP_NAME}\nComment={APP_COMMENT}\nExec={exec_entry}\nIcon={ICON_NAME}\nTerminal=false\nCategories=Utility;\nStartupWMClass={STARTUP_WM_CLASS}\n"
    );
    fs::write(&desktop_entry_path, desktop_entry).map_err(|e| {
        format!(
            "Failed to write desktop entry {}: {}",
            desktop_entry_path.display(),
            e
        )
    })?;

    for (size, bytes) in ICONS {
        let icon_dir = hicolor_dir
            .join(format!("{size}x{size}"))
            .join("apps");
        fs::create_dir_all(&icon_dir).map_err(|e| {
            format!(
                "Failed to create icon directory {}: {}",
                icon_dir.display(),
                e
            )
        })?;
        let icon_path = icon_dir.join(format!("{ICON_NAME}.png"));
        fs::write(&icon_path, bytes).map_err(|e| {
            format!("Failed to write icon {}: {}", icon_path.display(), e)
        })?;
    }

    run_optional_command("update-desktop-database", |cmd| {
        cmd.arg(&applications_dir);
    });
    run_optional_command("gtk-update-icon-cache", |cmd| {
        cmd.arg("-f").arg("-t").arg(&hicolor_dir);
    });

    if let Some(parent) = marker_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            format!(
                "Failed to create AppImage marker directory {}: {}",
                parent.display(),
                e
            )
        })?;
    }
    fs::write(&marker_path, appimage_string.as_bytes()).map_err(|e| {
        format!(
            "Failed to write AppImage integration marker {}: {}",
            marker_path.display(),
            e
        )
    })?;

    Ok(())
}

#[cfg(target_os = "linux")]
fn marker_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    app_handle
        .path()
        .resolve(MARKER_FILE_NAME, tauri::path::BaseDirectory::AppData)
        .map_err(|e| format!("Failed to resolve AppImage marker path: {e}"))
}

#[cfg(target_os = "linux")]
fn integration_is_current(
    marker_path: &Path,
    appimage_path: &str,
    desktop_entry_path: &Path,
    icon_probe_path: &Path,
) -> bool {
    let marker_matches = fs::read_to_string(marker_path)
        .map(|content| content.trim() == appimage_path)
        .unwrap_or(false);
    marker_matches && desktop_entry_path.is_file() && icon_probe_path.is_file()
}

#[cfg(target_os = "linux")]
fn escape_desktop_exec(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let mut sanitized = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch == '%' {
            sanitized.push_str("%%");
        } else {
            sanitized.push(ch);
        }
    }
    let needs_quotes = sanitized.contains(' ') || sanitized.contains('\t') || sanitized.contains('"');
    if !needs_quotes {
        return sanitized;
    }
    let mut quoted = String::with_capacity(sanitized.len() + 2);
    quoted.push('"');
    for ch in sanitized.chars() {
        match ch {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            _ => quoted.push(ch),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(target_os = "linux")]
fn run_optional_command<F>(command: &str, configure: F)
where
    F: FnOnce(&mut Command),
{
    let mut cmd = Command::new(command);
    configure(&mut cmd);
    match cmd.status() {
        Ok(_) => {}
        Err(err) => {
            if err.kind() != io::ErrorKind::NotFound {
                eprintln!("Failed to run {command}: {err}");
            }
        }
    }
}
