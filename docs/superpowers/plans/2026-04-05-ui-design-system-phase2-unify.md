# UI 设计系统 - 阶段二：统一现有不一致实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.
>
> **分支:** `feat/ui-design-system`
> **前置条件:** 阶段一已完成，tokens 模块已就绪

**Goal:** 将核心页面中分散的硬编码像素值替换为 tokens 模块中的统一常量，重点修复组件库和 db_view 中的不一致问题。

**Tech Stack:** Rust (GPUI), `gpui_component::tokens::spacing`, `gpui_component::tokens::radius`

---

## 现状分析

### 硬编码问题分布

| 位置 | 模式 | 问题 | 优先级 |
|------|------|------|--------|
| `crates/db_view/src/sidebar/mod.rs:176` | `.h(px(36.0))` | 工具栏高度硬编码，应使用 TOOLBAR_HEIGHT | P0 |
| `main/src/encourage.rs:73` | `.h(px(36.0))` | 工具栏高度硬编码，应使用 TOOLBAR_HEIGHT | P0 |
| `crates/ui/src/dock/dock.rs:391` | `.h(px(29.))` | Dock tab 高度，与 tab_panel 不一致 | P1 |
| `crates/ui/src/dock/tab_panel.rs:643` | `.h(px(30.))` | Dock tab 高度，与 dock 不一致 | P1 |
| `crates/ui/src/tooltip.rs:100` | `.rounded(px(6.))` | 圆角硬编码，应使用 Radius::Md | P2 |
| `crates/ui/src/button/button.rs` | 字号相关硬编码 | 字号分散 | P2 |

### 间距硬编码（px() 调用）

在 `crates/ui/src/` 下共有 ~24 处 `.px()` 调用，大部分是合理的动态值（如内容宽度），但以下是需要统一的目标：
- `.h(px(36.0))` → `TOOLBAR_HEIGHT`
- `.rounded(px(6.))` → `Radius::Md`
- Dock tab 高度统一 → 建议 30px

---

## Task 1: 统一工具栏高度（TOOLBAR_HEIGHT）

**Files:**
- Modify: `crates/db_view/src/sidebar/mod.rs:176`
- Modify: `main/src/encourage.rs:73`

- [ ] **Step 1: 修改 db_view/sidebar/mod.rs**

找到 `.h(px(36.0))`，替换为使用 TOOLBAR_HEIGHT：

```rust
// 在文件顶部添加导入
use gpui_component::tokens::spacing::TOOLBAR_HEIGHT;

// 将 .h(px(36.0)) 替换为
.h(px(TOOLBAR_HEIGHT))
```

Run: `cd /d/usr/htdocs/onetcli && cargo build -p db_view 2>&1 | head -20`
Expected: 编译成功

- [ ] **Step 2: 修改 encourage.rs**

找到 `.h(px(36.0))`，替换为使用 TOOLBAR_HEIGHT：

```rust
use gpui_component::tokens::spacing::TOOLBAR_HEIGHT;

// 将 .h(px(36.0)) 替换为
.h(px(TOOLBAR_HEIGHT))
```

Run: `cd /d/usr/htdocs/onetcli && cargo build 2>&1 | head -20`
Expected: 编译成功

- [ ] **Step 3: 提交**

```bash
cd /d/usr/htdocs/onetcli && git add crates/db_view/src/sidebar/mod.rs main/src/encourage.rs && git commit -m "refactor: 统一工具栏高度使用 TOOLBAR_HEIGHT token

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: 统一 Dock tab 面板高度

**Files:**
- Modify: `crates/ui/src/dock/dock.rs:391`
- Modify: `crates/ui/src/dock/tab_panel.rs:643`

- [ ] **Step 1: 调研 dock tab 高度**

读取以下两个文件的相关代码：
- `crates/ui/src/dock/dock.rs` 第 391 行附近
- `crates/ui/src/dock/tab_panel.rs` 第 643 行附近

确认当前各自的值和使用场景，确定统一为哪个值（建议 30px 与现有 db_view 保持一致）。

- [ ] **Step 2: 修改 dock.rs**

将 `.h(px(29.))` 替换为统一高度（如 30px）：

```rust
// 在文件顶部添加常量或导入
const DOCK_TAB_HEIGHT: f32 = 30.0;

// 将 .h(px(29.)) 替换为
.h(px(DOCK_TAB_HEIGHT))
```

- [ ] **Step 3: 修改 tab_panel.rs**

将 `.h(px(30.))` 替换为使用同一常量：

```rust
.h(px(DOCK_TAB_HEIGHT))
```

- [ ] **Step 4: 验证编译**

Run: `cd /d/usr/htdocs/onetcli && cargo build -p gpui-component 2>&1 | head -20`
Expected: 编译成功

- [ ] **Step 5: 提交**

```bash
cd /d/usr/htdocs/onetcli && git add crates/ui/src/dock/dock.rs crates/ui/src/dock/tab_panel.rs && git commit -m "refactor: 统一 Dock tab 面板高度

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: 统一圆角使用（Radius token）

**Files:**
- Modify: `crates/ui/src/tooltip.rs:100`

- [ ] **Step 1: 修改 tooltip.rs**

找到 `.rounded(px(6.))`，替换为使用 Radius token：

```rust
// 在文件顶部添加导入
use gpui_component::tokens::radius::Radius;

// 将 .rounded(px(6.)) 替换为
.rounded(Radius::Md.px())
```

- [ ] **Step 2: 验证编译**

Run: `cd /d/usr/htdocs/onetcli && cargo build -p gpui-component 2>&1 | head -20`
Expected: 编译成功

- [ ] **Step 3: 提交**

```bash
cd /d/usr/htdocs/onetcli && git add crates/ui/src/tooltip.rs && git commit -m "refactor: 统一 tooltip 圆角使用 Radius token

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: 全面扫描并修复组件库中剩余硬编码

**Files:**
- Grep: `crates/ui/src/` 下所有 `px()` 调用
- Modify: 确认合理的硬编码（如动态值）保留，样式值替换

- [ ] **Step 1: 扫描 px() 调用**

Run: `cd /d/usr/htdocs/onetcli && grep -rn "\.px(" crates/ui/src/ --include="*.rs" | grep -v tokens/ | grep -v tests.rs`
Expected: 列出所有 px() 调用位置

- [ ] **Step 2: 评估并替换**

对于每个 px() 调用，判断是否为样式值（应替换为 token）还是动态值（保留）：

| 类型 | 示例 | 处理 |
|------|------|------|
| 样式值 | `.h(px(36.0))` 工具栏高度 | 替换为 token |
| 样式值 | `.rounded(px(6.))` 圆角 | 替换为 Radius token |
| 动态值 | `.h(px(height))` 由参数决定 | 保留 |
| 动态值 | `.w(px(total_width))` 由内容决定 | 保留 |
| 布局值 | Divider 的 1px 间隔 | 保留（PANEL_GAP 已是 f32） |

- [ ] **Step 3: 验证编译**

Run: `cd /d/usr/htdocs/onetcli && cargo build -p gpui-component 2>&1 | head -20`
Expected: 编译成功

- [ ] **Step 4: 提交**

```bash
cd /d/usr/htdocs/onetcli && git add crates/ui/src/ && git commit -m "refactor(ui): 替换组件库中剩余的硬编码样式值

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: 全量验证

- [ ] **Step 1: 全量编译**

Run: `cd /d/usr/htdocs/onetcli && cargo build 2>&1 | tail -5`
Expected: 编译成功

- [ ] **Step 2: 全量测试**

Run: `cd /d/usr/htdocs/onetcli && cargo test 2>&1 | tail -10`
Expected: 所有测试通过

- [ ] **Step 3: 总结提交**

```bash
cd /d/usr/htdocs/onetcli && git log --oneline -10
```
