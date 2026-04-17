# UI 设计系统 - 阶段五：统一侧边栏与毛玻璃效果

> **分支:** `feat/ui-design-system`
> **前置条件:** 阶段一~四已完成

**Goal:** 消除各模块中重复的毛玻璃辅助函数，并修复 Home 页面侧边栏宽度的硬编码值。

---

## 现状分析

### 重复的毛玻璃函数

| 位置 | 函数名 | 状态 |
|------|--------|------|
| `main/src/home_tab.rs:141` | `macos_home_sidebar_glass` | ❌ 重复 |
| `crates/db_view/src/sidebar/mod.rs:21` | `macos_sidebar_glass` | ❌ 重复 |
| `crates/db_view/src/db_tree_view.rs:46` | `macos_sidebar_glass` | ❌ 重复 |
| `crates/terminal_view/src/sidebar/mod.rs:19` | `glass_bg` | ❌ 重复 |
| `crates/terminal_view/src/sidebar/mod.rs:62` | `macos_sidebar_glass` 等 | ❌ 重复 |
| `crates/redis_view/src/sidebar.rs:14` | `macos_sidebar_glass` | ❌ 重复 |
| `crates/mongodb_view/src/sidebar.rs:14` | `macos_sidebar_glass` | ❌ 重复 |

这些函数实现几乎完全相同，都是：
```rust
fn glass_sidebar(mut color: Hsla, blur_enabled: bool, opacity: f32) -> Hsla {
    if blur_enabled {
        color.a = (opacity + LEFT_PANEL_ALPHA_OFFSET).clamp(0.0, 1.0);
    }
    color
}
```

### 硬编码的侧边栏宽度

| 位置 | 值 | 应改为 |
|------|---|--------|
| `main/src/home_tab.rs:3019` | `px(200.0)` | `SIDEBAR_WIDTH` (255px) |

---

## Task 1: 在 gpui-component 中创建统一的 sidebar glass 函数

**Files:**
- Create: `crates/ui/src/glass_sidebar.rs`
- Modify: `crates/ui/src/lib.rs`

- [ ] **Step 1: 创建 `crates/ui/src/glass_sidebar.rs`**

```rust
//! 侧边栏毛玻璃效果辅助函数
//!
//! 提供统一的侧边栏毛玻璃透明度调整逻辑，
//! 替代各模块中重复的本地实现。

use crate::theme::LEFT_PANEL_ALPHA_OFFSET;
use gpui::Hsla;

/// 为侧边栏背景应用毛玻璃透明度
///
/// 在 blur 启用时，将 opacity + 偏移量作为 alpha 值，
/// 否则返回原始颜色。
///
/// # Arguments
/// * `color` - 原始背景色
/// * `blur_enabled` - 是否启用 blur
/// * `opacity` - 基础透明度（0.0~1.0）
///
/// # Example
/// ```rust
/// div().bg(glass_sidebar(cx.theme().sidebar, blur_enabled, 0.84))
/// ```
pub fn glass_sidebar(color: Hsla, blur_enabled: bool, opacity: f32) -> Hsla {
    if blur_enabled {
        let mut c = color;
        c.a = (opacity + LEFT_PANEL_ALPHA_OFFSET).clamp(0.0, 1.0);
        c
    } else {
        color
    }
}

/// 为侧边栏背景应用毛玻璃透明度（接受 f64 参数）
pub fn glass_sidebar_f64(color: Hsla, blur_enabled: bool, opacity: f64) -> Hsla {
    glass_sidebar(color, blur_enabled, opacity as f32)
}
```

- [ ] **Step 2: 在 `lib.rs` 中导出**

在 `crates/ui/src/lib.rs` 中添加：
```rust
pub mod glass_sidebar;
pub use glass_sidebar::{glass_sidebar, glass_sidebar_f64};
```

- [ ] **Step 3: 验证编译**

Run: `cargo build -p gpui-component`
Expected: 编译成功

- [ ] **Step 4: 提交**

```bash
git add crates/ui/src/glass_sidebar.rs crates/ui/src/lib.rs
git commit -m "feat(ui): 添加统一的 glass_sidebar 辅助函数

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: 替换各模块中的重复毛玻璃函数

**Files:**
- Modify: `main/src/home_tab.rs` — 删除 `macos_home_sidebar_glass`
- Modify: `crates/db_view/src/sidebar/mod.rs` — 删除 `macos_sidebar_glass`
- Modify: `crates/db_view/src/db_tree_view.rs` — 删除 `macos_sidebar_glass`
- Modify: `crates/terminal_view/src/sidebar/mod.rs` — 删除 `glass_bg` 和 `macos_sidebar_glass`
- Modify: `crates/redis_view/src/sidebar.rs` — 删除 `macos_sidebar_glass`
- Modify: `crates/mongodb_view/src/sidebar.rs` — 删除 `macos_sidebar_glass`

- [ ] **Step 1: 替换 main/src/home_tab.rs**

删除 `macos_home_sidebar_glass` 函数（约 10 行），将调用处：
```rust
let sidebar_bg = macos_home_sidebar_glass(cx.theme().sidebar, blur_enabled, glass_opacity);
```
改为：
```rust
let sidebar_bg = gpui_component::glass_sidebar_f64(cx.theme().sidebar, blur_enabled, glass_opacity);
```

同时删除不再使用的 `macos_home_sidebar_glass` 函数定义。

- [ ] **Step 2: 替换 crates/db_view/src/sidebar/mod.rs**

删除本地 `macos_sidebar_glass` 函数（约 7 行），将调用处：
```rust
let bg = macos_sidebar_glass(cx.theme().sidebar, blur_enabled, glass_opacity);
```
改为：
```rust
let bg = gpui_component::glass_sidebar_f64(cx.theme().sidebar, blur_enabled, glass_opacity);
```

- [ ] **Step 3: 替换 crates/db_view/src/db_tree_view.rs**

删除本地 `macos_sidebar_glass` 函数，替换调用。

- [ ] **Step 4: 替换 crates/terminal_view/src/sidebar/mod.rs**

删除 `glass_bg` 和 `macos_sidebar_glass` 函数（约 20 行），替换调用。

- [ ] **Step 5: 替换 crates/redis_view/src/sidebar.rs**

删除本地 `macos_sidebar_glass` 函数，替换调用。

- [ ] **Step 6: 替换 crates/mongodb_view/src/sidebar.rs**

删除本地 `macos_sidebar_glass` 函数，替换调用。

- [ ] **Step 7: 验证编译**

Run: `cargo build`
Expected: 编译成功，无重复函数警告

- [ ] **Step 8: 提交**

```bash
git add main/src/home_tab.rs crates/db_view/src/sidebar/mod.rs crates/db_view/src/db_tree_view.rs crates/terminal_view/src/sidebar/mod.rs crates/redis_view/src/sidebar.rs crates/mongodb_view/src/sidebar.rs
git commit -m "refactor: 统一替换重复的毛玻璃函数为 glass_sidebar

- 删除 main/src/home_tab.rs 的 macos_home_sidebar_glass
- 删除 db_view/sidebar/mod.rs 的 macos_sidebar_glass
- 删除 db_view/db_tree_view.rs 的 macos_sidebar_glass
- 删除 terminal_view/sidebar/mod.rs 的 glass_bg 和 macos_sidebar_glass
- 删除 redis_view/sidebar.rs 的 macos_sidebar_glass
- 删除 mongodb_view/sidebar.rs 的 macos_sidebar_glass

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: 修复 Home 侧边栏宽度硬编码

**Files:**
- Modify: `main/src/home_tab.rs:3019`

- [ ] **Step 1: 查找硬编码值**

在 home_tab.rs 中找到 `.w(px(200.0))` 侧边栏宽度的具体位置。

- [ ] **Step 2: 替换为 token**

添加导入：
```rust
use one_core::layout::SIDEBAR_DEFAULT_WIDTH;
```

替换：
```rust
// 将 .w(px(200.0)) 替换为
.w(SIDEBAR_DEFAULT_WIDTH)
```

> 注意：如果 200px 是设计上有意的值（比默认 255px 更窄），需要确认设计意图。可以保留 200px 作为 Home 页面的特定值。

- [ ] **Step 3: 验证编译**

Run: `cargo build`
Expected: 编译成功

- [ ] **Step 4: 提交**

```bash
git add main/src/home_tab.rs
git commit -m "refactor: 统一 Home 侧边栏宽度

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: 全量验证

- [ ] **Step 1: 全量编译**

Run: `cargo build`
Expected: 编译成功

- [ ] **Step 2: 运行测试**

Run: `cargo test -p gpui-component`
Expected: 所有测试通过

- [ ] **Step 3: 总结**

Run: `git log --oneline HEAD~20..HEAD`
