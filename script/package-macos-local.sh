#!/bin/bash
set -euo pipefail

detect_macos_target() {
    local arch
    arch="$(uname -m)"

    case "$arch" in
        arm64|aarch64)
            echo "aarch64-apple-darwin"
            ;;
        x86_64)
            echo "x86_64-apple-darwin"
            ;;
        *)
            echo "错误：不支持的 macOS 架构 ${arch}" >&2
            exit 1
            ;;
    esac
}

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TARGET="${1:-$(detect_macos_target)}"
PROFILE_NAME="${ONETCLI_BUILD_PROFILE:-release-fast}"
VERSION="${ONETCLI_VERSION:-0.1.0}"
BUILD_DMG="${BUILD_DMG:-false}"

echo "开始本地快速打包"
echo "目标架构：${TARGET}"
echo "构建 Profile：${PROFILE_NAME}"
echo "版本：${VERSION}"

cd "$PROJECT_DIR"

cargo build --profile "${PROFILE_NAME}" -p main --target "${TARGET}"

ONETCLI_BUILD_PROFILE="${PROFILE_NAME}" \
ONETCLI_VERSION="${VERSION}" \
    bash "${SCRIPT_DIR}/bundle-macos.sh" "${TARGET}"

if [ "${BUILD_DMG}" = "true" ]; then
    bash "${SCRIPT_DIR}/bundle-macos-dmg.sh" "${TARGET}"
fi
