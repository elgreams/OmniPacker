# OmniPacker Build Guide

## Overview

OmniPacker uses platform-specific build scripts to ensure each build only includes the binaries needed for its target platform.

## Binary Organization

Binaries are organized by platform in `src-tauri/binaries/`:

```
src-tauri/binaries/
├── win-x64/          # Windows x86_64
├── win-arm64/        # Windows ARM64
├── linux-x64/        # Linux x86_64
├── linux-arm64/      # Linux ARM64
├── linux-arm/        # Linux ARM32
├── macos-x64/        # macOS x86_64 (Intel)
└── macos-arm64/      # macOS ARM64 (Apple Silicon)
```

**IMPORTANT**: Never move, rename, or delete files from these directories manually.

## Development Builds

For local development, use the standard command:

```bash
npm run tauri dev
```

This will use all platform binaries for development but only load the correct ones at runtime based on your platform.

## Production Builds

### Automated CI/CD (Recommended)

The GitHub Actions workflow (`.github/workflows/build.yml`) automatically:
1. Builds for all supported platforms using dedicated build scripts
2. Creates platform-specific artifacts
3. Creates GitHub releases on version tags

**To trigger a release:**
```bash
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0
```

### Local Platform-Specific Builds

Each platform has a dedicated build script that handles binary management automatically.

#### Linux

```bash
# Linux x64
./scripts/build-linux-x64.sh

# Linux ARM64
./scripts/build-linux-arm64.sh

# Linux ARM
./scripts/build-linux-arm.sh
```

#### macOS

```bash
# macOS Intel (x64)
./scripts/build-macos-x64.sh

# macOS Apple Silicon (ARM64)
./scripts/build-macos-arm64.sh
```

#### Windows

```powershell
# Windows x64
.\scripts\build-windows-x64.ps1

# Windows ARM64
.\scripts\build-windows-arm64.ps1
```

**How the scripts work:**
1. Create temporary backup of all binaries
2. Remove non-target platform binaries
3. Build the application with Tauri
4. Automatically restore all binaries (even if build fails)

**Note**: Cross-compilation may require additional toolchains. See [Tauri Prerequisites](https://tauri.app/v1/guides/building/) for details.

## Build Artifacts

After building, artifacts are located in:
```
src-tauri/target/<target-triple>/release/bundle/
```

### Linux
- `appimage/*.AppImage`
- `deb/*.deb`

### Windows
- `msi/*.msi`
- `nsis/*.exe`

### macOS
- `dmg/*.dmg`
- `macos/*.app`

## Troubleshooting

### "Binary not found" errors in production builds

**Cause**: The build included binaries for multiple platforms or wrong platform.

**Solution**: Use the platform-specific build scripts or CI/CD workflow to ensure only the target platform's binaries are bundled.

### Build scripts fail to restore binaries

**Cause**: Script interrupted or error during build.

**Solution**: Manually restore from git:
```bash
git checkout src-tauri/binaries
```

## Adding New Binaries

If you need to add new platform binaries:

1. Place them in the appropriate platform directory:
   ```
   src-tauri/binaries/<platform>/your-binary
   ```

2. Update `src-tauri/tauri.conf.json` resources array if needed (though glob patterns should catch new files automatically)

3. Update binary resolution code in `depot_runner.rs` or `zip_runner.rs` if adding new binary types

## Technical Details

### Binary Resolution at Runtime

The application uses platform detection to construct the correct binary path:

```rust
// Example from depot_runner.rs
let platform_subdir = get_platform_subdir(); // Returns "linux-x64" on Linux x64
let path = format!("binaries/{}/{}", platform_subdir, binary_name);
```

### Bundle Configuration

`tauri.conf.json` lists all platform directories as resources:

```json
"resources": [
  "binaries/win-x64/*",
  "binaries/linux-x64/*",
  ...
]
```

During builds, the scripts remove unwanted platform directories before bundling, ensuring only the target platform's binaries are included.
