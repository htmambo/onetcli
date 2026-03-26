#!/usr/bin/env bash
set -euo pipefail

SOURCE_SVG="${1:?缺少源 SVG 路径}"
OUTPUT_SVG="${2:?缺少输出 SVG 路径}"

if [[ ! -f "${SOURCE_SVG}" ]]; then
    echo "错误：未找到源 SVG ${SOURCE_SVG}" >&2
    exit 1
fi

mkdir -p "$(dirname "${OUTPUT_SVG}")"

perl -0pe 's@\n  <!-- Background -->\n  <rect width="512" height="512" rx="96" fill="url\(#bgGrad\)"/>\n@@' \
    "${SOURCE_SVG}" > "${OUTPUT_SVG}"

if cmp -s "${SOURCE_SVG}" "${OUTPUT_SVG}"; then
    echo "错误：透明背景预处理未生效，请检查 ${SOURCE_SVG} 中的背景图层定义。" >&2
    exit 1
fi
