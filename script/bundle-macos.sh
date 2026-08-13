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

resolve_version() {
    if [[ -n "${OMNIHUB_VERSION:-}" ]]; then
        echo "${OMNIHUB_VERSION}"
        return
    fi

    local version
    version="$(sed -n 's/^version = "\(.*\)"/\1/p' "${PROJECT_DIR}/main/Cargo.toml" | head -n 1)"
    if [[ -z "${version}" ]]; then
        echo "错误：无法从 main/Cargo.toml 读取版本号，且 OMNIHUB_VERSION 未设置。" >&2
        exit 1
    fi

    echo "${version}"
}

APP_NAME="OmniHub"
BINARY_NAME="omnihub"
TARGET="${1:-$(detect_macos_target)}"
PROFILE_NAME="${OMNIHUB_BUILD_PROFILE:-release}"
VERSION="$(resolve_version)"
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
sed "s/\${OMNIHUB_VERSION}/${VERSION}/g" \
    "${PROJECT_DIR}/resources/macos/Info.plist" \
    > "$APP_DIR/Contents/Info.plist"

# 仅在 icns 缺失时才从 logo-macos.svg 生成。
# sips/iconutil 的输出随 macOS 版本变化，每次打包都重新生成会污染工作区，
# 因此 icns 作为已提交资源复用；需要更新图标时请手动执行
# script/generate-macos-icon.sh 并提交变更。
ICNS_PATH="${PROJECT_DIR}/resources/macos/OmniHub.icns"
if [ ! -f "$ICNS_PATH" ]; then
    bash "${PROJECT_DIR}/script/generate-macos-icon.sh"
fi

# Copy icon
if [ -f "$ICNS_PATH" ]; then
    cp "$ICNS_PATH" "$APP_DIR/Contents/Resources/OmniHub.icns"
else
    echo "警告：未找到图标文件 ${ICNS_PATH}"
fi

# Copy bundled themes to Resources
mkdir -p "$APP_DIR/Contents/Resources/themes"
if [ -d "${PROJECT_DIR}/themes" ]; then
    for theme_file in "${PROJECT_DIR}/themes"/*.json "${PROJECT_DIR}/themes"/*.jsonc; do
        if [ -f "$theme_file" ]; then
            cp "$theme_file" "$APP_DIR/Contents/Resources/themes/"
        fi
    done
fi

# Write PkgInfo
echo -n "APPL????" > "$APP_DIR/Contents/PkgInfo"

# 末尾（仅 macOS）：ad-hoc 签名避免 macOS 15+/26 Local Network Privacy 拦截局域网
# 详见 AGENTS.md:337 已验证经验：未签名的 .app 在 Finder 启动时对局域网返回 EHOSTUNREACH
if [ "$(uname)" = "Darwin" ] && [ -d "$APP_DIR" ]; then
  if codesign --force --deep --sign - "$APP_DIR"; then
    echo "已对 ${APP_DIR} 执行 ad-hoc codesign（macOS 15+/26 局域网权限可用）"
  else
    echo "⚠️ codesign 失败；${APP_DIR} 在 macOS 15+/26 上可能无法触发本地网络权限授权"
  fi
fi

echo "打包完成：${APP_DIR}"
echo "目录内容："
ls -la "$APP_DIR/Contents/"
ls -la "$APP_DIR/Contents/MacOS/"
ls -la "$APP_DIR/Contents/Resources/"
