#!/bin/bash
set -e

cd "$(dirname "$0")"

# Auto-bump version based on git commit count and short hash
COMMIT_COUNT=$(git rev-list --count HEAD 2>/dev/null || echo "0")
COMMIT_HASH=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
VERSION="0.1.${COMMIT_COUNT}"
FULL_VERSION="${VERSION}+${COMMIT_HASH}"
echo "Setting version to ${FULL_VERSION}..."

# Portable sed -i (macOS vs GNU)
sedi() {
    if [[ "$OSTYPE" == "darwin"* ]]; then
        sed -i '' "$@"
    else
        sed -i "$@"
    fi
}

# Update tauri.conf.json (semver only, no hash)
sedi "s/\"version\": \"[^\"]*\"/\"version\": \"${VERSION}\"/" tauri.conf.json

# Update Cargo.toml (semver only, no hash)
sedi "s/^version = \"[^\"]*\"/version = \"${VERSION}\"/" Cargo.toml

# Write full version to a file for the frontend
echo "${FULL_VERSION}" > frontend/version.txt

echo "Cleaning previous build..."
rm -rf target/release/bundle

echo "Building Aeromessage..."
cargo tauri build

APP_PATH="target/release/bundle/macos/Aeromessage.app"
OUT_DIR="out"
DMG_PATH="$OUT_DIR/Aeromessage.dmg"

if [ ! -d "$APP_PATH" ]; then
    echo "Error: Build failed - app not found at $APP_PATH"
    exit 1
fi

echo "Build successful: $APP_PATH"

# Create output directory
mkdir -p "$OUT_DIR"

# Create DMG if create-dmg is available
if command -v create-dmg &> /dev/null; then
    echo "Creating DMG..."
    rm -f "$DMG_PATH"
    # Use --skip-jenkins to avoid opening Finder windows during creation
    create-dmg \
        --volname "Aeromessage" \
        --window-pos 200 120 \
        --window-size 600 400 \
        --icon-size 100 \
        --icon "Aeromessage.app" 150 190 \
        --app-drop-link 450 190 \
        --no-internet-enable \
        --skip-jenkins \
        "$DMG_PATH" \
        "$APP_PATH"
    echo "DMG created: $DMG_PATH"
else
    echo "Error: create-dmg not found. Install with: brew install create-dmg"
    exit 1
fi

# Install to ~/Applications if requested
if [ "$1" = "--install" ]; then
    echo "Installing to ~/Applications..."
    rm -rf ~/Applications/Aeromessage.app
    cp -R "$APP_PATH" ~/Applications/
    echo "Installed to ~/Applications/Aeromessage.app"

    if [ "$2" = "--open" ]; then
        open ~/Applications/Aeromessage.app
    fi
fi

echo "Done!"
