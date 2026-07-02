use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const LOGIN_FILE_NAME: &str = "login.dat";
const COMPRESSION_PASSWORD_FILE_NAME: &str = "compression.dat";
const LOGIN_PREFIX: &str = "OP1:";
const XOR_KEY: &[u8] = b"omnipacker-login-key";

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginData {
    pub username: String,
    pub password: String,
}

fn login_file_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    // Portable-aware: in portable mode the file lives next to the exe so saved
    // credentials travel with the app and are never left on the host machine.
    Ok(crate::output_dir::resolve_config_dir(app_handle)?.join(LOGIN_FILE_NAME))
}

fn ensure_parent_dir(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            format!(
                "Failed to create login data directory {}: {}",
                parent.display(),
                e
            )
        })?;
    }
    Ok(())
}

fn xor_bytes(data: &[u8]) -> Vec<u8> {
    data.iter()
        .enumerate()
        .map(|(index, byte)| byte ^ XOR_KEY[index % XOR_KEY.len()])
        .collect()
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(out, "{:02x}", byte);
    }
    out
}

fn decode_hex(input: &str) -> Result<Vec<u8>, String> {
    if input.len() % 2 != 0 {
        return Err("Invalid hex payload length.".to_string());
    }
    let mut out = Vec::with_capacity(input.len() / 2);
    let mut index = 0;
    while index < input.len() {
        let byte = u8::from_str_radix(&input[index..index + 2], 16)
            .map_err(|e| format!("Invalid hex payload at {}: {}", index, e))?;
        out.push(byte);
        index += 2;
    }
    Ok(out)
}

fn encrypt_payload(plain_text: &str) -> String {
    let masked = xor_bytes(plain_text.as_bytes());
    format!("{}{}", LOGIN_PREFIX, encode_hex(&masked))
}

fn decrypt_payload(payload: &str) -> Result<String, String> {
    let trimmed = payload.trim();
    let hex = trimmed
        .strip_prefix(LOGIN_PREFIX)
        .ok_or_else(|| "Unsupported login data format.".to_string())?;
    let bytes = decode_hex(hex)?;
    let unmasked = xor_bytes(&bytes);
    String::from_utf8(unmasked).map_err(|e| format!("Invalid login data: {}", e))
}

#[tauri::command]
pub fn save_login_data(
    app_handle: tauri::AppHandle,
    username: String,
    password: String,
) -> Result<(), String> {
    if username.trim().is_empty() || password.is_empty() {
        return Err("Username and password are required.".to_string());
    }

    let login_data = LoginData { username, password };
    let json = serde_json::to_string(&login_data)
        .map_err(|e| format!("Failed to encode login data: {}", e))?;
    let encoded = encrypt_payload(&json);
    let path = login_file_path(&app_handle)?;
    ensure_parent_dir(&path)?;
    std::fs::write(&path, encoded)
        .map_err(|e| format!("Failed to write login data to {}: {}", path.display(), e))?;
    Ok(())
}

#[tauri::command]
pub fn load_login_data(app_handle: tauri::AppHandle) -> Result<Option<LoginData>, String> {
    let path = login_file_path(&app_handle)?;
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read login data from {}: {}", path.display(), e))?;
    let json = decrypt_payload(&content)?;
    let login_data =
        serde_json::from_str(&json).map_err(|e| format!("Failed to parse login data: {}", e))?;
    Ok(Some(login_data))
}

#[tauri::command]
pub fn delete_login_data(app_handle: tauri::AppHandle) -> Result<(), String> {
    let path = login_file_path(&app_handle)?;
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("Failed to delete login data at {}: {}", path.display(), e))?;
    }
    Ok(())
}

// --- Compression password store ---
//
// Same portable-aware location and OP1 payload format as login.dat. Kept out
// of the webview's localStorage so a portable install leaves nothing on the
// host machine and the password doesn't sit at a well-known localStorage key.

fn compression_password_file_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(crate::output_dir::resolve_config_dir(app_handle)?.join(COMPRESSION_PASSWORD_FILE_NAME))
}

/// Saves the compression password. An empty password deletes the stored file
/// so disabling the feature leaves nothing behind.
#[tauri::command]
pub fn save_compression_password(
    app_handle: tauri::AppHandle,
    password: String,
) -> Result<(), String> {
    let path = compression_password_file_path(&app_handle)?;
    if password.is_empty() {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| {
                format!(
                    "Failed to delete compression password at {}: {}",
                    path.display(),
                    e
                )
            })?;
        }
        return Ok(());
    }
    let encoded = encrypt_payload(&password);
    ensure_parent_dir(&path)?;
    std::fs::write(&path, encoded).map_err(|e| {
        format!(
            "Failed to write compression password to {}: {}",
            path.display(),
            e
        )
    })?;
    Ok(())
}

#[tauri::command]
pub fn load_compression_password(app_handle: tauri::AppHandle) -> Result<String, String> {
    let path = compression_password_file_path(&app_handle)?;
    if !path.exists() {
        return Ok(String::new());
    }
    let content = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "Failed to read compression password from {}: {}",
            path.display(),
            e
        )
    })?;
    decrypt_payload(&content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_roundtrips_arbitrary_text() {
        let original = "hunter2 with spaces + unicode 密码";
        let encoded = encrypt_payload(original);
        assert!(encoded.starts_with(LOGIN_PREFIX));
        assert!(!encoded.contains("hunter2"));
        assert_eq!(decrypt_payload(&encoded).unwrap(), original);
    }

    #[test]
    fn decrypt_rejects_unknown_format() {
        assert!(decrypt_payload("plaintext-password").is_err());
        assert!(decrypt_payload("OP1:zz-not-hex").is_err());
    }
}
