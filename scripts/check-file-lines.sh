#!/usr/bin/env bash
# S9：守护单文件行数不超过软门禁。
#
# AGENTS.md 经验"单文件 ≥ 4000 行不要 fullauto 拆分"——把 4000 作为软门禁 CI 化。
# 超出门禁的 PR 标红，提示 reviewer 评估是否需要拆分。

set -uo pipefail

# 默认上限；可通过环境变量 SOFT_LIMIT 覆盖
SOFT_LIMIT="${SOFT_LIMIT:-4000}"

# 允许的额外上限（针对特定文件，路径相对仓库根）
# 用法：MAX_FILE_LIMIT="main/src/setting_tab.rs=3000:crates/foo/bar.rs=2500" bash scripts/check-file-lines.sh
declare -A MAX_FILE_LIMIT
if [[ -n "${MAX_FILE_LIMIT:-}" ]]; then
    IFS=':' read -ra pairs <<<"$MAX_FILE_LIMIT"
    for pair in "${pairs[@]}"; do
        key="${pair%%=*}"
        val="${pair#*=}"
        MAX_FILE_LIMIT["$key"]="$val"
    done
fi

# 仅检查 main + crates 下的 .rs 文件；排除 vendor/ / target/
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# 收集所有候选文件
mapfile -t files < <(
    find "$ROOT/main/src" "$ROOT/crates" \
        -type f -name "*.rs" \
        ! -path "*/target/*" \
        ! -path "*/vendor/*"
)

violations=0
for f in "${files[@]}"; do
    rel="${f#"$ROOT/"}"
    # 该文件专属上限（默认全局 SOFT_LIMIT）
    limit="$SOFT_LIMIT"
    if [[ -n "${MAX_FILE_LIMIT[$rel]:-}" ]]; then
        limit="${MAX_FILE_LIMIT[$rel]}"
    fi
    line_count=$(wc -l <"$f")
    if [[ "$line_count" -ge "$limit" ]]; then
        echo "[check-file-lines] $rel: $line_count lines (>= $limit)"
        violations=$((violations + 1))
    fi
done

if [[ "$violations" -gt 0 ]]; then
    echo ""
    echo "[check-file-lines] $violations file(s) exceed their limit."
    echo "[check-file-lines] See AGENTS.md 'single-file-4000-lines' for splitting guidance."
    exit 1
fi

echo "[check-file-lines] all files within limits (OK)"