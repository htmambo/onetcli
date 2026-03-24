#!/usr/bin/env bash
set -euo pipefail

APP_NAME="OnetCli"
PACKAGE_NAME="onetcli"
BINARY_NAME="onetcli"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
PROFILE_NAME="${ONETCLI_BUILD_PROFILE:-release-fast}"
SKIP_BUILD="${ONETCLI_SKIP_BUILD:-false}"
MAINTAINER="${ONETCLI_DEB_MAINTAINER:-OnetCli <xiaofei.hf@gmail.com>}"
SECTION="${ONETCLI_DEB_SECTION:-utils}"
PRIORITY="${ONETCLI_DEB_PRIORITY:-optional}"
DESCRIPTION="${ONETCLI_DEB_DESCRIPTION:-One Net Client - Database, SSH, Terminal, AI Tools}"

usage() {
    cat <<'EOF'
用法：
  script/package-linux-deb.sh [target-triple]

示例：
  script/package-linux-deb.sh
  script/package-linux-deb.sh x86_64-unknown-linux-gnu
  ONETCLI_BUILD_PROFILE=release script/package-linux-deb.sh
  ONETCLI_SKIP_BUILD=true script/package-linux-deb.sh

可选环境变量：
  ONETCLI_BUILD_PROFILE    构建 profile，默认 release-fast，可选 dev/debug/release/release-fast
  ONETCLI_SKIP_BUILD       为 true 时跳过 cargo build，默认 false
  ONETCLI_VERSION          覆盖 deb 版本号，默认读取 main/Cargo.toml
  ONETCLI_DEB_OUTPUT_DIR   deb 输出目录，默认 target/dist
  ONETCLI_DEB_DEPENDS      手动覆盖 Depends 字段
  ONETCLI_DEB_MAINTAINER   覆盖 Maintainer 字段
EOF
}

detect_linux_target() {
    local arch
    arch="$(uname -m)"

    case "${arch}" in
        x86_64)
            echo "x86_64-unknown-linux-gnu"
            ;;
        aarch64|arm64)
            echo "aarch64-unknown-linux-gnu"
            ;;
        *)
            echo "错误：不支持的 Linux 架构 ${arch}" >&2
            exit 1
            ;;
    esac
}

target_to_deb_arch() {
    local target="$1"

    case "${target}" in
        x86_64-*linux*)
            echo "amd64"
            ;;
        aarch64-*linux*|arm64-*linux*)
            echo "arm64"
            ;;
        *)
            echo "错误：无法将目标三元组 ${target} 映射为 Debian 架构" >&2
            exit 1
            ;;
    esac
}

resolve_version() {
    if [[ -n "${ONETCLI_VERSION:-}" ]]; then
        echo "${ONETCLI_VERSION}"
        return
    fi

    local version
    version="$(sed -n 's/^version = "\(.*\)"/\1/p' "${PROJECT_DIR}/main/Cargo.toml" | head -n 1)"
    if [[ -z "${version}" ]]; then
        echo "错误：无法从 main/Cargo.toml 读取版本号" >&2
        exit 1
    fi

    echo "${version}"
}

profile_output_dir() {
    case "${PROFILE_NAME}" in
        dev|debug)
            echo "debug"
            ;;
        *)
            echo "${PROFILE_NAME}"
            ;;
    esac
}

build_binary() {
    if [[ "${SKIP_BUILD}" == "true" ]]; then
        echo "跳过构建，直接复用现有二进制。"
        return
    fi

    echo "开始构建 ${APP_NAME}"
    if [[ "${PROFILE_NAME}" == "dev" || "${PROFILE_NAME}" == "debug" ]]; then
        cargo build -p main --target "${TARGET}"
    else
        cargo build --profile "${PROFILE_NAME}" -p main --target "${TARGET}"
    fi
}

detect_depends() {
    local binary_path="$1"

    if [[ -n "${ONETCLI_DEB_DEPENDS:-}" ]]; then
        echo "${ONETCLI_DEB_DEPENDS}"
        return
    fi

    if command -v dpkg-shlibdeps >/dev/null 2>&1; then
        local temp_dir=""
        local output=""
        local depends=""

        temp_dir="$(mktemp -d "${PROJECT_DIR}/target/dpkg-shlibdeps.XXXXXX")"
        mkdir -p "${temp_dir}/debian"
        cat > "${temp_dir}/debian/control" <<EOF
Source: ${PACKAGE_NAME}
Section: ${SECTION}
Priority: ${PRIORITY}
Maintainer: ${MAINTAINER}
Standards-Version: 4.7.0

Package: ${PACKAGE_NAME}
Architecture: ${DEB_ARCH}
Description: ${DESCRIPTION}
EOF

        if output="$(cd "${temp_dir}" && dpkg-shlibdeps -O "${binary_path}" 2>/dev/null)"; then
            depends="$(printf '%s\n' "${output}" | sed -n 's/^shlibs:Depends=//p')"
        fi

        rm -rf "${temp_dir}"

        if [[ -n "${depends}" ]]; then
            echo "${depends}"
            return
        fi

        echo "警告：自动解析运行时依赖失败，将回退到最小依赖集合。" >&2
    fi

    echo "libc6, libstdc++6"
}

write_control_file() {
    {
        echo "Package: ${PACKAGE_NAME}"
        echo "Version: ${VERSION}"
        echo "Section: ${SECTION}"
        echo "Priority: ${PRIORITY}"
        echo "Architecture: ${DEB_ARCH}"
        echo "Maintainer: ${MAINTAINER}"
        if [[ -n "${DEPENDS}" ]]; then
            echo "Depends: ${DEPENDS}"
        fi
        echo "Description: ${DESCRIPTION}"
        echo " A cross-platform desktop client for databases, SSH/SFTP, terminal and AI tools."
    } > "${STAGING_DIR}/DEBIAN/control"
}

write_maintainer_scripts() {
    cat > "${STAGING_DIR}/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database /usr/share/applications >/dev/null 2>&1 || true
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q /usr/share/icons/hicolor >/dev/null 2>&1 || true
fi
EOF

    cat > "${STAGING_DIR}/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database /usr/share/applications >/dev/null 2>&1 || true
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q /usr/share/icons/hicolor >/dev/null 2>&1 || true
fi
EOF

    chmod 755 "${STAGING_DIR}/DEBIAN/postinst" "${STAGING_DIR}/DEBIAN/postrm"
}

copy_package_files() {
    mkdir -p \
        "${STAGING_DIR}/DEBIAN" \
        "${STAGING_DIR}/usr/bin" \
        "${STAGING_DIR}/usr/share/applications" \
        "${STAGING_DIR}/usr/share/icons/hicolor/128x128/apps" \
        "${STAGING_DIR}/usr/share/icons/hicolor/256x256/apps" \
        "${STAGING_DIR}/usr/share/icons/hicolor/512x512/apps" \
        "${STAGING_DIR}/usr/share/doc/${PACKAGE_NAME}"

    install -m 755 "${BINARY_PATH}" "${STAGING_DIR}/usr/bin/${BINARY_NAME}"
    install -m 644 "${PROJECT_DIR}/resources/linux/onetcli.desktop" \
        "${STAGING_DIR}/usr/share/applications/${PACKAGE_NAME}.desktop"
    install -m 644 "${PROJECT_DIR}/resources/linux/onetcli-128.png" \
        "${STAGING_DIR}/usr/share/icons/hicolor/128x128/apps/${PACKAGE_NAME}.png"
    install -m 644 "${PROJECT_DIR}/resources/linux/onetcli-256.png" \
        "${STAGING_DIR}/usr/share/icons/hicolor/256x256/apps/${PACKAGE_NAME}.png"
    install -m 644 "${PROJECT_DIR}/resources/linux/onetcli-512.png" \
        "${STAGING_DIR}/usr/share/icons/hicolor/512x512/apps/${PACKAGE_NAME}.png"
    install -m 644 "${PROJECT_DIR}/README.md" \
        "${STAGING_DIR}/usr/share/doc/${PACKAGE_NAME}/README.md"
    install -m 644 "${PROJECT_DIR}/LICENSE-APACHE" \
        "${STAGING_DIR}/usr/share/doc/${PACKAGE_NAME}/LICENSE-APACHE"
    install -m 644 "${PROJECT_DIR}/ONETCLI_LICENSE" \
        "${STAGING_DIR}/usr/share/doc/${PACKAGE_NAME}/ONETCLI_LICENSE"
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
    exit 0
fi

TARGET="${1:-${ONETCLI_TARGET:-$(detect_linux_target)}}"
VERSION="$(resolve_version)"
DEB_ARCH="$(target_to_deb_arch "${TARGET}")"
OUTPUT_DIR="${ONETCLI_DEB_OUTPUT_DIR:-${PROJECT_DIR}/target/dist}"
STAGING_DIR="${PROJECT_DIR}/target/linux-deb/${PACKAGE_NAME}_${VERSION}_${DEB_ARCH}"
DEB_PATH="${OUTPUT_DIR}/${PACKAGE_NAME}_${VERSION}_${DEB_ARCH}.deb"
PROFILE_DIR="$(profile_output_dir)"
BINARY_PATH="${PROJECT_DIR}/target/${TARGET}/${PROFILE_DIR}/${BINARY_NAME}"

echo "开始打包 Linux deb"
echo "目标架构：${TARGET} -> ${DEB_ARCH}"
echo "构建 Profile：${PROFILE_NAME}"
echo "版本：${VERSION}"
echo "输出目录：${OUTPUT_DIR}"

cd "${PROJECT_DIR}"
build_binary

if [[ ! -f "${BINARY_PATH}" ]]; then
    echo "错误：未找到二进制文件 ${BINARY_PATH}" >&2
    echo "请先执行构建，或确认 ONETCLI_BUILD_PROFILE / TARGET 配置正确。" >&2
    exit 1
fi

DEPENDS="$(detect_depends "${BINARY_PATH}")"

rm -rf "${STAGING_DIR}"
mkdir -p "${OUTPUT_DIR}"
copy_package_files
write_control_file
write_maintainer_scripts

rm -f "${DEB_PATH}"
dpkg-deb --root-owner-group --build "${STAGING_DIR}" "${DEB_PATH}"

echo "deb 打包完成：${DEB_PATH}"
echo "包信息："
dpkg-deb --info "${DEB_PATH}"
