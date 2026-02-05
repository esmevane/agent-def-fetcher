#!/bin/bash
#
# build-macos-dmg.sh - Build macOS DMG for cargo-dist extra-artifacts
#
# This script is called by cargo-dist's extra-artifacts feature.
# It only runs on macOS; on other platforms it exits successfully with no output.

set -e

# Only run on macOS
if [[ "$(uname)" != "Darwin" ]]; then
    echo "Skipping DMG build on non-macOS platform"
    exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Install cargo-bundle if not present
if ! command -v cargo-bundle &> /dev/null; then
    echo "Installing cargo-bundle..."
    cargo install cargo-bundle
fi

# Build the release binary first (cargo-dist may have already done this)
echo "Building release binary..."
cargo build --release -p agent-defs-gpui

# Build the .app bundle
echo "Building .app bundle..."
cd "$PROJECT_ROOT/crates/agent-defs-gpui"
cargo bundle --release

# Create DMG
APP_PATH="$PROJECT_ROOT/target/release/bundle/osx/Agent Defs Browser.app"
DMG_NAME="Agent-Defs-Browser-macos-universal.dmg"
DMG_PATH="$PROJECT_ROOT/target/distrib/$DMG_NAME"

echo "Creating DMG..."
mkdir -p "$PROJECT_ROOT/target/distrib"

# Create a temporary directory for DMG contents
TEMP_DMG_DIR=$(mktemp -d)
cp -R "$APP_PATH" "$TEMP_DMG_DIR/"

# Create a symlink to /Applications for easy drag-and-drop install
ln -s /Applications "$TEMP_DMG_DIR/Applications"

# Create the DMG
hdiutil create -volname "Agent Defs Browser" \
    -srcfolder "$TEMP_DMG_DIR" \
    -ov -format UDZO \
    "$DMG_PATH"

# Cleanup
rm -rf "$TEMP_DMG_DIR"

echo "Created: $DMG_PATH"
