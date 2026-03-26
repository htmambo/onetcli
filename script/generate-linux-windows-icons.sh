#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
SOURCE_SVG="${1:-${PROJECT_DIR}/logo.svg}"
LINUX_DIR="${PROJECT_DIR}/resources/linux"
WINDOWS_DIR="${PROJECT_DIR}/resources/windows"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/onetcli-transparent-icon.XXXXXX")"
TRANSPARENT_SVG="${WORK_DIR}/logo-transparent.svg"

cleanup() {
    rm -rf "${WORK_DIR}"
}
trap cleanup EXIT

if [[ ! -f "${SOURCE_SVG}" ]]; then
    echo "错误：未找到源 SVG ${SOURCE_SVG}" >&2
    exit 1
fi

if ! command -v rsvg-convert >/dev/null 2>&1; then
    echo "错误：缺少 rsvg-convert，无法渲染 SVG。" >&2
    exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
    echo "错误：缺少 python3，无法打包 Windows ICO。" >&2
    exit 1
fi

bash "${SCRIPT_DIR}/prepare-transparent-logo.sh" "${SOURCE_SVG}" "${TRANSPARENT_SVG}"

mkdir -p "${LINUX_DIR}" "${WINDOWS_DIR}"

render_png() {
    local size="$1"
    local output="$2"
    rsvg-convert \
        --keep-aspect-ratio \
        --width "${size}" \
        --height "${size}" \
        "${TRANSPARENT_SVG}" \
        --output "${output}"
}

for size in 128 256 512; do
    render_png "${size}" "${LINUX_DIR}/onetcli-${size}.png"
done

WINDOWS_SIZES=(16 24 32 48 64 128 256)
WINDOWS_PNGS=()
for size in "${WINDOWS_SIZES[@]}"; do
    output="${WINDOWS_DIR}/onetcli-${size}.png"
    render_png "${size}" "${output}"
    WINDOWS_PNGS+=("${output}")
done

python3 - "${WINDOWS_DIR}/onetcli.ico" "${WINDOWS_PNGS[@]}" <<'PY'
import struct
import sys
from pathlib import Path

output = Path(sys.argv[1])
png_paths = [Path(arg) for arg in sys.argv[2:]]
payloads = [path.read_bytes() for path in png_paths]

header_size = 6 + len(payloads) * 16
offset = header_size
entries = []

for path, payload in zip(png_paths, payloads):
    size = int(path.stem.rsplit("-", 1)[1])
    width = 0 if size == 256 else size
    height = 0 if size == 256 else size
    entries.append(struct.pack(
        "<BBBBHHII",
        width,
        height,
        0,
        0,
        1,
        32,
        len(payload),
        offset,
    ))
    offset += len(payload)

output.write_bytes(
    struct.pack("<HHH", 0, 1, len(payloads)) +
    b"".join(entries) +
    b"".join(payloads)
)
PY

echo "已生成 Linux PNG 与 Windows PNG/ICO："
printf '  %s\n' \
    "${LINUX_DIR}/onetcli-128.png" \
    "${LINUX_DIR}/onetcli-256.png" \
    "${LINUX_DIR}/onetcli-512.png" \
    "${WINDOWS_DIR}/onetcli-16.png" \
    "${WINDOWS_DIR}/onetcli-24.png" \
    "${WINDOWS_DIR}/onetcli-32.png" \
    "${WINDOWS_DIR}/onetcli-48.png" \
    "${WINDOWS_DIR}/onetcli-64.png" \
    "${WINDOWS_DIR}/onetcli-128.png" \
    "${WINDOWS_DIR}/onetcli-256.png" \
    "${WINDOWS_DIR}/onetcli.ico"
