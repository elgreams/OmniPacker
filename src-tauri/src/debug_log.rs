use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;

use crate::output_dir::resolve_downloads_dir;

/// Resolves the logs directory as a sibling of the downloads directory.
fn resolve_logs_dir(app_handle: &AppHandle) -> Option<PathBuf> {
    let downloads = resolve_downloads_dir(app_handle).ok()?;
    let logs_dir = downloads.parent()?.join("logs");
    fs::create_dir_all(&logs_dir).ok()?;
    Some(logs_dir)
}

/// Returns a path for a timestamped log file in the logs/ directory.
pub fn resolve_log_path(app_handle: &AppHandle, name: &str) -> Option<PathBuf> {
    let dir = resolve_logs_dir(app_handle)?;
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    Some(dir.join(format!("{name}-{timestamp}.log")))
}
