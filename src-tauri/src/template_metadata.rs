use chrono::{Datelike, Timelike, Utc};
use serde::Serialize;
use std::sync::Mutex;
use tauri::State;

use crate::job_metadata::JobMetadataFile;

#[derive(Clone, Debug, Serialize)]
pub struct TemplateDepot {
    pub depot_id: String,
    pub depot_name: String,
    pub manifest_id: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct TemplateMetadata {
    pub game_name: String,
    pub os: String,
    pub branch: String,
    pub build_datetime_utc: String,
    pub build_id: String,
    pub depots: Vec<TemplateDepot>,
}

impl TemplateMetadata {
    pub fn from_job_metadata(metadata: &JobMetadataFile) -> Self {
        let timestamp = metadata
            .build_datetime_utc
            .unwrap_or(metadata.appinfo_fetched_at)
            .with_timezone(&Utc);
        let depots = metadata
            .depots
            .iter()
            .map(|depot| TemplateDepot {
                depot_id: depot.depot_id.clone(),
                depot_name: depot.depot_name.clone(),
                manifest_id: depot.manifest_id.clone(),
            })
            .collect();

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
            depots,
        }
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
