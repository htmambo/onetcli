# Surface Alpha Unification Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 收口 Home / Settings / Terminal 的局部透明度公式，修正 Dialog 最低透明度实现与测试不一致，同时保持当前运行时视觉行为尽量不变。

**Architecture:** 在 `gpui-component` 主题层新增统一的 surface alpha helper，把 Home / Settings / Terminal 当前分叉的透明度计算迁移到同一处管理；Dialog 统一实现与测试语义，并补上“glass 开关只控制材质、不直接控制透明度”的测试护栏。

**Tech Stack:** Rust, GPUI, gpui-component theme system, main, terminal_view

---

### Task 1: 新增统一的 surface alpha helper

**Files:**
- Create: `crates/ui/src/surface_alpha.rs`
- Modify: `crates/ui/src/lib.rs`

**Steps:**
1. 抽象 Home / Settings / Terminal 当前在用的透明度计算形态。
2. 在 `gpui-component` 暴露统一 helper，覆盖：
   - 基础偏移型 alpha
   - Home 的 level-ratio 型 alpha
   - Windows layer 二次压缩
   - 侧栏 glass alpha
3. 保持 helper 命名表达语义，而不是页面名。

### Task 2: 迁移 Home / Settings / Terminal 到统一 helper

**Files:**
- Modify: `main/src/home_tab.rs`
- Modify: `main/src/setting_tab.rs`
- Modify: `crates/terminal_view/src/view.rs`

**Steps:**
1. Home 改为通过统一 helper 计算 toolbar / section / card 等表面 alpha。
2. Settings 改为通过统一 helper 计算 sidebar / page / header / group alpha。
3. Terminal 改为通过统一 helper 计算 canvas surface opacity。
4. 保持现有视觉语义与平台分支不变，只收口公式来源。

### Task 3: 修正 Dialog 最低透明度冲突

**Files:**
- Modify: `crates/ui/src/theme/glass.rs`

**Steps:**
1. 统一 `DIALOG_SURFACE_BASE_OPACITY` 的实现与测试语义。
2. 本轮默认以“当前实现行为”为准，优先修正测试断言，避免未经确认的视觉行为变更。

### Task 4: 验证

**Files:**
- Verify: `crates/ui/src/theme/glass.rs`
- Verify: `main/src/home_tab.rs`
- Verify: `main/src/setting_tab.rs`
- Verify: `crates/terminal_view/src/view.rs`

**Steps:**
1. 运行 `cargo test -p gpui-component theme::glass -- --nocapture` 或最接近的可用测试入口。
2. 运行 `cargo build -p gpui-component`
3. 运行 `cargo build -p terminal_view`
4. 运行 `cargo build -p main`
5. 若测试入口受限，至少记录编译结果与阻塞原因。

---

## Current Status

- 已完成：
  - 新增 `crates/ui/src/surface_alpha.rs`
  - Home / Settings / Terminal 已迁移到统一 helper
  - `Dialog` 测试已完全对齐当前实现：content 最低 alpha `0.70`，chrome/title/footer `+0.20` 后 clamp
  - 新增 `sidebar_surface_color_with_offset(...)`
  - 新增 `layered_surface_color(...)`
  - `Terminal / SSH` 现有右侧窄工具栏底板与按钮态已切回应用主题语义：底板为 `sidebar + secondary` 混合层，按钮态使用 `list_active / sidebar_accent`
  - `Terminal / SSH` 展开侧栏背景也已切到 `sidebar_surface_color(muted, ...)`，Quick Command / File Manager / Server Monitor 不再用 `background` 覆盖外层表面
  - `DB / Mongo / Redis` 侧栏已统一改为消费 `sidebar_surface_color(...)`
  - `DatabaseObjects` toolbar 与输入框已迁移到 `sidebar_surface_color_with_offset(...)`
  - `Table Designer` 的表头 / 选中态 / hover / 行分隔 / 拖拽 chip 已改为消费 `table_*` 语义 token
  - 新增 `OverlayScrimLevel` 与 `overlay_scrim_color(...)`，先统一 `Blocking / Loading` 两档遮罩强度
  - `SFTP` 的 shell / search bar / header / path bar、拖拽卡片、drop overlay、connection overlay 卡片与 scrim 已迁移到 `layered_surface_color(...)` 或统一 overlay helper
- 语义确认：
  - `main/locales/main.yml` 已明确：
    - `glass_effect_desc`: “为应用窗口启用平台毛玻璃或材质效果，不影响透明度”
    - `glass_opacity_desc`: “独立调整主界面表面的透明程度，不受毛玻璃开关影响”
  - 因此当前代码应保持“关闭 glass 后窗口改为 Opaque，但页面表面仍按 `glass_opacity` 参与 plain surface tuning”的行为。
- 跟进实现：
  - 为 `surface_alpha` helper 增加单元测试
  - 为 `plain_surface_tuning(...)` 增加语义测试
  - 修复 `crates/ui/src/tokens/tests.rs` 中阻塞验证的测试 typo
- 验证结果：
  - `rustfmt --edition 2024 ...`：通过
  - `cargo test -p gpui-component -- --nocapture`：通过（161 passed）
  - `cargo build -p terminal_view`：通过
  - `cargo build -p main`：通过
  - 增量改动后再次运行 `cargo test -p gpui-component surface_alpha -- --nocapture`：通过（14 passed）
  - 增量改动后再次运行 `cargo build -p main`：通过
  - 终端右侧工具栏 / Table Designer / SFTP 状态层改动后再次运行 `cargo build -p main`：通过
  - SFTP scrim / Table Designer 拖拽 chip / Terminal 展开侧栏收口后再次运行 `cargo build -p main`：通过
  - Home 同源 terminal 工具栏修复与 overlay helper 收口后再次运行 `cargo test -p gpui-component surface_alpha -- --nocapture`：通过（16 passed）
  - Home 同源 terminal 工具栏修复与 overlay helper 收口后再次运行 `cargo build -p main`：通过
  - Terminal 窄工具栏按钮态继续对齐 Home sidebar 后再次运行 `cargo build -p main`：通过
  - Terminal 窄工具栏底板进一步提亮并移除 terminal palette 残留状态后再次运行 `cargo build -p main`：通过
