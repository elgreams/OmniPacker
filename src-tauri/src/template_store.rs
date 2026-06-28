use crate::template_renderer::TemplatePayload;
use serde::Serialize;
use serde_json;
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

/// Reserved built-in profile names. These are synthesized in the frontend and
/// must never be saved to or deleted from disk, so the library always offers a
/// clean `standard`/`crew` regardless of what the user has saved.
const RESERVED_NAMES: [&str; 2] = ["standard", "crew"];

/// A saved template profile as returned to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct StoredProfile {
    pub name: String,
    pub payload: TemplatePayload,
}

/// Resolves the directory that holds saved profile JSON files. Portable-aware:
/// sits next to the exe in portable mode, in app-data otherwise.
fn get_profiles_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    Ok(crate::output_dir::resolve_config_dir(app_handle)?.join("profiles"))
}

/// Sanitizes a profile name into a safe filename stem. Keeps alphanumerics,
/// dash, and underscore; collapses everything else to `_`. Matches the
/// frontend's `sanitizeProfileFilename` so a name maps to the same file on
/// both sides.
fn sanitize_filename(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "profile".to_string()
    } else {
        trimmed
    }
}

fn is_reserved(name: &str) -> bool {
    RESERVED_NAMES
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(name.trim()))
}

/// Lists all saved profiles, sorted by name. Built-ins are NOT included here;
/// the frontend prepends them.
#[tauri::command]
pub fn list_profiles(app_handle: AppHandle) -> Result<Vec<StoredProfile>, String> {
    let dir = get_profiles_dir(&app_handle)?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut profiles = Vec::new();
    let entries =
        fs::read_dir(&dir).map_err(|e| format!("Failed to read profiles directory: {}", e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let json = match fs::read_to_string(&path) {
            Ok(json) => json,
            Err(_) => continue,
        };
        let payload: TemplatePayload = match serde_json::from_str(&json) {
            Ok(payload) => payload,
            Err(_) => continue,
        };
        // The on-disk filename stem is the source of truth for the name.
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        profiles.push(StoredProfile { name, payload });
    }

    profiles.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(profiles)
}

/// Saves a profile to `profiles/<sanitized-name>.json`. Rejects the reserved
/// built-in names so they cannot be shadowed on disk.
#[tauri::command]
pub fn save_profile(
    app_handle: AppHandle,
    name: String,
    template_payload: TemplatePayload,
) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Profile name cannot be empty.".to_string());
    }
    if is_reserved(trimmed) {
        return Err(format!(
            "\"{}\" is a reserved built-in profile. Choose a different name.",
            trimmed
        ));
    }

    let dir = get_profiles_dir(&app_handle)?;
    fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create profiles directory: {}", e))?;

    let file_stem = sanitize_filename(trimmed);
    let path = dir.join(format!("{}.json", file_stem));

    let json = serde_json::to_string_pretty(&template_payload)
        .map_err(|e| format!("Failed to serialize profile: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write profile file: {}", e))?;

    // Return the canonical (filename) name so the frontend can select it.
    Ok(file_stem)
}

/// Deletes a saved profile by name. Reserved built-ins cannot be deleted.
#[tauri::command]
pub fn delete_profile(app_handle: AppHandle, name: String) -> Result<(), String> {
    let trimmed = name.trim();
    if is_reserved(trimmed) {
        return Err("Built-in profiles cannot be deleted.".to_string());
    }

    let dir = get_profiles_dir(&app_handle)?;
    let file_stem = sanitize_filename(trimmed);
    let path = dir.join(format!("{}.json", file_stem));
    if path.exists() {
        fs::remove_file(&path)
            .map_err(|e| format!("Failed to delete profile file: {}", e))?;
    }
    Ok(())
}

/// Opens the profiles directory in the system file manager, creating it first
/// if no profile has been saved yet.
#[tauri::command]
pub fn open_profiles_folder(app_handle: AppHandle) -> Result<(), String> {
    let dir = get_profiles_dir(&app_handle)?;
    fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create profiles directory: {}", e))?;
    app_handle
        .opener()
        .open_path(dir.to_string_lossy().to_string(), None::<String>)
        .map_err(|e| format!("Failed to open profiles folder: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename_basic() {
        assert_eq!(sanitize_filename("My Crew!"), "My_Crew");
        assert_eq!(sanitize_filename("standard"), "standard");
        assert_eq!(sanitize_filename("a/b\\c"), "a_b_c");
    }

    #[test]
    fn test_sanitize_filename_empty_fallback() {
        assert_eq!(sanitize_filename("***"), "profile");
        assert_eq!(sanitize_filename("   "), "profile");
    }

    #[test]
    fn test_is_reserved_case_insensitive() {
        assert!(is_reserved("standard"));
        assert!(is_reserved("Crew"));
        assert!(is_reserved("  STANDARD "));
        assert!(!is_reserved("mycustom"));
    }
}
