# 终端主题切换字号保持 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 修复终端侧边栏切换 theme 时实际字号被重置为 13 的问题，确保当前字号和其他排版参数保持不变。

**Architecture:** 在 `TerminalView` 主题应用入口增加“保留当前排版参数”的合并逻辑，让主题切换只更新配色和主题标识，不覆盖字号、字体族、备用字体和行高比例。用 `view.rs` 内联单测验证该行为，避免再次回归。

**Tech Stack:** Rust、GPUI、现有 `terminal_view` 单元测试

---

### Task 1: 固化根因并补留痕

**Files:**
- Modify: `.claude/operations-log.md`
- Create: `.claude/context-summary-terminal-theme-font-size-preserve.md`

**Step 1: 记录复现与根因**

记录“设置字号为 18 -> 打开终端 -> 侧边栏切换 theme -> 实际字号回到 13”的复现路径，并写明根因是 `TerminalTheme` 整体覆盖。

**Step 2: 校验上下文**

确认 `set_theme`、`apply_theme`、`apply_terminal_settings` 三处职责边界清晰。

### Task 2: 修复主题切换逻辑

**Files:**
- Modify: `crates/terminal_view/src/view.rs`

**Step 1: 提取纯函数**

新增一个帮助函数，用当前主题的排版参数合并目标主题。

**Step 2: 修改 `set_theme`**

让本地 theme 切换使用合并后的主题，而不是直接覆盖。

**Step 3: 修改 `apply_theme`**

让跨 tab / 全局同步分支复用同样的合并逻辑，并使用完整主题比较避免误判。

### Task 3: 补回归测试

**Files:**
- Modify: `crates/terminal_view/src/view.rs`

**Step 1: 新增单测**

验证切换到新主题时：
- 主题名和颜色来自新主题
- 字号、字体族、备用字体、行高比例保留当前值

**Step 2: 运行单测**

执行针对性测试，确保新增用例通过。

### Task 4: 验证与报告

**Files:**
- Create: `.claude/verification-report-terminal-theme-font-size-preserve.md`
- Modify: `.claude/operations-log.md`

**Step 1: 运行 `cargo test` 与 `cargo check`**

至少覆盖 `terminal_view` 针对性测试和主工程编译检查。

**Step 2: 写验证报告**

记录通过项、限制项和最终结论。
