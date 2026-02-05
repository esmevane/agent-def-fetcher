#!/bin/bash
#
# build-icon.sh - Convert a source image to a macOS app icon (.icns)
#
# Usage: ./build-icon.sh [source.png]
#
# If no source is provided, uses icon-schmicon.png in the same directory.
#
# Requirements:
#   - ImageMagick (brew install imagemagick)
#   - macOS (for iconutil)
#
# macOS Big Sur icon specs:
#   - 1024x1024 canvas
#   - 824x824 icon area (100px padding on each side)
#   - 185px corner radius
#   - Drop shadow: 28px blur, 12px Y offset, 50% black

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SOURCE="${1:-$SCRIPT_DIR/icon-schmicon.png}"
OUTPUT_DIR="$SCRIPT_DIR"

if [[ ! -f "$SOURCE" ]]; then
    echo "Error: Source file not found: $SOURCE"
    exit 1
fi

if ! command -v magick &> /dev/null; then
    echo "Error: ImageMagick not found. Install with: brew install imagemagick"
    exit 1
fi

echo "Building macOS icon from: $SOURCE"

# Temp files
TEMP_824="$OUTPUT_DIR/.icon-824-temp.png"
ROUNDED="$OUTPUT_DIR/icon-rounded.png"
ICONSET="$OUTPUT_DIR/AppIcon.iconset"
ICNS="$OUTPUT_DIR/AppIcon.icns"

# Clean up previous builds
rm -rf "$ICONSET" "$ICNS" "$TEMP_824" 2>/dev/null || true

# Step 1: Resize to 824x824 and apply rounded corners (185px radius)
echo "  Resizing to 824x824 with rounded corners..."
magick "$SOURCE" -resize 824x824! \
  \( +clone -alpha extract \
     -draw 'fill black polygon 0,0 0,185 185,0 fill white circle 185,185 185,0' \
     \( +clone -flip \) -compose Multiply -composite \
     \( +clone -flop \) -compose Multiply -composite \
  \) -alpha off -compose CopyOpacity -composite \
  "$TEMP_824"

# Step 2: Place on 1024x1024 canvas with drop shadow
echo "  Adding shadow and padding..."
magick -size 1024x1024 xc:transparent \
  \( "$TEMP_824" \
     \( +clone -background black -shadow 50x28+0+12 \) \
     +swap -background transparent -layers merge +repage \
  \) \
  -gravity center -composite \
  "$ROUNDED"

rm "$TEMP_824"

# Step 3: Generate iconset with all required sizes
echo "  Generating iconset..."
mkdir -p "$ICONSET"

sips -z 16 16     "$ROUNDED" --out "$ICONSET/icon_16x16.png"      > /dev/null
sips -z 32 32     "$ROUNDED" --out "$ICONSET/icon_16x16@2x.png"   > /dev/null
sips -z 32 32     "$ROUNDED" --out "$ICONSET/icon_32x32.png"      > /dev/null
sips -z 64 64     "$ROUNDED" --out "$ICONSET/icon_32x32@2x.png"   > /dev/null
sips -z 128 128   "$ROUNDED" --out "$ICONSET/icon_128x128.png"    > /dev/null
sips -z 256 256   "$ROUNDED" --out "$ICONSET/icon_128x128@2x.png" > /dev/null
sips -z 256 256   "$ROUNDED" --out "$ICONSET/icon_256x256.png"    > /dev/null
sips -z 512 512   "$ROUNDED" --out "$ICONSET/icon_256x256@2x.png" > /dev/null
sips -z 512 512   "$ROUNDED" --out "$ICONSET/icon_512x512.png"    > /dev/null
sips -z 1024 1024 "$ROUNDED" --out "$ICONSET/icon_512x512@2x.png" > /dev/null

# Step 4: Convert to icns
echo "  Converting to .icns..."
iconutil -c icns "$ICONSET" -o "$ICNS"

# Clean up iconset (keep icon-rounded.png for reference)
rm -rf "$ICONSET"

echo ""
echo "Done! Created:"
echo "  $ROUNDED (1024x1024 PNG with transparency)"
echo "  $ICNS (macOS icon file)"
echo ""
echo "To rebuild the app bundle:"
echo "  cd $(dirname "$SCRIPT_DIR")"
echo "  cargo bundle --release"
