# OmniPacker

**OmniPacker** is a cross-platform desktop application (Tauri v2) that serves as a
GUI frontend for **DepotDownloader** with optional **compression via 7-Zip**.

OmniPacker focuses on making the common workflow easy while keeping the results
deterministic and portable across platforms.

OmniPacker is a community-driven project built for reliable, repeatable releases.

Not affiliated with Valve, Steam, or DepotDownloader.

---

## Download

- See the Releases page for the latest downloads and release notes: https://github.com/elgreams/OmniPacker/releases
---

## Quick Start

1. Enter your AppID, OS, and optional branch.
2. Add to Queue.
3. Start the queue.
4. Open the output folder when complete.

Outputs are stored under `downloads/outputs`.
Use the "Open Output Folder" button to jump to the latest output location.

---

## Highlights

- Cross-platform desktop app with a consistent workflow.
- QR authentication support and queue-wide auth reuse.
- Adaptive 7-Zip compression tuned to current CPU and RAM conditions.
- Cancel during compression while keeping uncompressed output.
- Built-in Template Editor for BBCode release notes.

---

## Authentication Notes

- QR login is supported for Steam authentication.
- Saved login details are stored locally on your machine.
- Credentials are not sent anywhere except to DepotDownloader for login.

---

## Architecture Overview

- Frontend: Plain HTML / CSS / JavaScript (no framework).
- Backend: Rust + Tauri commands.
- Sidecars:
  - DepotDownloader (stable pinned version)
  - 7-Zip
- Execution model: Sidecar binaries are launched and monitored by the Rust backend.

The frontend does not execute external tools directly.

---

## Development

### Quick Start

```bash
npm install
npm run tauri dev
```

---

## Release Builds (Signed, Production-Quality)

Release builds **must** be created using the platform-specific build scripts in the
`scripts/` directory. These scripts ensure that only the target platform's binaries
are bundled, preventing unnecessary bloat from including binaries for other platforms.

**IMPORTANT:** Do not run `npm run tauri build` directly. Always use the appropriate
build script for your target platform.

### Common Prerequisites

- Node.js + npm
- Rust toolchain with appropriate targets installed
- Tauri CLI (`npm install` in this repo)
- Platform-specific build dependencies (see Tauri v2 docs for your OS)
- Python 3 (for Linux/macOS build scripts)

### Linux

OmniPacker supports Linux x64:

**Linux x64 (x86_64):**
```bash
npm install
./scripts/build-linux-x64.sh
```
Artifacts: `src-tauri/target/x86_64-unknown-linux-gnu/release/bundle/`

Linux bundles (AppImage, deb, etc.) will be emitted in the target-specific bundle
directory.

When running the AppImage, OmniPacker will install a per-user desktop entry and
icons under `~/.local/share` on first launch so file managers can display the
correct icon without additional tools.

### macOS

OmniPacker supports both Intel and Apple Silicon Macs.
Minimum version: macOS 12 (Monterey) due to DepotDownloader requirements.

**Prerequisites:**
1. Install Xcode Command Line Tools

**macOS x64 (Intel):**
```bash
npm install
./scripts/build-macos-x64.sh
```
Artifacts: `src-tauri/target/x86_64-apple-darwin/release/bundle/`

**macOS ARM64 (Apple Silicon):**
```bash
npm install
rustup target add aarch64-apple-darwin
./scripts/build-macos-arm64.sh
```
Artifacts: `src-tauri/target/aarch64-apple-darwin/release/bundle/`


### Windows

OmniPacker supports Windows x64.

**Prerequisites:**
1. Install **Node.js LTS** (includes npm)
2. Install the **Rust toolchain** via `rustup` (stable)
3. Install **Visual Studio 2022 Build Tools** (or Visual Studio Community) with:
   - **Desktop development with C++** workload
   - **MSVC v143** toolset
   - **Windows 10/11 SDK**
4. Ensure **WebView2 Runtime** is installed (required by Tauri on Windows)

**Windows x64:**
```powershell
npm install
powershell -ExecutionPolicy Bypass -File .\scripts\build-windows-x64.ps1
```
Artifacts: `src-tauri\target\x86_64-pc-windows-msvc\release\bundle\`
