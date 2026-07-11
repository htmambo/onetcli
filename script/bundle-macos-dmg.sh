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

APP_NAME="OmniHub"
TARGET="${1:-$(detect_macos_target)}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="${OMNIHUB_PROJECT_DIR:-$(cd "$SCRIPT_DIR/.." && pwd)}"
APP_DIR="${PROJECT_DIR}/target/${APP_NAME}.app"
TMP_DIR="${PROJECT_DIR}/target/dmg"
DMG_NAME="omnihub-${TARGET}.dmg"
DMG_PATH="${PROJECT_DIR}/${DMG_NAME}"
DMG_RETRIES="${OMNIHUB_DMG_RETRIES:-3}"
DMG_RETRY_DELAY="${OMNIHUB_DMG_RETRY_DELAY:-5}"

if [ ! -d "$APP_DIR" ]; then
    echo "错误：未找到 App 包 ${APP_DIR}"
    echo "请先执行：script/bundle-macos.sh ${TARGET}"
    exit 1
fi

rm -rf "$TMP_DIR"
mkdir -p "$TMP_DIR"
cp -R "$APP_DIR" "$TMP_DIR/${APP_NAME}.app"
ln -s /Applications "$TMP_DIR/Applications"

# 生成可分发的压缩 DMG（UDZO）
create_dmg() {
    local attempt
    local tmp_dmg

    for attempt in $(seq 1 "$DMG_RETRIES"); do
        tmp_dmg="${PROJECT_DIR}/target/${DMG_NAME}.tmp.${attempt}.$$.dmg"
        rm -f "$tmp_dmg" "$DMG_PATH"

        if hdiutil create \
            -volname "${APP_NAME}" \
            -srcfolder "$TMP_DIR" \
            -ov \
            -size 200m \
            -format UDZO \
            "$tmp_dmg"; then
            mv -f "$tmp_dmg" "$DMG_PATH"
            return 0
        fi

        rm -f "$tmp_dmg" "$DMG_PATH"
        if [ "$attempt" -lt "$DMG_RETRIES" ]; then
            echo "hdiutil create failed; retrying in ${DMG_RETRY_DELAY}s (${attempt}/${DMG_RETRIES})..."
            sleep "$DMG_RETRY_DELAY"
        fi
    done

    echo "Error: failed to create DMG after ${DMG_RETRIES} attempt(s)"
    return 1
}

create_dmg

# 可选：如果提供签名身份，则对 DMG 执行签名
if [ -n "${MACOS_SIGN_IDENTITY:-}" ]; then
    echo "使用签名身份对 DMG 签名：${MACOS_SIGN_IDENTITY}"
    codesign --force --sign "${MACOS_SIGN_IDENTITY}" "$DMG_PATH"
fi

echo "DMG 打包完成：${DMG_PATH}"
ls -lh "$DMG_PATH"
