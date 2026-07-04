use chrono::{Datelike, Timelike, Utc};
use serde::Serialize;
use std::sync::Mutex;
use tauri::State;

use crate::job_metadata::JobMetadataFile;
use crate::checksum::FileChecksum;

#[derive(Clone, Debug, Serialize)]
pub struct TemplateDepot {
    pub depot_id: String,
    pub depot_name: String,
    pub manifest_id: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct TemplateChecksum {
    pub file_name: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct TemplateMetadata {
    pub game_name: String,
    pub os: String,
    pub branch: String,
    pub build_datetime_utc: String,
    pub build_id: String,
    /// Steam App ID. Used by the crew preset for the header image and store URL.
    pub app_id: String,
    /// Short store description (from job.json). Empty when unavailable.
    pub game_description: String,
    /// Official website URL (from job.json), or empty when none.
    pub website: String,
    /// Uploader handle. Sourced from the global "Uploader name" setting, not
    /// job.json, so it is populated separately after construction (empty by
    /// default). Feeds the `{{username}}` token.
    pub username: String,
    /// Upload date. Sourced from the global "Upload date" setting (manual text
    /// or today's date), not job.json, so it is populated separately after
    /// construction (empty by default). Feeds the `{{upload_date}}` token.
    pub upload_date: String,
    /// Primary depot's ID. Scalar counterpart to the per-depot `{{depot_id}}`
    /// loop token, so single-field blocks (title/version/free text) can
    /// reference the main game depot. Empty when no primary depot is known.
    pub primary_depot_id: String,
    /// Primary depot's manifest ID. Scalar counterpart to the per-depot
    /// `{{manifest_id}}` loop token. Empty when the primary depot has no
    /// matching entry in `depots`.
    pub primary_manifest_id: String,
    pub depots: Vec<TemplateDepot>,
    /// For single-archive case. Empty when there are zero or multiple output
    /// files (use `checksums` then).
    pub sha256: String,
    /// One entry per resulting output file.
    pub checksums: Vec<TemplateChecksum>,
}

impl TemplateMetadata {
    pub fn from_job_metadata(metadata: &JobMetadataFile) -> Self {
        let timestamp = metadata
            .build_datetime_utc
            .unwrap_or(metadata.appinfo_fetched_at)
            .with_timezone(&Utc);
        let depots: Vec<TemplateDepot> = metadata
            .depots
            .iter()
            .map(|depot| TemplateDepot {
                depot_id: depot.depot_id.clone(),
                depot_name: depot.depot_name.clone(),
                manifest_id: depot.manifest_id.clone(),
            })
            .collect();

        // Resolve the primary depot's manifest from the depot list so the scalar
        // `{{primary_manifest_id}}` token has a value. Empty when the primary
        // depot isn't present (e.g. older job.json without a match).
        let primary_manifest_id = depots
            .iter()
            .find(|d| d.depot_id == metadata.primary_depot_id)
            .map(|d| d.manifest_id.clone())
            .unwrap_or_default();

        let month_name = month_name(timestamp.month());
        let build_datetime_utc = format!(
            "{} {}, {} - {:02}:{:02}:{:02} UTC",
            month_name,
            timestamp.day(),
            timestamp.year(),
            timestamp.hour(),
            timestamp.minute(),
            timestamp.second()
        );

        Self {
            game_name: metadata.game_name.clone(),
            os: map_platform_to_os(&metadata.platform),
            branch: metadata.branch.clone(),
            build_datetime_utc,
            build_id: metadata.build_id.clone(),
            app_id: metadata.appid.clone(),
            game_description: metadata.game_description.clone(),
            website: metadata.website.clone().unwrap_or_default(),
            // The uploader handle is injected later via set_uploader; default to
            // empty so the `{{username}}` token renders blank when unset.
            username: String::new(),
            // The upload date is injected later via set_upload_date; default to
            // empty so the `{{upload_date}}` token renders blank when unset.
            upload_date: String::new(),
            primary_depot_id: metadata.primary_depot_id.clone(),
            primary_manifest_id,
            depots,
            sha256: String::new(),
            checksums: vec![],
        }
    }

    /// Applies the global uploader handle (from the "Uploader name" setting),
    /// which originates from the frontend rather than job.json.
    pub fn set_uploader(&mut self, username: String) {
        self.username = username;
    }

    /// Applies the global upload date (from the "Upload date" setting), which
    /// originates from the frontend rather than job.json.
    pub fn set_upload_date(&mut self, upload_date: String) {
        self.upload_date = upload_date;
    }

    /// Applies the computed output checksums, populated after compression.
    pub fn set_checksums(&mut self, checksums: Vec<FileChecksum>) {
        self.checksums = checksums
            .iter()
            .map(|c| TemplateChecksum {
                file_name: c.file_name.clone(),
                sha256: c.sha256.clone(),
            })
            .collect();
        self.sha256 = match self.checksums.as_slice() {
            [only] => only.sha256.clone(),
            _ => String::new(),
        };
    }
}

#[derive(Default)]
pub struct TemplateMetadataState {
    inner: Mutex<Option<TemplateMetadata>>,
}

impl TemplateMetadataState {
    pub fn set(&self, metadata: TemplateMetadata) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = Some(metadata);
        }
    }

    pub fn get(&self) -> Option<TemplateMetadata> {
        self.inner.lock().ok().and_then(|guard| guard.clone())
    }
}

#[tauri::command]
pub fn get_template_metadata(
    state: State<'_, TemplateMetadataState>,
) -> Result<Option<TemplateMetadata>, String> {
    Ok(state.get())
}

fn map_platform_to_os(platform: &str) -> String {
    match platform {
        "Win64" | "Win32" | "Linux64" | "MacOS64" | "MacOSArm64" => {
            platform.to_string()
        }
        _ => platform.to_string(),
    }
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job_metadata::{BuildIdSource, DepotInfo, JobMetadataFile};

    #[test]
    fn primary_tokens_resolve_to_primary_depot() {
        // from_job_metadata must carry the primary depot id and find its manifest
        // in the depot list, even when the primary isn't the first depot.
        let job = JobMetadataFile::new(
            "job".to_string(),
            "2379780".to_string(),
            "public".to_string(),
            "Win64".to_string(),
            "2379781".to_string(),
            "Balatro".to_string(),
            "18674832".to_string(),
            BuildIdSource::AppBuildid,
            None,
            vec![
                DepotInfo {
                    depot_id: "228989".to_string(),
                    depot_name: "Steamworks Common Redistributables".to_string(),
                    manifest_id: "7206221393165260579".to_string(),
                    manifest_id_used: None,
                    dlcappid: None,
                },
                DepotInfo {
                    depot_id: "2379781".to_string(),
                    depot_name: "Balatro".to_string(),
                    manifest_id: "4851806656204679952".to_string(),
                    manifest_id_used: None,
                    dlcappid: None,
                },
            ],
        );

        let meta = TemplateMetadata::from_job_metadata(&job);
        assert_eq!(meta.primary_depot_id, "2379781");
        assert_eq!(meta.primary_manifest_id, "4851806656204679952");
    }

    #[test]
    fn primary_manifest_empty_when_primary_absent_from_depots() {
        // If the primary depot id has no matching depot entry, the manifest token
        // resolves to empty rather than picking an unrelated depot.
        let job = JobMetadataFile::new(
            "job".to_string(),
            "2379780".to_string(),
            "public".to_string(),
            "Win64".to_string(),
            "999999".to_string(),
            "Balatro".to_string(),
            "18674832".to_string(),
            BuildIdSource::AppBuildid,
            None,
            vec![DepotInfo {
                depot_id: "2379781".to_string(),
                depot_name: "Balatro".to_string(),
                manifest_id: "4851806656204679952".to_string(),
                manifest_id_used: None,
                dlcappid: None,
            }],
        );

        let meta = TemplateMetadata::from_job_metadata(&job);
        assert_eq!(meta.primary_depot_id, "999999");
        assert_eq!(meta.primary_manifest_id, "");
    }
}
