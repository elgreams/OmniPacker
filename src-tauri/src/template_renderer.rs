use crate::template_metadata::TemplateMetadata;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Template block types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TemplateBlock {
    Title { config: TitleConfig },
    Version { config: VersionConfig },
    DepotList { config: DepotListConfig },
    FreeText { config: FreeTextConfig },
    UploadedVersion { config: UploadedVersionConfig },
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
        TemplateBlock::FreeText {
            config: FreeTextConfig {
                text: "Made using [url=https://github.com/elgreams/OmniPacker]OmniPacker[/url]".to_string(),
            },
        },
    ]
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
    let txt_path = if output_path.is_dir() {
        // If it's a directory, use the directory name
        let dir_name = output_path
            .file_name()
            .ok_or("Invalid output path")?
            .to_string_lossy();
        output_path.with_file_name(format!("{}.txt", dir_name))
    } else if output_path.extension().and_then(|e| e.to_str()) == Some("7z") {
        // If it's a .7z file, replace extension with .txt
        output_path.with_extension("txt")
    } else {
        return Err("Output path must be a directory or .7z file".to_string());
    };

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
}
