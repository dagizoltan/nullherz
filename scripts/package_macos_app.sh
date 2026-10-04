#!/usr/bin/env bash
set -e

# Nullherz macOS Bundle Packaging Script
# Generates dist/Nullherz.app bundle with Info.plist, executable, and icns resources.

APP_NAME="Nullherz"
DIST_DIR="dist"
APP_BUNDLE="${DIST_DIR}/${APP_NAME}.app"
CONTENTS_DIR="${APP_BUNDLE}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"

echo "Building Nullherz release executable for macOS..."
cargo build --release -p nullherz-inspector

echo "Preparing macOS bundle layout..."
mkdir -p "${MACOS_DIR}"
mkdir -p "${RESOURCES_DIR}"

echo "Copying executable into bundle..."
cp target/release/nullherz-inspector "${MACOS_DIR}/${APP_NAME}"
chmod +x "${MACOS_DIR}/${APP_NAME}"

echo "Writing Info.plist..."
cat <<EOF > "${CONTENTS_DIR}/Info.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>${APP_NAME}</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon.icns</string>
    <key>CFBundleIdentifier</key>
    <string>cloud.genetic.nullherz</string>
    <key>CFBundleName</key>
    <string>${APP_NAME}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSMicrophoneUsageDescription</key>
    <string>Nullherz requires microphone access for live audio analysis and visual feedback.</string>
</dict>
</plist>
EOF

if [ -f "assets/icons/AppIcon.icns" ]; then
    echo "Copying icon assets..."
    cp "assets/icons/AppIcon.icns" "${RESOURCES_DIR}/AppIcon.icns"
fi

echo "✔ Nullherz macOS Bundle created successfully at ${APP_BUNDLE}"
