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

APP_NAME="OnetCli"
BINARY_NAME="onetcli"
TARGET="${1:-$(detect_macos_target)}"
VERSION="${ONETCLI_VERSION:-0.1.0}"
PROFILE_NAME="${ONETCLI_BUILD_PROFILE:-release}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
APP_DIR="${PROJECT_DIR}/target/${APP_NAME}.app"

echo "开始打包 ${APP_NAME}.app"
echo "目标架构：${TARGET}"
echo "构建 Profile：${PROFILE_NAME}"
echo "版本：${VERSION}"

# Clean previous bundle
rm -rf "$APP_DIR"

# Create .app directory structure
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Resources"

# Copy binary
BINARY_PATH="${PROJECT_DIR}/target/${TARGET}/${PROFILE_NAME}/${BINARY_NAME}"
if [ ! -f "$BINARY_PATH" ]; then
    echo "错误：未找到二进制文件 ${BINARY_PATH}"
    echo "请先执行：cargo build --profile ${PROFILE_NAME} -p main --target ${TARGET}"
    exit 1
fi
cp "$BINARY_PATH" "$APP_DIR/Contents/MacOS/${BINARY_NAME}"

# Copy Info.plist and substitute version
sed "s/\${ONETCLI_VERSION}/${VERSION}/g" \
    "${PROJECT_DIR}/resources/macos/Info.plist" \
    > "$APP_DIR/Contents/Info.plist"

# Regenerate macOS icon from logo.svg before bundling to avoid stale .icns assets.
bash "${PROJECT_DIR}/script/generate-macos-icon.sh"

# Copy icon
ICNS_PATH="${PROJECT_DIR}/resources/macos/OnetCli.icns"
if [ -f "$ICNS_PATH" ]; then
    cp "$ICNS_PATH" "$APP_DIR/Contents/Resources/OnetCli.icns"
else
    echo "警告：未找到图标文件 ${ICNS_PATH}"
fi

# Write PkgInfo
echo -n "APPL????" > "$APP_DIR/Contents/PkgInfo"

echo "打包完成：${APP_DIR}"
echo "目录内容："
ls -la "$APP_DIR/Contents/"
ls -la "$APP_DIR/Contents/MacOS/"
ls -la "$APP_DIR/Contents/Resources/"
