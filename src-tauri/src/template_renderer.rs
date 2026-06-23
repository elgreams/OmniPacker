use crate::template_metadata::TemplateMetadata;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Canonical OmniPacker credit line. Single source of truth on the Rust side;
/// the frontend (main.js) has a matching `OMNIPACKER_CREDIT` constant.
const OMNIPACKER_CREDIT: &str =
    "Made using [url=https://github.com/elgreams/OmniPacker]OmniPacker[/url]";

/// Template block types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TemplateBlock {
    Title { config: TitleConfig },
    Version { config: VersionConfig },
    DepotList { config: DepotListConfig },
    FreeText { config: FreeTextConfig },
    UploadedVersion { config: UploadedVersionConfig },
    AdvertiseOmnipacker {
        #[serde(default)]
        config: AdvertiseConfig,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TitleConfig {
    pub template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionConfig {
    pub template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepotListConfig {
    pub title: Option<String>,
    #[serde(rename = "lineTemplate")]
    pub line_template: String,
    #[serde(rename = "useCodeBlock")]
    pub use_code_block: Option<bool>,
    #[serde(rename = "maxDepots")]
    pub max_depots: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreeTextConfig {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadedVersionConfig {
    pub template: String,
}

/// Fixed-content "Advertise OmniPacker" block carries no user-editable fields.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdvertiseConfig {}

/// Template payload structure matching frontend format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplatePayload {
    pub version: u32,
    pub blocks: Vec<TemplateBlock>,
}

/// Renders a template string with metadata values
fn render_template_string(
    template: &str,
    values: &HashMap<String, String>,
) -> String {
    let mut result = template.to_string();

    // Replace {{field}} tokens with values
    for (key, value) in values {
        let token = format!("{{{{{}}}}}", key);
        result = result.replace(&token, value);
    }

    result
}

/// Gets the block type name for spacing logic
fn block_type_name(block: &TemplateBlock) -> &'static str {
    match block {
        TemplateBlock::Title { .. } => "title",
        TemplateBlock::Version { .. } => "version",
        TemplateBlock::DepotList { .. } => "depot_list",
        TemplateBlock::FreeText { .. } => "free_text",
        TemplateBlock::UploadedVersion { .. } => "uploaded_version",
        TemplateBlock::AdvertiseOmnipacker { .. } => "advertise_omnipacker",
    }
}

/// Renders a complete template with metadata
pub fn render_template(
    blocks: &[TemplateBlock],
    metadata: &TemplateMetadata,
) -> Result<String, String> {
    let mut output_parts: Vec<String> = Vec::new();

    // Build base values for single-field tokens
    let mut base_values = HashMap::new();
    base_values.insert("game_name".to_string(), metadata.game_name.clone());
    base_values.insert("os".to_string(), metadata.os.clone());
    base_values.insert("branch".to_string(), metadata.branch.clone());
    base_values.insert("build_datetime_utc".to_string(), metadata.build_datetime_utc.clone());
    base_values.insert("build_id".to_string(), metadata.build_id.clone());

    for block in blocks {
        let part = match block {
            TemplateBlock::Title { config } => {
                render_template_string(&config.template, &base_values)
            }

            TemplateBlock::Version { config } => {
                render_template_string(&config.template, &base_values)
            }

            TemplateBlock::UploadedVersion { config } => {
                render_template_string(&config.template, &base_values)
            }

            TemplateBlock::FreeText { config } => {
                render_template_string(&config.text, &base_values)
            }

            TemplateBlock::AdvertiseOmnipacker { .. } => OMNIPACKER_CREDIT.to_string(),

            TemplateBlock::DepotList { config } => {
                let max_depots = config.max_depots.unwrap_or(100);
                let depots_to_show = metadata.depots.iter().take(max_depots);

                let title = config.title.as_deref().unwrap_or("Depots");
                let use_code = config.use_code_block.unwrap_or(false);

                let mut lines = Vec::new();
                for depot in depots_to_show {
                    let mut depot_values = HashMap::new();
                    depot_values.insert("depot_id".to_string(), depot.depot_id.clone());
                    depot_values.insert("depot_name".to_string(), depot.depot_name.clone());
                    depot_values.insert("manifest_id".to_string(), depot.manifest_id.clone());

                    let rendered = render_template_string(&config.line_template, &depot_values);
                    lines.push(rendered);
                }

                // Build spoiler with optional code block
                let mut depot_output = format!("[spoiler={}]\n", title);
                if use_code {
                    depot_output.push_str("[code=text]");
                }
                depot_output.push_str(&lines.join("\n"));
                if use_code {
                    depot_output.push_str("[/code]");
                }
                depot_output.push_str("\n[/spoiler]");
                depot_output
            }
        };
        output_parts.push(part);
    }

    // Join parts with proper spacing (matching frontend logic)
    let mut output = String::new();
    for i in 0..output_parts.len() {
        output.push_str(&output_parts[i]);

        if i + 1 < output_parts.len() {
            let current_type = block_type_name(&blocks[i]);
            let next_type = block_type_name(&blocks[i + 1]);

            // Match CS.RIN-style spacing between default blocks
            let separator = if current_type == "version" && next_type == "depot_list" {
                "\n\n"
            } else if current_type == "depot_list" && next_type == "uploaded_version" {
                ""
            } else {
                "\n"
            };

            output.push_str(separator);
        }
    }

    Ok(output)
}

/// Creates default template blocks matching frontend defaults
pub fn create_default_template() -> Vec<TemplateBlock> {
    vec![
        TemplateBlock::Title {
            config: TitleConfig {
                template: "[url=][color=white][b]{{game_name}} [{{os}}] [Branch: {{branch}}] (Clean Steam Files)[/b][/color][/url]".to_string(),
            },
        },
        TemplateBlock::Version {
            config: VersionConfig {
                template: "[size=85][color=white][b]Version:[/b] [i]{{build_datetime_utc}} [Build {{build_id}}][/i][/color][/size]".to_string(),
            },
        },
        TemplateBlock::DepotList {
            config: DepotListConfig {
                title: Some("\"[color=white]Depots & Manifests[/color]\"".to_string()),
                line_template: "{{depot_id}} - {{depot_name}} [Manifest {{manifest_id}}]".to_string(),
                use_code_block: Some(true),
                max_depots: Some(100),
            },
        },
        TemplateBlock::UploadedVersion {
            config: UploadedVersionConfig {
                template: "[color=white][b]Uploaded version:[/b] [i]{{build_datetime_utc}} [Build {{build_id}}][/i][/color]".to_string(),
            },
        },
    ]
}

/// Derives the sibling `.txt` path for the template alongside an output.
///
/// This is purely string-based: it does NOT probe the filesystem. The output
/// path is either a packed directory name (e.g. `Game.Build.123.Win64.Public`)
/// or a `.7z` archive, and the template `.txt` always sits next to it.
///
/// We must not rely on `Path::is_dir()` (the directory may already be gone after
/// compression) nor on `Path::extension()` (the dotted folder name makes the
/// "extension" the branch token like `Public`, not `7z`). Both of those caused
/// "Output path must be a directory or .7z file" failures. Instead: strip a
/// trailing `.7z` if present, otherwise keep the whole name, then append `.txt`.
fn derive_template_txt_path(output_path: &PathBuf) -> Result<PathBuf, String> {
    let file_name = output_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Invalid output path")?;

    let base = file_name
        .strip_suffix(".7z")
        .or_else(|| file_name.strip_suffix(".7Z"))
        .unwrap_or(file_name);

    Ok(output_path.with_file_name(format!("{}.txt", base)))
}

/// Writes rendered template to a text file next to the output
pub fn write_template_file(
    output_path: &PathBuf,
    metadata: &TemplateMetadata,
    template_blocks: Option<&[TemplateBlock]>,
) -> Result<(), String> {
    // Use default template if none provided
    let default_blocks = create_default_template();
    let blocks = template_blocks.unwrap_or(&default_blocks);

    // Render template
    let rendered = render_template(blocks, metadata)?;

    // Determine output file path
    let txt_path = derive_template_txt_path(output_path)?;

    // Write file
    fs::write(&txt_path, rendered)
        .map_err(|e| format!("Failed to write template file: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template_metadata::TemplateDepot;

    #[test]
    fn test_render_template_string() {
        let mut values = HashMap::new();
        values.insert("game_name".to_string(), "Test Game".to_string());
        values.insert("os".to_string(), "Win64".to_string());

        let template = "Game: {{game_name}} OS: {{os}}";
        let result = render_template_string(template, &values);

        assert_eq!(result, "Game: Test Game OS: Win64");
    }

    #[test]
    fn test_render_template() {
        let metadata = TemplateMetadata {
            game_name: "Balatro".to_string(),
            os: "Win64".to_string(),
            branch: "Public".to_string(),
            build_datetime_utc: "February 24, 2025 - 22:02:36 UTC".to_string(),
            build_id: "18674832".to_string(),
            depots: vec![
                TemplateDepot {
                    depot_id: "2923300".to_string(),
                    depot_name: "Balatro Content".to_string(),
                    manifest_id: "4851806656204679952".to_string(),
                },
            ],
        };

        let blocks = vec![
            TemplateBlock::Title {
                config: TitleConfig {
                    template: "{{game_name}} [{{os}}]".to_string(),
                },
            },
            TemplateBlock::DepotList {
                config: DepotListConfig {
                    title: Some("Test Depots".to_string()),
                    line_template: "{{depot_name}}: {{manifest_id}}".to_string(),
                    use_code_block: Some(false),
                    max_depots: Some(100),
                },
            },
        ];

        let result = render_template(&blocks, &metadata).unwrap();
        assert!(result.contains("Balatro [Win64]"));
        assert!(result.contains("[spoiler=Test Depots]"));
        assert!(result.contains("Balatro Content: 4851806656204679952"));
    }

    #[test]
    fn test_advertise_omnipacker_renders_credit_line() {
        let metadata = TemplateMetadata {
            game_name: "Balatro".to_string(),
            os: "Win64".to_string(),
            branch: "Public".to_string(),
            build_datetime_utc: "February 24, 2025 - 22:02:36 UTC".to_string(),
            build_id: "18674832".to_string(),
            depots: vec![],
        };

        let blocks = vec![TemplateBlock::AdvertiseOmnipacker {
            config: AdvertiseConfig {},
        }];

        let result = render_template(&blocks, &metadata).unwrap();
        assert_eq!(result, OMNIPACKER_CREDIT);
    }

    #[test]
    fn test_default_template_does_not_include_credit() {
        // The credit line is no longer part of the built-in default; it is only
        // available as the selectable "Advertise OmniPacker" block.
        let metadata = TemplateMetadata {
            game_name: "Balatro".to_string(),
            os: "Win64".to_string(),
            branch: "Public".to_string(),
            build_datetime_utc: "February 24, 2025 - 22:02:36 UTC".to_string(),
            build_id: "18674832".to_string(),
            depots: vec![TemplateDepot {
                depot_id: "2923300".to_string(),
                depot_name: "Balatro Content".to_string(),
                manifest_id: "4851806656204679952".to_string(),
            }],
        };

        let rendered = render_template(&create_default_template(), &metadata).unwrap();
        assert!(!rendered.contains("OmniPacker"));
    }

    #[test]
    fn test_derive_txt_path_dotted_directory() {
        // The packed output folder name has dots; the trailing token (`Public`)
        // must NOT be treated as a file extension. Regression for
        // "Output path must be a directory or .7z file".
        let dir = PathBuf::from("/out/Garrys.Mod.Build.23150004.Win64.Public");
        let txt = derive_template_txt_path(&dir).unwrap();
        assert_eq!(
            txt,
            PathBuf::from("/out/Garrys.Mod.Build.23150004.Win64.Public.txt")
        );
    }

    #[test]
    fn test_derive_txt_path_7z_archive() {
        let archive = PathBuf::from("/out/Garrys.Mod.Build.23150004.Win64.Public.7z");
        let txt = derive_template_txt_path(&archive).unwrap();
        assert_eq!(
            txt,
            PathBuf::from("/out/Garrys.Mod.Build.23150004.Win64.Public.txt")
        );
    }

    #[test]
    fn test_derive_txt_path_7z_uppercase() {
        let archive = PathBuf::from("/out/Game.7Z");
        let txt = derive_template_txt_path(&archive).unwrap();
        assert_eq!(txt, PathBuf::from("/out/Game.txt"));
    }

    #[test]
    fn test_derive_txt_path_plain_name() {
        let dir = PathBuf::from("/out/Game");
        let txt = derive_template_txt_path(&dir).unwrap();
        assert_eq!(txt, PathBuf::from("/out/Game.txt"));
    }

    /// Creates a unique temporary directory for a test and returns its path.
    fn temp_dir(label: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        let unique = format!(
            "omnipacker_tmpl_{}_{}",
            label,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        dir.push(unique);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_metadata() -> TemplateMetadata {
        TemplateMetadata {
            game_name: "Balatro".to_string(),
            os: "Win64".to_string(),
            branch: "Public".to_string(),
            build_datetime_utc: "February 24, 2025 - 22:02:36 UTC".to_string(),
            build_id: "18674832".to_string(),
            depots: vec![TemplateDepot {
                depot_id: "2923300".to_string(),
                depot_name: "Balatro Content".to_string(),
                manifest_id: "4851806656204679952".to_string(),
            }],
        }
    }

    #[test]
    fn test_write_template_file_dotted_directory() {
        // End-to-end: the dotted output folder name that previously errored must
        // now produce a sibling .txt with rendered content actually on disk.
        let root = temp_dir("write_dir");
        let output_dir = root.join("Balatro.Build.18674832.Win64.Public");
        fs::create_dir_all(&output_dir).unwrap();

        write_template_file(&output_dir, &sample_metadata(), None).unwrap();

        let expected_txt = root.join("Balatro.Build.18674832.Win64.Public.txt");
        assert!(expected_txt.is_file(), "template .txt was not written");
        let contents = fs::read_to_string(&expected_txt).unwrap();
        assert!(contents.contains("Balatro"));
        assert!(contents.contains("4851806656204679952"));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn test_write_template_file_7z_archive() {
        // The .7z path: the directory has already been removed by compression,
        // so only the archive path exists. The .txt must still land beside it.
        let root = temp_dir("write_7z");
        let archive = root.join("Balatro.Build.18674832.Win64.Public.7z");

        write_template_file(&archive, &sample_metadata(), None).unwrap();

        let expected_txt = root.join("Balatro.Build.18674832.Win64.Public.txt");
        assert!(expected_txt.is_file(), "template .txt was not written");
        let contents = fs::read_to_string(&expected_txt).unwrap();
        assert!(contents.contains("Balatro"));

        fs::remove_dir_all(&root).ok();
    }
}
