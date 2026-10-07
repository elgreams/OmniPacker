use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use tauri::AppHandle;

use crate::output_dir::resolve_scratch_dir;

/// Generates a unique job ID in the format: <ISO8601_UTC_timestamp>_<short_unique_id>
/// Example: 2026-01-05T11-30-02Z_a1b2c3
pub fn generate_job_id() -> String {
    let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%SZ").to_string();
    let unique_id = generate_short_id();
    format!("{}_{}", timestamp, unique_id)
}

/// Generates a short 6-character alphanumeric ID
fn generate_short_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    // Use process ID and nanoseconds for uniqueness
    let seed = nanos ^ (std::process::id() as u128);

    // Convert to base36 (0-9, a-z) and take last 6 chars
    let chars: Vec<char> = "0123456789abcdefghijklmnopqrstuvwxyz".chars().collect();
    let mut result = String::new();
    let mut n = seed;

    for _ in 0..6 {
        result.push(chars[(n % 36) as usize]);
        n /= 36;
    }

    result
}

/// Resolves the staging directory path for a job
/// Returns: <scratch>/staging/<job_id>/
pub fn resolve_staging_dir(app_handle: &AppHandle, job_id: &str) -> Result<PathBuf, String> {
    let scratch_dir = resolve_scratch_dir(app_handle)?;
    Ok(scratch_dir.join("staging").join(job_id))
}

/// Creates the staging directory for a job
/// Returns the path to the created staging directory
pub fn create_staging_dir(app_handle: &AppHandle, job_id: &str) -> Result<PathBuf, String> {
    let staging_dir = resolve_staging_dir(app_handle, job_id)?;

    if staging_dir.exists() {
        return Err(format!(
            "Staging directory already exists: {}. This should never happen with unique job IDs.",
            staging_dir.display()
        ));
    }

    fs::create_dir_all(&staging_dir).map_err(|err| {
        format!(
            "Failed to create staging directory {}: {}",
            staging_dir.display(),
            err
        )
    })?;

    Ok(staging_dir)
}

/// Deletes the staging directory for a job (used on failure)
pub fn cleanup_staging_dir(app_handle: &AppHandle, job_id: &str) -> Result<(), String> {
    let staging_dir = resolve_staging_dir(app_handle, job_id)?;

    if !staging_dir.exists() {
        // Already cleaned up or never created
        return Ok(());
    }

    fs::remove_dir_all(&staging_dir).map_err(|err| {
        format!(
            "Failed to cleanup staging directory {}: {}",
            staging_dir.display(),
            err
        )
    })?;

    Ok(())
}

/// Deletes any orphaned staging directories left behind by interrupted runs.
pub fn cleanup_orphaned_staging(app_handle: &AppHandle) -> Result<usize, String> {
    let scratch_dir = resolve_scratch_dir(app_handle)?;
    // Half-assembled outputs (`.tmp_<job_id>`) from a run that crashed or was
    // force-closed mid-finalization. They hold a full copy of the game and
    // nothing else ever removes them. Safe to delete at startup because the
    // single-instance guard means no other job can be using them.
    let mut removed = remove_orphaned_temp_outputs(&scratch_dir);

    let staging_root = scratch_dir.join("staging");

    if !staging_root.exists() {
        return Ok(removed);
    }

    let entries = fs::read_dir(&staging_root).map_err(|err| {
        format!(
            "Failed to read staging directory {}: {}",
            staging_root.display(),
            err
        )
    })?;

    let mut errors = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                errors.push(format!("Failed to read staging entry: {err}"));
                continue;
            }
        };
        let path = entry.path();
        if path.is_dir() {
            match fs::remove_dir_all(&path) {
                Ok(()) => removed += 1,
                Err(err) => errors.push(format!(
                    "Failed to remove staging directory {}: {}",
                    path.display(),
                    err
                )),
            }
        } else if path.is_file() {
            match fs::remove_file(&path) {
                Ok(()) => removed += 1,
                Err(err) => errors.push(format!(
                    "Failed to remove staging file {}: {}",
                    path.display(),
                    err
                )),
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors.join(" | "));
    }
    Ok(removed)
}

/// Deletes `.tmp_<job_id>` assembly dirs directly under `scratch_dir`.
/// Matches the exact name shape finalization uses (job IDs start with a
/// four-digit year), so unrelated folders in a custom output dir are never
/// touched. Returns how many were removed; failures are skipped.
fn remove_orphaned_temp_outputs(scratch_dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(scratch_dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter(|e| {
            e.file_name()
                .to_str()
                .and_then(|n| n.strip_prefix(".tmp_"))
                .is_some_and(|id| id.len() > 4 && id[..4].chars().all(|c| c.is_ascii_digit()))
        })
        .filter(|e| fs::remove_dir_all(e.path()).is_ok())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orphaned_temp_outputs_are_removed_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("omnipacker_tmpsweep_{}", generate_short_id()));
        for name in [".tmp_2026-10-07T12-00-00Z_abc123", ".tmp_notajob", "Game.Build.1.Win64.Public", "staging"] {
            fs::create_dir_all(dir.join(name).join("inner")).unwrap();
        }
        fs::write(dir.join(".tmp_2026-file"), b"x").unwrap(); // a file, not a dir
        assert_eq!(remove_orphaned_temp_outputs(&dir), 1);
        assert!(!dir.join(".tmp_2026-10-07T12-00-00Z_abc123").exists());
        assert!(dir.join(".tmp_notajob").exists());
        assert!(dir.join("Game.Build.1.Win64.Public").exists());
        assert!(dir.join("staging").exists());
        assert!(dir.join(".tmp_2026-file").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_generate_job_id_format() {
        let job_id = generate_job_id();

        // Should contain underscore separator
        assert!(job_id.contains('_'), "Job ID should contain underscore");

        // Should have timestamp part (YYYY-MM-DDTHH-MM-SSZ format)
        let parts: Vec<&str> = job_id.split('_').collect();
        assert_eq!(
            parts.len(),
            2,
            "Job ID should have timestamp and unique parts"
        );

        // Timestamp should be 20 chars (YYYY-MM-DDTHH-MM-SSZ)
        assert_eq!(parts[0].len(), 20, "Timestamp should be 20 characters");

        // Unique ID should be 6 chars
        assert_eq!(parts[1].len(), 6, "Unique ID should be 6 characters");
    }

    #[test]
    fn test_generate_short_id_uniqueness() {
        let id1 = generate_short_id();
        // Small delay to ensure different nanosecond values
        std::thread::sleep(std::time::Duration::from_millis(1));
        let id2 = generate_short_id();

        // IDs should be different (high probability)
        assert_ne!(id1, id2, "Generated IDs should be unique");
    }
}
