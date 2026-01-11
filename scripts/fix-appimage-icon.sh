#!/usr/bin/env bash
# Fix AppImage icon integration issue
# This script patches the AppImage to fix the broken .DirIcon symlink

set -e

APPIMAGE_PATH="$1"

if [ -z "$APPIMAGE_PATH" ] || [ ! -f "$APPIMAGE_PATH" ]; then
    echo "Usage: $0 <path-to-appimage>"
    echo "Example: $0 src-tauri/target/x86_64-unknown-linux-gnu/release/bundle/appimage/OmniPacker_0.1.0_amd64.AppImage"
    exit 1
fi

echo "Fixing AppImage icon integration for: $APPIMAGE_PATH"

# Create temp directory
TEMP_DIR=$(mktemp -d)
cd "$TEMP_DIR"

# Extract the AppImage
echo "Extracting AppImage..."
"$APPIMAGE_PATH" --appimage-extract > /dev/null 2>&1

# Fix the broken .DirIcon symlink
echo "Fixing .DirIcon symlink..."
cd squashfs-root
if [ -L .DirIcon ]; then
    rm .DirIcon
fi
ln -sf OmniPacker.png .DirIcon

# Fix the app icon symlink to use a larger size (128x128 instead of 16x16)
echo "Updating icon symlink..."
if [ -L omnipacker.png ]; then
    rm omnipacker.png
fi
ln -sf usr/share/icons/hicolor/128x128/apps/omnipacker.png omnipacker.png

cd ..

# Get appimagetool or download it if not available
APPIMAGETOOL=""
if command -v appimagetool &> /dev/null; then
    APPIMAGETOOL="appimagetool"
elif command -v appimagetool-x86_64.AppImage &> /dev/null; then
    APPIMAGETOOL="appimagetool-x86_64.AppImage"
else
    echo "Downloading appimagetool..."
    wget -q https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage
    chmod +x appimagetool-x86_64.AppImage
    APPIMAGETOOL="./appimagetool-x86_64.AppImage"
fi

# Repack the AppImage
echo "Repacking AppImage..."
ARCH=x86_64 $APPIMAGETOOL squashfs-root "$APPIMAGE_PATH" > /dev/null 2>&1

# Cleanup
cd - > /dev/null
rm -rf "$TEMP_DIR"

echo "AppImage icon integration fixed successfully!"
echo ""
echo "Note: For the taskbar icon to appear, you may need to:"
echo "1. Run: gtk-update-icon-cache ~/.local/share/icons/hicolor/ (if integrated)"
echo "2. Or install appimaged for automatic AppImage desktop integration"
echo "3. Or manually integrate with: $APPIMAGE_PATH --appimage-integrate"
