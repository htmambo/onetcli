#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
SOURCE_SVG="${1:-${PROJECT_DIR}/logo.svg}"
OUTPUT_ICNS="${2:-${PROJECT_DIR}/resources/macos/OnetCli.icns}"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/onetcli-icon.XXXXXX")"
ICONSET_DIR="${WORK_DIR}/OnetCli.iconset"
MASTER_PNG="${WORK_DIR}/OnetCli-master.png"
MACOS_SVG="${WORK_DIR}/logo-macos.svg"
MACOS_ICON_SIZE=1024
MACOS_ICON_MARGIN=96
MACOS_ICON_SCALE="0.8"

cleanup() {
    rm -rf "$WORK_DIR"
}
trap cleanup EXIT

if [ ! -f "$SOURCE_SVG" ]; then
    echo "Error: SVG source not found at ${SOURCE_SVG}"
    exit 1
fi

mkdir -p "$ICONSET_DIR"
mkdir -p "$(dirname "$OUTPUT_ICNS")"

python3 - "$SOURCE_SVG" "$MACOS_SVG" "$MACOS_ICON_SIZE" "$MACOS_ICON_MARGIN" "$MACOS_ICON_SCALE" << 'PYEOF'
import sys
import xml.etree.ElementTree as ET

source_path = sys.argv[1]
output_path = sys.argv[2]
canvas_size = int(sys.argv[3])
margin = int(sys.argv[4])
scale = float(sys.argv[5])

ET.register_namespace('', 'http://www.w3.org/2000/svg')
tree = ET.parse(source_path)
root = tree.getroot()

namespace = root.tag.split('}', 1)[0].strip('{') if '}' in root.tag else ''
root_tag = root.tag
new_root = ET.Element(root_tag, dict(root.attrib))
new_root.set('viewBox', f'0 0 {canvas_size} {canvas_size}')
new_root.set('width', str(canvas_size))
new_root.set('height', str(canvas_size))

children = list(root)
for child in children:
    if child.tag == f'{{{namespace}}}defs':
        new_root.append(child)

content_group = ET.SubElement(
    new_root,
    f'{{{namespace}}}g' if namespace else 'g',
    {'transform': f'translate({margin} {margin}) scale({scale})'},
)

for child in children:
    if child.tag == f'{{{namespace}}}defs':
        continue
    content_group.append(child)

ET.ElementTree(new_root).write(output_path, encoding='utf-8', xml_declaration=True)
PYEOF

echo "Rendering macOS icon from ${SOURCE_SVG}..."
sips -s format png "$MACOS_SVG" --out "$MASTER_PNG" >/dev/null

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

rm -f "$OUTPUT_ICNS"
iconutil -c icns "$ICONSET_DIR" -o "$OUTPUT_ICNS"

# Clean trailing garbage from PNG data in ic* chunks.
# sips sometimes writes extra bytes after PNG IEND, causing non-deterministic icns output.
# NOTE: In ICNS format, each chunk's size field INCLUDES the 8-byte header itself.
python3 - "$OUTPUT_ICNS" << 'PYEOF'
import struct
icns_path = __import__('sys').argv[1]
with open(icns_path, 'rb') as f:
    data = f.read()
result = bytearray(data)
pos = 8
modified = False
while pos + 8 <= len(result):
    chunk_type = bytes(result[pos:pos+4])
    # chunk_size includes the 8-byte header (ICNS spec)
    chunk_size = struct.unpack('>I', result[pos+4:pos+8])[0]
    if chunk_size < 8:
        break
    if chunk_type[:2] == b'ic':
        png_data = bytes(result[pos+8:pos+chunk_size])
        iend_pos = png_data.find(b'IEND')
        if iend_pos >= 0:
            proper_end = iend_pos + 12
            if proper_end < len(png_data):
                del result[pos+8+proper_end:pos+chunk_size]
                result[pos+4:pos+8] = struct.pack('>I', 8 + proper_end)
                result[4:8] = struct.pack('>I', len(result))
                modified = True
                chunk_size = 8 + proper_end
    pos += chunk_size
if modified:
    with open(icns_path, 'wb') as f:
        f.write(result)
    print(f"Cleaned trailing garbage from {icns_path}")
PYEOF

echo "Generated ${OUTPUT_ICNS}"
