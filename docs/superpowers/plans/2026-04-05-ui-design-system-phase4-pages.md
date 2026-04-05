# UI 设计系统 - 阶段四：核心页面应用实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.
>
> **分支:** `feat/ui-design-system`
> **前置条件:** 阶段一/二/三已完成，tokens 模块已就绪

**Goal:** 将核心页面（Home、Settings、数据库视图）中的硬编码样式值替换为统一的设计 token，重点修复 P0-P1 级不一致问题。

---

## 扫描结果摘要

| 优先级 | 问题类型 | 数量 | 示例 |
|--------|---------|------|------|
| P0 | 工具栏宽高硬编码 | 2 | encourage.rs, sidebar/mod.rs → **阶段二已修复** |
| P1 | 圆角不一致、大宽度未用 token | 4 | setting_tab 16px, db_tab 24px, objects_tab 32px, chat_panel 360px |
| P2 | 圆角硬编码（多处） | 15+ | home_tab, home/*, db_view/*, saved_connection_picker |
| - | 重复常量（database_tab.rs） | 4 | 定义了已存在于 layout.rs 的常量 |

---

## Task 1: 修复 P1 级不一致

**Files:**
- Modify: `main/src/setting_tab.rs:541` — `.rounded(px(16.0))` → `Radius::Xl.px()`
- Modify: `crates/db_view/src/database_tab.rs:399` — `.rounded(px(24.0))` → `Radius::Xl.px()`
- Modify: `crates/db_view/src/database_objects_tab.rs:672` — `.h(px(32.))` 工具栏高度
- Modify: `crates/db_view/src/chatdb/chat_panel.rs:442` — `.w(px(360.0))` → `CHAT_SIDEBAR_WIDTH` token

- [ ] **Step 1: 修复 setting_tab.rs**

```rust
use gpui_component::tokens::radius::Radius;

// 替换 .rounded(px(16.0)) 为
.rounded(Radius::Xl.px())
```

- [ ] **Step 2: 修复 database_tab.rs (圆角)**

```rust
use crate::tokens::radius::Radius;

// 替换 .rounded(px(24.0)) 为
.rounded(Radius::Xl.px())
```

- [ ] **Step 3: 修复 database_objects_tab.rs (工具栏高度)**

调研该工具栏高度 32px 是否应改为 36px（TOOLBAR_HEIGHT）。如果合理则替换。

- [ ] **Step 4: 修复 chat_panel.rs (聊天侧边栏宽度)**

```rust
use crate::layout::CHAT_SIDEBAR_DEFAULT_WIDTH;

// 替换 .w(px(360.0)) 为
.w(CHAT_SIDEBAR_DEFAULT_WIDTH)
```

- [ ] **Step 5: 验证编译**

Run: `cd /d/usr/htdocs/onetcli && cargo build 2>&1 | tail -5`
Expected: 编译成功

- [ ] **Step 6: 提交**

```bash
git add main/src/setting_tab.rs crates/db_view/src/database_tab.rs crates/db_view/src/database_objects_tab.rs crates/db_view/src/chatdb/chat_panel.rs
git commit -m "refactor: 修复 P1 级样式不一致

- setting_tab: 16px 圆角改为 Radius::Xl
- database_tab: 24px 圆角改为 Radius::Xl
- database_objects_tab: 工具栏高度改为统一值
- chat_panel: 360px 宽度改为 CHAT_SIDEBAR_DEFAULT_WIDTH

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: 修复 P2 级圆角硬编码

**Files:**
- Modify: `main/src/home/home_workspace_filter.rs:124` — `.rounded(px(4.0))` → `Radius::Sm`
- Modify: `main/src/home/home_new_connection.rs:165` — `.rounded(px(6.0))` → `Radius::Md`
- Modify: `main/src/home/home_connection_quick_open.rs:86` — `.rounded(px(6.0))` → `Radius::Md`
- Modify: `main/src/home_tab.rs` (多处 8px 圆角)
- Modify: `crates/db_view/src/db_tree_view.rs:189` — `.rounded(px(4.0))` → `Radius::Sm`
- Modify: `crates/db_view/src/database_tab.rs:429` — `.rounded(px(8.0))` → `Radius::Lg`
- Modify: `main/src/saved_connection_picker.rs` (多处 6px 圆角)

- [ ] **Step 1: 修复 Home 子模块圆角**

在 `home_workspace_filter.rs`, `home_new_connection.rs`, `home_connection_quick_open.rs` 中：
```rust
use gpui_component::tokens::radius::Radius;

// .rounded(px(4.0)) → .rounded(Radius::Sm.px())
// .rounded(px(6.0)) → .rounded(Radius::Md.px())
```

- [ ] **Step 2: 修复 home_tab.rs 圆角**

替换所有 `.rounded(px(8.0))` 为 `.rounded(Radius::Lg.px())`

- [ ] **Step 3: 修复 db_view 圆角**

在 `db_tree_view.rs` 和 `database_tab.rs` 中替换对应圆角值。

- [ ] **Step 4: 修复 saved_connection_picker.rs**

替换所有 `.rounded(px(6.0))` 为 `.rounded(Radius::Md.px())`

- [ ] **Step 5: 验证编译**

Run: `cd /d/usr/htdocs/onetcli && cargo build 2>&1 | tail -5`
Expected: 编译成功

- [ ] **Step 6: 提交**

```bash
git add main/src/home/ main/src/home_tab.rs main/src/saved_connection_picker.rs crates/db_view/src/db_tree_view.rs crates/db_view/src/database_tab.rs
git commit -m "refactor: 统一核心页面圆角使用 Radius token

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: 消除 database_tab.rs 中的重复常量

**Files:**
- Modify: `crates/db_view/src/database_tab.rs` — 删除重复的常量定义，改用 `one_core::layout::*`

- [ ] **Step 1: 读取 database_tab.rs 的常量定义**

找到以下常量（大约在第 32-35 行）：
```rust
const PANEL_MIN_SIZE: Pixels = px(100.0);
const TREE_PANEL_DEFAULT_SIZE: Pixels = px(250.0);
const CHAT_SIDEBAR_MIN_WIDTH: Pixels = px(360.0);
const CHAT_SIDEBAR_DEFAULT_WIDTH: Pixels = px(420.0);
```

- [ ] **Step 2: 替换为 layout 模块引用**

```rust
use one_core::layout::{
    PANEL_MIN_SIZE,
    TREE_PANEL_DEFAULT_SIZE,
    CHAT_SIDEBAR_MIN_WIDTH,
    CHAT_SIDEBAR_DEFAULT_WIDTH,
};

// 删除本地重复的 const 定义
```

- [ ] **Step 3: 验证编译**

Run: `cd /d/usr/htdocs/onetcli && cargo build 2>&1 | tail -5`
Expected: 编译成功

- [ ] **Step 4: 提交**

```bash
git add crates/db_view/src/database_tab.rs
git commit -m "refactor: 消除 database_tab.rs 中重复的布局常量

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: 全量验证

- [ ] **Step 1: 全量编译**

Run: `cd /d/usr/htdocs/onetcli && cargo build 2>&1 | tail -5`
Expected: 编译成功

- [ ] **Step 2: 运行 tokens 测试**

Run: `cd /d/usr/htdocs/onetcli && cargo test -p gpui-component tokens::tests 2>&1 | tail -10`
Expected: 9/9 测试通过

- [ ] **Step 3: 检查变更范围**

Run: `cd /d/usr/htdocs/onetcli && git diff --stat HEAD~15 HEAD`
Expected: 列出所有修改的文件

- [ ] **Step 4: 最终总结**

```bash
cd /d/usr/htdocs/onetcli && git log --oneline HEAD~20..HEAD
```
