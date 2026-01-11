#!/bin/bash
set -e

echo "========================================"
echo "Building OmniPacker for Linux x64"
echo "Target: x86_64-unknown-linux-gnu"
echo "========================================"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARIES_DIR="$PROJECT_ROOT/src-tauri/binaries"
CONFIG_FILE="$PROJECT_ROOT/src-tauri/tauri.conf.json"
TEMP_BACKUP=$(mktemp -d)

echo "Creating backup of binaries in $TEMP_BACKUP"
cp -r "$BINARIES_DIR" "$TEMP_BACKUP/"

echo "Creating backup of tauri.conf.json"
cp "$CONFIG_FILE" "$CONFIG_FILE.backup"

cleanup() {
  echo "Restoring original files..."
  rm -rf "$BINARIES_DIR"
  mv "$TEMP_BACKUP/binaries" "$BINARIES_DIR"
  rm -rf "$TEMP_BACKUP"

  if [ -f "$CONFIG_FILE.backup" ]; then
    mv "$CONFIG_FILE.backup" "$CONFIG_FILE"
  fi
}

trap cleanup EXIT

echo "Modifying tauri.conf.json to only include linux-x64 resources..."
python3 -c "
import json
with open('$CONFIG_FILE', 'r') as f:
    config = json.load(f)
config['bundle']['resources'] = ['binaries/linux-x64/*']
with open('$CONFIG_FILE', 'w') as f:
    json.dump(config, f, indent=2)
"

echo "Removing non-target platform binaries..."
cd "$BINARIES_DIR"
for dir in */; do
  dir_name="${dir%/}"
  if [ "$dir_name" != "linux-x64" ]; then
    echo "  Removing $dir_name"
    rm -rf "$dir_name"
  fi
done

echo ""
echo "Remaining binaries:"
ls -la

cd "$PROJECT_ROOT"
echo ""
echo "Starting Tauri build..."
npm run tauri build -- --target x86_64-unknown-linux-gnu

echo ""
echo "Fixing AppImage icon integration..."
APPIMAGE_PATH="$PROJECT_ROOT/src-tauri/target/x86_64-unknown-linux-gnu/release/bundle/appimage/OmniPacker_0.1.0_amd64.AppImage"
if [ -f "$APPIMAGE_PATH" ]; then
  "$SCRIPT_DIR/fix-appimage-icon.sh" "$APPIMAGE_PATH"
else
  echo "Warning: AppImage not found at $APPIMAGE_PATH"
fi

echo ""
echo "========================================"
echo "Build complete!"
echo "Artifacts:"
echo "  AppImage: src-tauri/target/x86_64-unknown-linux-gnu/release/bundle/appimage/"
echo "  Deb: src-tauri/target/x86_64-unknown-linux-gnu/release/bundle/deb/"
echo "========================================"
