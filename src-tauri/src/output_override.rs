use std::path::{Path, PathBuf};
use std::sync::mpsc;

use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

const OVERRIDE_FILE_NAME: &str = "output_dir.txt";

/// Path to the file that persists the user's custom output directory. Stored in
/// AppData (not the localStorage settings blob) because it's an OS path the
/// backend must validate, and it should survive a localStorage clear.
fn override_file_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    app_handle
        .path()
        .resolve(OVERRIDE_FILE_NAME, tauri::path::BaseDirectory::AppData)
        .map_err(|e| format!("Failed to resolve output override path: {e}"))
}

/// Reads the persisted custom output directory, if one is set. Returns `None`
/// when no override file exists or it's empty. Does NOT validate writability —
/// that happens at set time and again at job time.
pub fn read_override(app_handle: &tauri::AppHandle) -> Option<PathBuf> {
    let path = override_file_path(app_handle).ok()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

/// Confirms a directory exists and can be written to, creating it if needed.
/// Shared shape with output_dir::ensure_writable_dir but kept local so the
/// override module owns its own validation message.
fn validate_writable_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path)
        .map_err(|e| format!("Output folder could not be created: {} ({e})", path.display()))?;

    let probe = path.join(".omnipacker_write_test");
    match std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            Ok(())
        }
        Err(e) => Err(format!(
            "Output folder is not writable: {} ({e})",
            path.display()
        )),
    }
}

/// Returns the currently-saved custom output directory as a string, or `None`
/// when the default (app-managed) location is in use.
#[tauri::command]
pub fn get_output_override(app_handle: tauri::AppHandle) -> Result<Option<String>, String> {
    Ok(read_override(&app_handle).map(|p| p.to_string_lossy().to_string()))
}

/// Persists a custom output directory after confirming it exists and is
/// writable. Finished packages are written here directly; transient scratch
/// (staging, temp assembly, logs) goes in a hidden `.omnipacker-tmp/` beneath it
/// (see output_dir::resolve_outputs_dir / resolve_scratch_dir).
#[tauri::command]
pub fn set_output_override(app_handle: tauri::AppHandle, path: String) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return clear_output_override(app_handle);
    }

    let dir = PathBuf::from(trimmed);
    validate_writable_dir(&dir)?;

    let file = override_file_path(&app_handle)?;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {e}"))?;
    }
    std::fs::write(&file, dir.to_string_lossy().as_bytes())
        .map_err(|e| format!("Failed to save output override: {e}"))?;
    Ok(())
}

/// Removes the custom output directory, reverting to the default location.
#[tauri::command]
pub fn clear_output_override(app_handle: tauri::AppHandle) -> Result<(), String> {
    let file = override_file_path(&app_handle)?;
    if file.exists() {
        std::fs::remove_file(&file)
            .map_err(|e| format!("Failed to clear output override: {e}"))?;
    }
    Ok(())
}

/// Opens a native folder picker and returns the chosen path, or `None` if the
/// user cancelled. Does not persist — the frontend calls `set_output_override`
/// with the result so validation runs once, centrally.
///
/// This MUST be `async`: the non-blocking `pick_folder` dispatches the dialog to
/// the main-thread event loop, so if the command itself blocked the main thread
/// waiting for the result, the dialog could never run and the app would freeze.
/// As an async command it runs off the main thread, and the blocking channel
/// `recv` is moved onto a blocking worker via `spawn_blocking`, leaving the main
/// loop free to drive the dialog.
#[tauri::command]
pub async fn pick_output_folder(app_handle: tauri::AppHandle) -> Result<Option<String>, String> {
    let (tx, rx) = mpsc::channel();
    app_handle.dialog().file().pick_folder(move |choice| {
        let _ = tx.send(choice);
    });
    let choice = tauri::async_runtime::spawn_blocking(move || rx.recv())
        .await
        .map_err(|e| format!("Folder picker task failed: {e}"))?
        .map_err(|_| "Folder picker closed unexpectedly".to_string())?;
    Ok(choice.map(|fp| fp.to_string()))
}
