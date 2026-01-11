use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Current metadata schema version
pub const METADATA_VERSION: &str = "1.0.0";

/// Source of the build ID
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BuildIdSource {
    /// Build ID from app-level appinfo (preferred)
    AppBuildid,
    /// Fallback: manifest ID of the primary depot
    PrimaryManifestId,
}

/// Information about a single depot
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DepotInfo {
    /// Steam depot ID
    pub depot_id: String,
    /// Human-readable depot name (from appinfo or fallback)
    pub depot_name: String,
    /// Manifest ID for this depot
    pub manifest_id: String,
    /// The manifest ID actually used during download (if different/discoverable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_id_used: Option<String>,
}

/// Job metadata written to job.json in the staging directory
///
/// This file exists for:
/// - Determinism
/// - Debugging
/// - Crash safety
/// - Reproducible finalization
///
/// NOT for resume support.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobMetadataFile {
    /// Unique job identifier (matches staging directory name)
    pub job_id: String,
    /// Steam App ID
    pub appid: String,
    /// Branch name (default: "public")
    pub branch: String,
    /// Target platform (for naming only; does NOT filter depots)
    pub platform: String,
    /// Primary depot ID (explicit, never inferred heuristically)
    pub primary_depot_id: String,
    /// Human-readable game name from Steam
    pub game_name: String,
    /// Build ID (SteamDB-compatible when possible)
    pub build_id: String,
    /// Source of the build ID
    pub build_id_source: BuildIdSource,
    /// Build release datetime (UTC) if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_datetime_utc: Option<DateTime<Utc>>,
    /// List of depots included in this job
    pub depots: Vec<DepotInfo>,
    /// Timestamp when appinfo was fetched
    pub appinfo_fetched_at: DateTime<Utc>,
    /// Metadata schema version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_version: Option<String>,
}

impl JobMetadataFile {
    /// Creates a new JobMetadataFile with required fields
    pub fn new(
        job_id: String,
        appid: String,
        branch: String,
        platform: String,
        primary_depot_id: String,
        game_name: String,
        build_id: String,
        build_id_source: BuildIdSource,
        build_datetime_utc: Option<DateTime<Utc>>,
        depots: Vec<DepotInfo>,
    ) -> Self {
        Self {
            job_id,
            appid,
            branch,
            platform,
            primary_depot_id,
            game_name,
            build_id,
            build_id_source,
            build_datetime_utc,
            depots,
            appinfo_fetched_at: Utc::now(),
            metadata_version: Some(METADATA_VERSION.to_string()),
        }
    }

    /// Writes the job metadata to job.json in the specified directory
    pub fn write_to_dir(&self, staging_dir: &Path) -> Result<(), String> {
        let job_json_path = staging_dir.join("job.json");

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize job metadata: {}", e))?;

        fs::write(&job_json_path, json)
            .map_err(|e| format!("Failed to write job.json to {}: {}", job_json_path.display(), e))?;

        Ok(())
    }

    /// Reads job metadata from job.json in the specified directory
    pub fn read_from_dir(staging_dir: &Path) -> Result<Self, String> {
        let job_json_path = staging_dir.join("job.json");

        let content = fs::read_to_string(&job_json_path)
            .map_err(|e| format!("Failed to read job.json from {}: {}", job_json_path.display(), e))?;

        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse job.json: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_metadata_serialization() {
        let metadata = JobMetadataFile::new(
            "2026-01-05T11-30-02Z_a1b2c3".to_string(),
            "123456".to_string(),
            "public".to_string(),
            "Linux".to_string(),
            "123457".to_string(),
            "Test Game".to_string(),
            "18674832".to_string(),
            BuildIdSource::AppBuildid,
            None,
            vec![
                DepotInfo {
                    depot_id: "123457".to_string(),
                    depot_name: "Test Game Content".to_string(),
                    manifest_id: "9876543210".to_string(),
                    manifest_id_used: None,
                },
            ],
        );

        let json = serde_json::to_string_pretty(&metadata).unwrap();
        assert!(json.contains("\"job_id\": \"2026-01-05T11-30-02Z_a1b2c3\""));
        assert!(json.contains("\"build_id_source\": \"app_buildid\""));
    }

    #[test]
    fn test_build_id_source_serialization() {
        assert_eq!(
            serde_json::to_string(&BuildIdSource::AppBuildid).unwrap(),
            "\"app_buildid\""
        );
        assert_eq!(
            serde_json::to_string(&BuildIdSource::PrimaryManifestId).unwrap(),
            "\"primary_manifest_id\""
        );
    }
}
