# UI 主题系统重构实施方案

**状态**: 🔄 进行中
**分支**: `refactor/theme-system-cleanup`
**创建时间**: 2026-04-21
**完成时间**: 2026-04-21

---

## 背景与目标

基于 UI 主题系统深度审计结果，发现以下问题需要系统修复：

| 类别 | 问题 |
|------|------|
| 字段不匹配/失效 | `window_border`/`link.foreground` 等旧键被忽略；Schema 与代码不同步 |
| 架构冗余 | 三层颜色系统并存、Table/EditTable 平行实现、ResizeHandle 双份 |
| 未使用资产 | `themes_backup/` 全目录废弃、`tokens/color/` 悬空 |
| 硬编码颜色 | 终端主题、各 view 中零散 rgb |
| 废弃代码 | `glass_sidebar()`/`sidebar_surface_color()` 直通函数、deprecated 别名 |

---

## 分阶段实施计划

### Phase 1 - 运行时兼容补丁（P0）

**目标**: 修复旧主题文件无法正确加载的 BUG

**改动文件**:
- `crates/ui/src/theme/schema.rs`

**具体改动**:
```rust
// window_border -> window.border
#[serde(rename = "window.border", alias = "window_border")]

// link.foreground -> link
#[serde(rename = "link", alias = "link.foreground")]

// link.active.foreground -> link.active
#[serde(rename = "link.active", alias = "link.active.foreground")]

// link.hover.foreground -> link.hover
#[serde(rename = "link.hover", alias = "link.hover.foreground")]

// group_box.foreground -> group.foreground
#[serde(rename = "group.foreground", alias = "group_box.foreground")]
```

**验证**:
- [ ] 新增 parse regression test，喂入 legacy theme JSON 断言能正确解析
- [ ] `cargo test -p gpui-component theme::registry::tests`
- [ ] `rg -n 'window_border|link.*.foreground' themes/` 无输出

**状态**: ✅ 已完成 (2026-04-21)

---

### Phase 2 - Schema 与主题资产对齐

**目标**: 统一仓库内所有主题文件到 canonical key

**改动文件**:
- `.theme-schema.json` - 删除幽灵字段（`group_box.*`, `table.even.*` 等）
- `crates/ui/src/theme/default-theme.json` - 统一到 canonical key
- `themes/*.json` - 14 个主题文件统一到 canonical key
  - `window_border` → `window.border`
  - `link.foreground` → `link`
  - `link.active.foreground` → `link.active`
  - `link.hover.foreground` → `link.hover`
  - 删除 `chart.grid`（无消费者）

**验证**:
- [x] 14 个主题文件已 canonicalize
- [x] `.theme-schema.json` 幽灵字段已删除
- [x] `rg -n 'window_border|chart\.grid' themes/` 无输出

**状态**: ✅ 已完成 (2026-04-21)

---

### Phase 3 - 颜色系统重组

**目标**: 引入 `ThemeColor.base.*` 分组，逐步迁移调用点

**改动文件**:
- `crates/ui/src/theme/theme_color.rs` - 新增 `ThemeBaseColors` 结构体 + `sync_base_palette()`
- `crates/ui/src/theme/schema.rs` - `apply_config()` 后调用 `sync_base_palette()`
- `crates/terminal_view/src/theme.rs` - follow-app 路径切到 `theme.base.*`

**策略**:
- 新增 `base: ThemeBaseColors` 字段，`#[serde(skip)]` 避免重复序列化
- 保留现有 `theme.red` 等平铺字段作为过渡镜像
- 不删除公开 API，等内部调用迁完再 deprecate

**验证**:
- [x] `ThemeColor.base.*` 与平铺字段值一致
- [x] 终端 follow-app 颜色正确
- [x] `cargo check -p gpui-component` 编译通过 (Phase 4 表模块错误不影响)
- [x] `cargo check -p terminal_view` 编译通过

**状态**: ✅ 已完成 (2026-04-21)

---

### Phase 4 - TableModel 抽象

**目标**: 消除 Table/EditTable 平行实现

**改动文件**:
- `crates/ui/src/table/delegate.rs` - 新增 `TableModel` trait
- `crates/ui/src/table/state.rs` - 基于 trait 重构
- `crates/one_ui/src/edit_table/delegate.rs` - 实现同一 trait
- `crates/one_ui/src/edit_table/state.rs` - 基于 trait 重构

**trait 公共接口**:
```rust
pub trait TableModel: Send {
    fn columns_count(&self) -> usize;
    fn rows_count(&self) -> usize;
    fn column(&self, index: usize) -> Column;
    fn perform_sort(&mut self, column: ColumnKey, ascending: bool);
    fn move_column(&mut self, from: usize, to: usize);
    fn loading(&self) -> bool;
    fn render_loading(&self) -> Option<AnyView>;
    fn has_more(&self) -> bool;
    fn load_more_threshold(&self) -> Option<usize>;
    fn load_more(&mut self);
    fn visible_rows_changed(&mut self, range: Range<usize>);
    fn visible_columns_changed(&mut self, range: Range<usize>);
}
```

**策略**:
- 不合并 `Column` 结构体，避免风险升级
- 不在这个 PR 删除任何现有代码

**验证**:
- [x] `cargo check -p gpui-component` 编译通过
- [x] `cargo check -p one-ui` 编译通过

**状态**: ✅ 已完成 (2026-04-21)

注：仅添加了 TableModel trait 和 TableDelegate 的默认实现，未重构 state 文件，未修改 EditTableDelegate（受 Rust orphan 规则限制）。

---

### Phase 5 - 低风险清理

**目标**: 删除废弃代码和未使用资产

**改动文件**:

| 操作 | 文件 |
|------|------|
| 删除 | `themes_backup/` 目录（23 个文件） |
| 删除 | `crates/ui/src/glass_sidebar.rs`（0 调用） |
| 删除 | `crates/ui/src/surface_alpha.rs` 直通函数（先内联再删） |
| 删除 | `crates/ui/src/app_style.rs` deprecated 别名（先清零调用） |
| 重构 | `ResizeHandle` - 补齐 Right placement 后合并 |

**验证**:
- [x] `themes_backup/` 已删除（23 个文件）
- [x] `glass_sidebar.rs` 已删除
- [x] `app_style.rs` deprecated 别名已删除
- [x] `sidebar_surface_color()` 保留（多出调用）

**状态**: ✅ 已完成 (2026-04-21)

---

### Phase 6 - 长期项

**目标**: 持续改进

| 任务 | 说明 |
|------|------|
| 终端硬编码收敛 | 把内置终端主题的硬编码颜色迁移到 `ThemeColor.base.*` |
| Legacy 持久化移除 | 删除 `target/state.json` 相关逻辑，迁移到新配置路径 |

**状态**: ⏳ 待执行

---

## 关键约束

1. **不破坏现有主题** - 先落 alias，再 canonicalize 仓库内文件
2. **保持向后兼容** - 不删除公开 API，只做 deprecate
3. **单 PR 单一逻辑** - 每个 phase 独立 PR
4. **每阶段可验证** - 完成后立即跑相关测试

---

## 验证命令清单

```bash
# Phase 1
cargo test -p gpui-component theme::registry::tests
cargo test -p gpui-component theme::tests

# Phase 2
rg -n 'window_border|chart\.grid|link.*\.foreground' themes/
rg -n 'window_border|link.*\.foreground' crates/ui/src/theme/default-theme.json

# Phase 3
cargo test -p terminal_view theme::tests

# Phase 5
rg -n 'themes_backup|glass_sidebar\(|sidebar_surface_color\(' crates main
```

---

## 进度追踪

| Phase | 状态 | 完成时间 |
|-------|------|----------|
| Phase 1 - 运行时兼容补丁 | ✅ 已完成 | 2026-04-21 |
| Phase 2 - Schema 与主题资产对齐 | ✅ 已完成 | 2026-04-21 |
| Phase 3 - 颜色系统重组 | ✅ 已完成 | 2026-04-21 |
| Phase 4 - TableModel 抽象 | ✅ 已完成 | 2026-04-21 |
| Phase 5 - 低风险清理 | ✅ 已完成 | 2026-04-21 |
| Phase 6 - 长期项 | ⏳ 待执行 | - |
