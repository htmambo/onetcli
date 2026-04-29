#!/usr/bin/env bash
set -euo pipefail

APP_NAME="OnetCli"
PACKAGE_NAME="onetcli"
BINARY_NAME="onetcli"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
PROFILE_NAME="${ONETCLI_BUILD_PROFILE:-release-fast}"
SKIP_BUILD="${ONETCLI_SKIP_BUILD:-false}"
ALLOW_DIRTY="${ONETCLI_ARCH_ALLOW_DIRTY:-false}"
MAINTAINER="${ONETCLI_ARCH_MAINTAINER:-OnetCli <xiaofei.hf@gmail.com>}"

usage() {
    cat <<'EOF'
用法：
  script/package-linux-arch.sh

示例：
  script/package-linux-arch.sh
  ONETCLI_BUILD_PROFILE=release-fast script/package-linux-arch.sh
  ONETCLI_SKIP_BUILD=true script/package-linux-arch.sh

可选环境变量：
  ONETCLI_BUILD_PROFILE    构建 profile，默认 release，可选 dev/debug/release/release-fast
  ONETCLI_SKIP_BUILD       为 true 时跳过项目目录预构建，默认 false
  ONETCLI_VERSION          覆盖版本号，默认读取 main/Cargo.toml
  ONETCLI_ARCH_ALLOW_DIRTY 为 true 时允许脏工作树继续打包，默认 false
  ONETCLI_ARCH_OUTPUT_DIR  包输出目录，默认 target/dist
  ONETCLI_ARCH_STAGING_DIR 构建暂存目录，默认使用 /tmp（空间不足时自动切换到项目目录）
EOF
}

detect_arch() {
    local arch
    arch="$(uname -m)"
    case "${arch}" in
        x86_64) echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        *) echo "错误：不支持的架构 ${arch}" >&2; exit 1 ;;
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
        echo "跳过项目目录预构建。"
        return
    fi

    echo "开始项目目录预构建 ${APP_NAME}"
    cd "${PROJECT_DIR}"
    if [[ "${PROFILE_NAME}" == "dev" || "${PROFILE_NAME}" == "debug" ]]; then
        cargo build -p main
    else
        cargo build --profile "${PROFILE_NAME}" -p main
    fi
}

create_source_tarball() {
    local tarball_path="$1"
    echo "创建源码 tarball: ${tarball_path}"

    cd "${PROJECT_DIR}"
    git archive --prefix="${PACKAGE_NAME}-${VERSION}/" -o "${tarball_path}" HEAD
}

print_dirty_worktree_summary() {
    local status_lines="$1"
    local line_count

    printf '%s\n' "${status_lines}" | sed -n '1,20p'
    line_count="$(printf '%s\n' "${status_lines}" | wc -l | tr -d ' ')"
    if [[ "${line_count}" -gt 20 ]]; then
        echo "..."
    fi
}

ensure_archive_input_is_clean() {
    local status_lines

    if ! git -C "${PROJECT_DIR}" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        return
    fi

    status_lines="$(git -C "${PROJECT_DIR}" status --short --untracked-files=all)"
    if [[ -z "${status_lines}" ]]; then
        return
    fi

    if [[ "${ALLOW_DIRTY}" == "true" ]]; then
        echo "警告：检测到工作树存在未提交改动，源码 tarball 仍将基于 HEAD 导出。" >&2
        echo "警告：未提交的文件改动不会自动进入 Arch 安装包，请确认这是你期望的行为。" >&2
        print_dirty_worktree_summary "${status_lines}" >&2
        return
    fi

    echo "错误：检测到工作树存在未提交改动，默认 Arch 打包会通过 git archive 导出 HEAD。" >&2
    echo "错误：这些未提交改动不会进入安装包，因此产物可能与本地验证结果不一致。" >&2
    if [[ "${SKIP_BUILD}" == "true" ]]; then
        echo "提示：即使启用了 ONETCLI_SKIP_BUILD=true，脚本也只会覆盖预构建二进制，其它未提交文件仍不会进入安装包。" >&2
    fi
    echo "请先提交改动后重试，或显式设置 ONETCLI_ARCH_ALLOW_DIRTY=true 继续。" >&2
    print_dirty_worktree_summary "${status_lines}" >&2
    exit 1
}

write_pkgbuild() {
    local pkgbuild_path="$1"
    local version="$2"

    cat > "${pkgbuild_path}" <<EOF
# Maintainer: ${MAINTAINER}
pkgname=${PACKAGE_NAME}
pkgver=${version}
pkgrel=1
pkgdesc="One Net Client - Database, SSH, Terminal, AI Tools"
arch=('x86_64' 'aarch64')
url="https://github.com/htmambo/onetcli"
license=('Apache-2.0' 'custom')
depends=(
    'fontconfig'
    'gcc-libs'
    'glibc'
    'libxcb'
    'libxkbcommon'
    'libxkbcommon-x11'
    'openssl'
    'systemd-libs'
    'vulkan-icd-loader'
    'wayland'
)
makedepends=('rust' 'clang' 'git')
source=("\${pkgname}-\${pkgver}.tar.gz")
sha256sums=('SKIP')

build() {
    cd "\${pkgname}-\${pkgver}"
    if [[ -f "target/release-fast/onetcli" ]]; then
        echo "使用预构建的二进制文件，跳过编译。"
    else
        # 清除 makepkg 的编译标志，避免干扰 C 依赖的构建
        unset CFLAGS CXXFLAGS LDFLAGS
        # 使用系统链接器替代 rust-lld
        export RUSTFLAGS="\${RUSTFLAGS:-} -C linker=gcc"
        cargo build --profile release-fast -p main
    fi
}

package() {
    cd "\${pkgname}-\${pkgver}"

    # Binary
    install -Dm755 "target/release-fast/onetcli" "\${pkgdir}/usr/bin/onetcli"

    # Desktop file
    install -Dm644 "resources/linux/onetcli.desktop" "\${pkgdir}/usr/share/applications/onetcli.desktop"

    # Icons
    install -Dm644 "resources/linux/onetcli-128.png" "\${pkgdir}/usr/share/icons/hicolor/128x128/apps/onetcli.png"
    install -Dm644 "resources/linux/onetcli-256.png" "\${pkgdir}/usr/share/icons/hicolor/256x256/apps/onetcli.png"
    install -Dm644 "resources/linux/onetcli-512.png" "\${pkgdir}/usr/share/icons/hicolor/512x512/apps/onetcli.png"

    # Themes
    install -dm755 "\${pkgdir}/usr/share/onetcli/themes"
    for theme in themes/*.json themes/*.jsonc; do
        if [[ -f "\${theme}" ]]; then
            install -Dm644 "\${theme}" "\${pkgdir}/usr/share/onetcli/themes/\$(basename "\${theme}")"
        fi
    done

    # Documentation
    install -Dm644 "README.md" "\${pkgdir}/usr/share/doc/\${pkgname}/README.md"

    # Licenses
    install -Dm644 "LICENSE-APACHE" "\${pkgdir}/usr/share/licenses/\${pkgname}/LICENSE-APACHE"
    install -Dm644 "ONETCLI_LICENSE" "\${pkgdir}/usr/share/licenses/\${pkgname}/ONETCLI_LICENSE"
}
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
    exit 0
fi

ARCH="$(detect_arch)"
VERSION="$(resolve_version)"
OUTPUT_DIR="${ONETCLI_ARCH_OUTPUT_DIR:-${PROJECT_DIR}/target/dist}"
PROFILE_DIR="$(profile_output_dir)"
BINARY_PATH="${PROJECT_DIR}/target/${PROFILE_DIR}/${BINARY_NAME}"

cd "${PROJECT_DIR}"
ensure_archive_input_is_clean
build_binary

if [[ ! -f "${BINARY_PATH}" ]]; then
    echo "错误：未找到二进制文件 ${BINARY_PATH}" >&2
    echo "请先执行构建，或确认 ONETCLI_BUILD_PROFILE 配置正确。" >&2
    exit 1
fi

if [[ -n "${ONETCLI_ARCH_STAGING_DIR:-}" ]]; then
    STAGING_DIR="${ONETCLI_ARCH_STAGING_DIR}"
    mkdir -p "${STAGING_DIR}"
else
    # release-fast 编译产物约 6~8GB，/tmp 空间不足时自动切换到项目目录
    tmp_avail="$(df -BG /tmp 2>/dev/null | awk 'NR==2 {print $4}' | tr -d 'G')"
    if [[ -n "${tmp_avail}" && "${tmp_avail}" -ge 15 ]]; then
        STAGING_DIR="$(mktemp -d "${TMPDIR:-/tmp}/${PACKAGE_NAME}-arch.XXXXXX")"
    else
        STAGING_DIR="${PROJECT_DIR}/target/arch-staging"
        mkdir -p "${STAGING_DIR}"
        echo "警告：/tmp 可用空间不足（${tmp_avail:-未知}G < 15G），已切换到项目目录暂存：${STAGING_DIR}" >&2
    fi
fi

TARBALL_PATH="${STAGING_DIR}/${PACKAGE_NAME}-${VERSION}.tar.gz"
PKGBUILD_PATH="${STAGING_DIR}/PKGBUILD"

cleanup() {
    if [[ -z "${ONETCLI_ARCH_STAGING_DIR:-}" && -d "${STAGING_DIR}" ]]; then
        rm -rf "${STAGING_DIR}"
    fi
}
trap cleanup EXIT

echo "开始打包 Arch Linux 包"
echo "目标架构：${ARCH}"
echo "构建 Profile：${PROFILE_NAME}"
echo "版本：${VERSION}"
echo "输出目录：${OUTPUT_DIR}"
echo "暂存目录：${STAGING_DIR}"

mkdir -p "${OUTPUT_DIR}"
OUTPUT_DIR="$(cd "${OUTPUT_DIR}" && pwd)"

create_source_tarball "${TARBALL_PATH}"
write_pkgbuild "${PKGBUILD_PATH}" "${VERSION}"

cd "${STAGING_DIR}"

# 如果跳过构建，将项目目录预构建的二进制复制到 makepkg src/ 目录中
if [[ "${SKIP_BUILD}" == "true" ]]; then
    mkdir -p "src"
    tar xzf "${TARBALL_PATH}" -C "src"
    mkdir -p "src/${PACKAGE_NAME}-${VERSION}/target/release-fast"
    cp "${BINARY_PATH}" "src/${PACKAGE_NAME}-${VERSION}/target/release-fast/${BINARY_NAME}"
    makepkg -fs --noextract --noconfirm
else
    makepkg -fs --noconfirm
fi

BUILT_PKG=$(ls -1 "${PACKAGE_NAME}-${VERSION}"-*.pkg.tar.* 2>/dev/null | head -n 1)
if [[ -z "${BUILT_PKG}" ]]; then
    echo "错误：makepkg 未生成包文件" >&2
    exit 1
fi

cp "${BUILT_PKG}" "${OUTPUT_DIR}/"
echo "Arch 包打包完成：${OUTPUT_DIR}/${BUILT_PKG}"
echo "包信息："
pacman -Qip "${OUTPUT_DIR}/${BUILT_PKG}"
