#!/bin/bash
set -e

cd "$(dirname "$0")"

echo "Building Aeromessage..."
npx @tauri-apps/cli build

APP_PATH="target/release/bundle/macos/Aeromessage.app"
DMG_PATH="target/release/bundle/macos/Aeromessage.dmg"

if [ ! -d "$APP_PATH" ]; then
    echo "Error: Build failed - app not found at $APP_PATH"
    exit 1
fi

echo "Build successful: $APP_PATH"

# Create DMG if create-dmg is available
if command -v create-dmg &> /dev/null; then
    echo "Creating DMG..."
    rm -f "$DMG_PATH"
    create-dmg \
        --volname "Aeromessage" \
        --window-pos 200 120 \
        --window-size 600 400 \
        --icon-size 100 \
        --icon "Aeromessage.app" 150 190 \
        --app-drop-link 450 190 \
        "$DMG_PATH" \
        "$APP_PATH"
    echo "DMG created: $DMG_PATH"
else
    echo "Note: install create-dmg to generate DMG (brew install create-dmg)"
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
