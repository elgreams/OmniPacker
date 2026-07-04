use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FileChecksum {
    pub file_name: String,
    pub sha256: String,
}

pub fn calculate_sha256(path: &Path) -> Result<String, String> {
    let file = File::open(path)
        .map_err(|e| format!("Failed to open {} for hashing: {}", path.display(), e))?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 1024 * 1024];

    loop {
        let read = reader
            .read(&mut buf)
            .map_err(|e| format!("Failed to read {} while hashing: {}", path.display(), e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }

    let digest = hasher.finalize();
    Ok(digest.iter().map(|b| format!("{:02x}", b)).collect())
}

fn find_archive_volumes(archive_path: &Path) -> Vec<PathBuf> {
    if archive_path.is_file() {
        return vec![archive_path.to_path_buf()];
    }

    let (Some(parent), Some(base_name)) = (
        archive_path.parent(),
        archive_path.file_name().and_then(|n| n.to_str()),
    ) else {
        return Vec::new();
    };

    let Ok(entries) = std::fs::read_dir(parent) else {
        return Vec::new();
    };

    let prefix = format!("{}.", base_name);
    let mut found: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .and_then(|name| name.strip_prefix(&prefix))
                .map(|suffix| suffix.len() >= 3 && suffix.chars().all(|c| c.is_ascii_digit()))
                .unwrap_or(false)
        })
        .collect();

    found.sort();
    found
}

pub fn calculate_checksums(output_path: &Path) -> Result<Vec<FileChecksum>, String> {
    if output_path.is_dir() {
        return Ok(Vec::new());
    }

    find_archive_volumes(output_path)
        .into_iter()
        .map(|volume| {
            let sha256 = calculate_sha256(&volume)?;
            let file_name = volume
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            Ok(FileChecksum { file_name, sha256 })
        })
        .collect()
}
