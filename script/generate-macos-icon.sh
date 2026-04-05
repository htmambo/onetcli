#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
SOURCE_SVG="${1:-${PROJECT_DIR}/logo.svg}"
OUTPUT_ICNS="${2:-${PROJECT_DIR}/resources/macos/OnetCli.icns}"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/onetcli-icon.XXXXXX")"
ICONSET_DIR="${WORK_DIR}/OnetCli.iconset"
MASTER_PNG="${WORK_DIR}/OnetCli-master.png"
TRANSPARENT_SVG="${WORK_DIR}/logo-transparent.svg"

cleanup() {
    rm -rf "$WORK_DIR"
}
trap cleanup EXIT

if [ ! -f "$SOURCE_SVG" ]; then
    echo "Error: SVG source not found at ${SOURCE_SVG}"
    exit 1
fi

# Skip generation if icns already exists
if [ -f "$OUTPUT_ICNS" ]; then
    echo "${OUTPUT_ICNS} already exists, skipping generation."
    exit 0
fi

mkdir -p "$ICONSET_DIR"
mkdir -p "$(dirname "$OUTPUT_ICNS")"

bash "${PROJECT_DIR}/script/prepare-transparent-logo.sh" "$SOURCE_SVG" "$TRANSPARENT_SVG"

echo "Rendering macOS icon from ${SOURCE_SVG}..."
sips -s format png "$TRANSPARENT_SVG" --out "$MASTER_PNG" >/dev/null

render_icon() {
    local size="$1"
    local name="$2"
    sips -z "$size" "$size" "$MASTER_PNG" --out "${ICONSET_DIR}/${name}" >/dev/null
}

render_icon 16 icon_16x16.png
render_icon 32 icon_16x16@2x.png
render_icon 32 icon_32x32.png
render_icon 64 icon_32x32@2x.png
render_icon 128 icon_128x128.png
render_icon 256 icon_128x128@2x.png
render_icon 256 icon_256x256.png
render_icon 512 icon_256x256@2x.png
render_icon 512 icon_512x512.png
render_icon 1024 icon_512x512@2x.png

iconutil -c icns "$ICONSET_DIR" -o "$OUTPUT_ICNS"

# Clean trailing garbage from PNG data in ic* chunks.
# sips sometimes writes extra bytes after PNG IEND, causing non-deterministic icns output.
python3 - "$OUTPUT_ICNS" << 'PYEOF'
import struct
icns_path = __import__('sys').argv[1]
with open(icns_path, 'rb') as f:
    data = f.read()
result = bytearray(data)
pos = 8
modified = False
while pos + 8 <= len(result):
    chunk_type = result[pos:pos+4]
    chunk_data_len = struct.unpack('>I', result[pos+4:pos+8])[0]
    chunk_total = 8 + chunk_data_len
    if chunk_type[:2] == b'ic':
        png_data = bytes(result[pos+8:pos+8+chunk_data_len])
        iend_pos = png_data.find(b'IEND')
        if iend_pos >= 0:
            proper_png_len = iend_pos + 12
            if proper_png_len < chunk_data_len:
                cleaned_png = png_data[:proper_png_len]
                before = bytes(result[:pos+8])
                after = bytes(result[pos+8+chunk_data_len:])
                new_file_len = len(before) + len(cleaned_png) + len(after)
                new_header = before[:4] + struct.pack('>I', new_file_len) + before[8:]
                result = bytearray(new_header + cleaned_png + after)
                modified = True
                chunk_total = 8 + proper_png_len
    pos += chunk_total
if modified:
    with open(icns_path, 'wb') as f:
        f.write(result)
    print(f"Cleaned trailing garbage from {icns_path}")
PYEOF

echo "Generated ${OUTPUT_ICNS}"
