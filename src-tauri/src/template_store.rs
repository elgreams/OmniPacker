use crate::template_renderer::TemplatePayload;
use serde_json;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// Gets the path to the template data file
fn get_template_path(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data directory: {}", e))?;
    Ok(app_data_dir.join("template.json"))
}

/// Saves template data to disk
#[tauri::command]
pub fn save_template_data(
    app_handle: AppHandle,
    template_payload: TemplatePayload,
) -> Result<(), String> {
    let path = get_template_path(&app_handle)?;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create app data directory: {}", e))?;
    }

    let json = serde_json::to_string_pretty(&template_payload)
        .map_err(|e| format!("Failed to serialize template: {}", e))?;

    fs::write(&path, json).map_err(|e| format!("Failed to write template file: {}", e))?;

    Ok(())
}

/// Loads template data from disk
#[tauri::command]
pub fn load_template_data(app_handle: AppHandle) -> Result<Option<TemplatePayload>, String> {
    let path = get_template_path(&app_handle)?;

    if !path.exists() {
        return Ok(None);
    }

    let json = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read template file: {}", e))?;

    let payload: TemplatePayload = serde_json::from_str(&json)
        .map_err(|e| format!("Failed to parse template file: {}", e))?;

    Ok(Some(payload))
}

/// Loads template data for internal use (not a command)
pub fn load_template_data_internal(app_handle: &AppHandle) -> Option<TemplatePayload> {
    load_template_data(app_handle.clone()).ok().flatten()
}
