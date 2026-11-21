#!/usr/bin/env python3
"""差异范围的 clippy 门禁。

仓库当前存在一批历史 clippy 告警，整体 `-D warnings` 会让 CI 立刻变红，
起不到门禁作用。本脚本只拦截**本次改动行**上新引入的告警：

1. 用 `git diff -U0` 算出相对基线分支的改动行区间；
2. 跑 `cargo clippy --workspace --all-targets --message-format=json`；
3. 把每条告警的主 span 行号映射回文件，若落在改动行区间内则判为新增告警。

基线分支由环境变量决定（优先级从高到低）：

- `CLIPPY_DIFF_BASE`：显式指定的基线（分支名或 commit）；
- `GITHUB_BASE_REF`：PR 目标分支（GitHub Actions 自动提供）；
- `origin/dev`：本地默认分支。

用法：
    cargo clippy --workspace --all-targets --message-format=json > /tmp/clippy.json
    python3 scripts/check_clippy_diff.py /tmp/clippy.json
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# 只有主干源码目录参与门禁；vendor/ 与 target/ 属于第三方或构建产物
GATED_PREFIXES = ("main/", "crates/")

# 豁免的 lint 名单文件：每行一个 lint code（# 开头为注释）
IGNORE_FILE = Path(__file__).resolve().parent / "clippy-ignore.txt"

# 历史告警基线：每行 "<相对路径>:<lint code>"，表示该文件该类告警已存量
BASELINE_FILE = Path(__file__).resolve().parent / "clippy-baseline.txt"


def load_ignored_codes() -> set[str]:
    """读取豁免名单。"""
    codes: set[str] = set()
    if not IGNORE_FILE.is_file():
        return codes
    for raw in IGNORE_FILE.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        codes.add(line)
    return codes


def load_baseline() -> set[tuple[str, str]]:
    """读取历史告警基线。"""
    entries: set[tuple[str, str]] = set()
    if not BASELINE_FILE.is_file():
        return entries
    for raw in BASELINE_FILE.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if ":" not in line:
            continue
        # lint code 形如 clippy::expect_used，必须按首个冒号切分
        path, _, code = line.partition(":")
        entries.add((path, code))
    return entries


def is_gated_path(rel_path: str) -> bool:
    """判断文件是否参与门禁。"""
    return rel_path.startswith(GATED_PREFIXES)


def resolve_base() -> str | None:
    """确定用于计算差异的基线引用。"""
    candidates: list[str] = []
    if os.environ.get("CLIPPY_DIFF_BASE"):
        candidates.append(os.environ["CLIPPY_DIFF_BASE"])
    if os.environ.get("GITHUB_BASE_REF"):
        candidates.append(f"origin/{os.environ['GITHUB_BASE_REF']}")
    candidates.append("origin/dev")

    for ref in candidates:
        exists = subprocess.run(
            ["git", "rev-parse", "--verify", "--quiet", f"{ref}^{{commit}}"],
            cwd=ROOT,
            capture_output=True,
        )
        if exists.returncode == 0:
            return ref
    return None


def require_merge_base(base: str) -> None:
    """确认能取到 merge base。

    浅克隆（fetch-depth=1）拿不到 merge base，三点语法会退化成两棵树直接比较，
    门禁范围会失真。这里直接报错，提示 CI 需要完整克隆。
    """
    proc = subprocess.run(
        ["git", "merge-base", base, "HEAD"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0 or not proc.stdout.strip():
        sys.stderr.write(
            f"[check-clippy-diff] 无法计算 {base}...HEAD 的 merge base。\n"
            "[check-clippy-diff] 请用完整历史克隆（CI 中 actions/checkout 设置 fetch-depth: 0）。\n"
        )
        raise SystemExit(2)


def changed_lines(base: str) -> dict[str, set[int]]:
    """返回 {相对路径: 改动后的行号集合}。"""
    proc = subprocess.run(
        ["git", "diff", "-U0", "--no-color", f"{base}...HEAD", "--", "main", "crates"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr)
        return {}

    result: dict[str, set[int]] = defaultdict(set)
    current: str | None = None
    for line in proc.stdout.splitlines():
        if line.startswith("+++ b/"):
            current = line[len("+++ b/") :]
            continue
        if current is None or not line.startswith("@@"):
            continue
        if not is_gated_path(current):
            continue
        # @@ -old_start,old_len +new_start,new_len @@
        new_part = line.split("+", 1)[1].split(" ", 1)[0]
        parts = new_part.split(",")
        start = int(parts[0])
        length = int(parts[1]) if len(parts) > 1 else 1
        if length == 0:
            # 纯删除：改动位置落在 start 上一行
            result[current].add(max(start, 1))
        else:
            result[current].update(range(start, start + length))
    return result


def iter_clippy_messages(path: Path):
    """从 clippy 的 JSON 输出里取出 compiler-message 记录。"""
    with path.open("r", encoding="utf-8", errors="replace") as handle:
        for line in handle:
            line = line.strip()
            if not line.startswith("{"):
                continue
            try:
                payload = json.loads(line)
            except json.JSONDecodeError:
                continue
            if payload.get("reason") != "compiler-message":
                continue
            message = payload.get("message") or {}
            level = message.get("level")
            if level not in ("warning", "error"):
                continue
            spans = message.get("spans") or []
            primary = next((s for s in spans if s.get("is_primary")), spans[0] if spans else None)
            if not primary:
                continue
            file_name = primary.get("file_name")
            line_start = primary.get("line_start")
            if not file_name or not line_start:
                continue
            yield level, message, file_name, line_start


def write_baseline(json_path: Path) -> int:
    """把当前 clippy 结果写成基线文件。"""
    entries: set[tuple[str, str]] = set()
    for _level, message, file_name, _line_start in iter_clippy_messages(json_path):
        rel = Path(file_name).as_posix()
        if rel.startswith("./"):
            rel = rel[2:]
        if not is_gated_path(rel):
            continue
        code = message.get("code") or {}
        code_text = code.get("code") if isinstance(code, dict) else None
        if not code_text:
            continue
        entries.add((rel, code_text))

    lines = [f"{path}:{code}" for path, code in sorted(entries)]
    header = [
        "# clippy 历史告警基线",
        "# 格式：<相对路径>:<lint code>；命中基线的告警不再阻断差异门禁。",
        "# 维护：cargo clippy --workspace --all-targets --message-format=json > clippy.json",
        "#       python3 scripts/check_clippy_diff.py --write-baseline clippy.json",
        "",
    ]
    BASELINE_FILE.write_text("\n".join(header + lines) + "\n", encoding="utf-8")
    print(f"[check-clippy-diff] 已写入基线 {BASELINE_FILE}（{len(lines)} 条）")
    return 0


def main() -> int:
    if len(sys.argv) >= 3 and sys.argv[1] == "--write-baseline":
        json_path = Path(sys.argv[2])
        if not json_path.is_file():
            sys.stderr.write(f"[check-clippy-diff] 未找到 clippy JSON 输出：{json_path}\n")
            return 2
        return write_baseline(json_path)

    if len(sys.argv) < 2:
        sys.stderr.write(
            "usage: check_clippy_diff.py [--write-baseline] <clippy-json-file>\n"
        )
        return 2

    json_path = Path(sys.argv[1])
    if not json_path.is_file():
        sys.stderr.write(f"[check-clippy-diff] 未找到 clippy JSON 输出：{json_path}\n")
        return 2

    base = resolve_base()
    if base is None:
        sys.stderr.write("[check-clippy-diff] 未能确定基线引用，跳过差异门禁。\n")
        return 0

    require_merge_base(base)
    touched = changed_lines(base)
    if not touched:
        print("[check-clippy-diff] 本次改动未涉及 Rust 文件，跳过差异门禁。")
        return 0

    ignored_codes = load_ignored_codes()
    baseline = load_baseline()

    violations = []
    for level, message, file_name, line_start in iter_clippy_messages(json_path):
        rel = Path(file_name).as_posix()
        if rel.startswith("./"):
            rel = rel[2:]
        if not is_gated_path(rel):
            continue
        lines = touched.get(rel)
        if not lines or line_start not in lines:
            continue
        code = message.get("code") or {}
        code_text = code.get("code") if isinstance(code, dict) else None
        if code_text and code_text in ignored_codes:
            continue
        if code_text and (rel, code_text) in baseline:
            continue
        rendered = message.get("rendered") or message.get("message") or ""
        violations.append((level, rel, line_start, code_text, rendered.strip()))

    if not violations:
        print("[check-clippy-diff] 改动行内无新增 clippy 告警（OK）")
        return 0

    print(f"[check-clippy-diff] 改动行内发现 {len(violations)} 条新增告警（基线 {base}）：")
    for level, rel, line, code_text, rendered in violations:
        tag = f"{code_text}" if code_text else "clippy"
        print(f"  {rel}:{line} [{level}/{tag}] {rendered}")
    print("")
    print("[check-clippy-diff] 请修复上述告警，或在代码中显式 `#[allow(...)]`。")
    return 1


if __name__ == "__main__":
    sys.exit(main())
