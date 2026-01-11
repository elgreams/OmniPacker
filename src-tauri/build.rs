use std::env;

fn main() {
    println!("cargo:rerun-if-changed=tauri.conf.json");
    println!("cargo:rerun-if-changed=binaries");

    // Determine the target platform subdirectory
    let target = env::var("TARGET").unwrap_or_default();
    let platform_subdir = match target.as_str() {
        "x86_64-pc-windows-msvc" => "win-x64",
        "aarch64-pc-windows-msvc" => "win-arm64",
        "x86_64-unknown-linux-gnu" => "linux-x64",
        "aarch64-unknown-linux-gnu" => "linux-arm64",
        "arm-unknown-linux-gnueabihf" => "linux-arm",
        "x86_64-apple-darwin" => "macos-x64",
        "aarch64-apple-darwin" => "macos-arm64",
        _ => {
            // Fallback detection
            if target.contains("x86_64") && target.contains("windows") {
                "win-x64"
            } else if target.contains("aarch64") && target.contains("windows") {
                "win-arm64"
            } else if target.contains("x86_64") && target.contains("linux") {
                "linux-x64"
            } else if target.contains("aarch64") && target.contains("linux") {
                "linux-arm64"
            } else if target.contains("arm") && target.contains("linux") {
                "linux-arm"
            } else if target.contains("x86_64") && target.contains("darwin") {
                "macos-x64"
            } else if target.contains("aarch64") && target.contains("darwin") {
                "macos-arm64"
            } else {
                "unknown"
            }
        }
    };

    println!("cargo:rustc-env=OMNIPACKER_PLATFORM_SUBDIR={}", platform_subdir);

    tauri_build::build()
}
