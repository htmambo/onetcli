# Glass Window Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 为 OnetCli 主窗口和主要数据视图落地统一的玻璃视觉。

**Architecture:** 使用平台原生窗口背景材质承接毛玻璃效果，再通过主题表面 token 的 alpha 调节把列表、表格、侧栏、弹窗和编辑器统一切到半透明表面。最后移除主应用里重复铺底的背景层，避免透明度叠加失真。

**Tech Stack:** Rust, GPUI, gpui-component theme system

---

### Task 1: 平台窗口背景

**Files:**
- Modify: `main/src/main.rs`

**Steps:**
1. 提取主窗口背景选择逻辑。
2. 为 macOS / Windows / Linux 设置合适的窗口背景材质。
3. 保留 Deepin/DDE 的安全回退路径。

### Task 2: 主题表面玻璃化

**Files:**
- Modify: `crates/ui/src/theme/theme_color.rs`
- Modify: `crates/ui/src/theme/registry.rs`
- Modify: `crates/ui/src/theme/schema.rs`
- Modify: `crates/ui/src/theme/default-theme.json`

**Steps:**
1. 新增全局 glass surface 调整函数。
2. 在默认主题加载和运行时主题应用时统一启用该调整。
3. 把默认亮/暗主题编辑器背景改成半透明。

### Task 3: 主应用叠层收敛

**Files:**
- Modify: `main/src/onetcli_app.rs`

**Steps:**
1. 去掉主应用外层重复背景。
2. 保留 Root 层基础玻璃表面，避免透明度重复叠加。

### Task 4: 验证

**Files:**
- Verify: `cargo build -p main`

**Steps:**
1. 编译主程序。
2. 如编译失败，修正类型或平台分支问题。
3. 记录验证结果。
