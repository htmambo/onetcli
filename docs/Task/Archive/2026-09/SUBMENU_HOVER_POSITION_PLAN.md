# Submenu hover 展开在 PopupMenu 框架内的完整修复

**Status**: ✅ 已完成（2026-09-13，commit `194d5f3d`）
**创建时间**: 2026-09-13
**补充说明（2026-09-20）**：实际根因并非下文所判的"架构限制"——父 PopupMenu 根容器的
`popover_style()` 内置 `overflow_hidden`，把 anchored 子菜单裁到父 bounds 内。
`194d5f3d` 移除该 overflow_hidden（手写 bg/border/shadow/rounded 替代）+
偏移改走 `anchored().offset()` 即修复，方案 A/B/C 证明不必要，dump_sql 扁平化
（`4b345b5f`）同步撤销。残留限制：父菜单 scrollable（>20 项）时
`overflow_y_scroll` content mask 仍会裁子菜单（`popup_menu.rs` 有 TODO）。
以下为当时的分析原文，保留备查。

## 背景与目标

### 用户反馈（2026-09-13）
> 数据库和数据表的右键菜单中的`转储SQL文件`点击后为什么没有反应？
> 子菜单正常应该是鼠标hover时就显示，但是现在不管是hover还是click都不显示

实测发现：
1. Submenu hover 也不显示子菜单
2. PopupMenuItem::Submenu 在 `crates/ui/src/menu/popup_menu.rs:1239-1299` 渲染分支没有 on_click 监听器，只有 hover 触发的 selected_index 状态

### 根因（通过实测 + 静态分析定位）

**问题 1（已部分修复）**：`update_submenu_menu_anchor` 计算的 `left` 偏移被错误地传给 `div().left(left)`（margin-left），而非 anchored 元素的 `.offset(...)`。`bounds.size.width - px(8.)` 作为 div margin 让 anchored 子菜单相对 MenuItemElement 向右偏移整个父 PopupMenu 宽度，子菜单算到屏幕外（snap_to_window_with_margin 兜底但 snap 行为不理想）。

**问题 2（架构限制，无法用 anchor 修复）**：Submenu 的 PopupMenu 实体作为父 PopupMenu 的 child 渲染，**被父 PopupMenu bounds clip**（用户实测观察到"子菜单只能在主菜单已经渲染的区域内显示"）。子菜单的 menu_items 按顺序渲染，与父 PopupMenu 同位置重叠（用户观察到"子菜单从第二项开始（含）往下和主菜单显示内容重叠，无法使用 hover 选中点击"）。

**问题 3（与 deferred 冲突）**：ContextMenu 用 `deferred().with_priority(1)` 让 PopupMenu 渲染到屏幕顶层（line 185-213）。Submenu 如果也用 `deferred()` 会触发 GPUI assertion `cannot call defer_draw during deferred drawing`（已在用户实测中复现）。

### 真正修复方向（架构层面）

**方案 A**：扩展 `ContextMenuSharedState` 同时跟踪多个 PopupMenu entity（主菜单 + 子菜单 + 子子菜单...），所有 PopupMenu 都用 `deferred().with_priority(1)` 渲染到屏幕顶层。需要修改：
- `crates/ui/src/menu/context_menu.rs` —— `ContextMenuSharedState` 改为 `Vec<Entity<PopupMenu>>`，render 时遍历 deferred render 所有 PopupMenu
- `crates/ui/src/menu/popup_menu.rs` —— Submenu 选中时把自身 entity 注册到 ContextMenuSharedState（需要传 ContextMenu state 引用）

**方案 B**：改造 PopupMenu 自身支持多层，每层 PopupMenu 独立的 `deferred().with_priority(1)` 渲染，自己管自己的 deferred draw。复杂度高，可能需要全局改造 PopupMenu 渲染路径。

**方案 C**：放弃 Submenu 的"自动从父 PopupMenu bounds 溢出"语义，改用**Submenu 自动扩展父 PopupMenu bounds**——即父 PopupMenu bounds 动态包含 Submenu bounds，Submenu 作为父 PopupMenu layout 的一部分渲染（不 clip）。但这会让父 PopupMenu 高度变化，影响其他菜单项布局。

### 推荐方案
**方案 A**（最贴合现有 ContextMenu/Submenu 架构）：
1. 扩展 `ContextMenuSharedState` 支持多 PopupMenu
2. Submenu 选中时注册到 ContextMenu state（需要 popup_menu 知道 context menu state）
3. 所有 PopupMenu 统一用 `deferred().with_priority(1)` 渲染

## 不在本次提交范围

本次提交只包含 **数据库 import/export tokio reactor panic 修复**（`crates/db/src/manager.rs` + 3 个 view 文件），Submenu 完整修复作为独立后续任务。

## 关键文件改动（后续任务）

### 修改（2-3 个）
- `crates/ui/src/menu/context_menu.rs`：ContextMenuSharedState 改 Vec + render 遍历 deferred
- `crates/ui/src/menu/popup_menu.rs`：Submenu 选中时注册到 ContextMenu state（需要新机制传递 state 引用）

### 不动
- `crates/db_view/src/database_view_plugin.rs`（Submenu 合并逻辑正常）
- Submenu hover 选中触发 `selected_index = Some(ix)` 逻辑（已正确）

## 验证（后续任务）
1. MySQL 表节点右键 → hover "转储 SQL 文件" → 子菜单正常显示在父 PopupMenu 右边
2. 子菜单子项 hover 高亮、点击触发对应 handler
3. 子菜单 → 子菜单（嵌套）正常工作
4. 子菜单不溢出窗口边界
