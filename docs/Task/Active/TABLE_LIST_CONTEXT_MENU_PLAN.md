# 数据库对象表列表右键菜单（与左侧树一致）

**Status**: ✅ Completed (completion: 2026-11-12)，自动化验证与外部评审通过，待用户手测
**Created**: 2026-11-12
**Creator**: Claude Code
**Branch**: refactor/remove-external-db-drivers

## 1. 目标与背景

数据库对象页签（如「表@hongshu2」）中的表列表目前没有右键菜单。需要为表列表行的右键菜单补齐与左侧树「表」节点一致的操作项，且对所有数据库类型通用（MySQL / PostgreSQL / SQLite，以及未来新增插件）。

## 2. 范围

- 仅 `DbNodeType::Table` 的列表行（`DatabaseObjects` 的 Table 列表）。
- 菜单内容完全由插件 manifest 生成：`build_context_menu_for(node.database_type, node.id, Table, cx)`，因此天然跨数据库，不为任何方言写特判。
- 数据记录列表（`table_data` 的 grid）不动；其他对象列表（视图/查询/列等）保持现状。
- 不新增 i18n 文案（菜单标签来自 manifest，另加 `Common.refresh`）。

## 3. 现状（代码事实）

- `Table` 已支持行级右键：`TableDelegate::context_menu(row_ix, menu, window, cx)`（`crates/ui/src/table/delegate.rs:72`），`TableState` 右键时记录可见行索引（`crates/ui/src/table/state.rs:523`）。
- `DatabaseObjectsTableDelegate` 未实现该钩子（`crates/db_view/src/database_objects_tab.rs:121`）。
- 树菜单由 `build_context_menu_for`（`crates/db_view/src/database_view_plugin.rs:618`）基于 manifest 生成，表节点包含：重命名、复制、清空、删除、转储 SQL（子菜单）、导入数据、导出表、数据传输。
- `db_tree_event.rs` 已订阅 `DatabaseObjectsEvent`，且上述操作都有现成 handler。
- 表节点由 `DatabaseObjects::build_node_for_row(row_ix)` 从行数据 + 当前节点构造，`row_ix` 为过滤后的可见行索引（`database_objects_tab.rs:643`）。

## 4. 方案

### 4.1 delegate 持有宿主句柄

`DatabaseObjectsTableDelegate` 增加 `objects: Option<WeakEntity<DatabaseObjects>>`，在两处 delegate 同步点填充（`sync_table_delegate` 与 render 同步）。不预存 `DbNode` 列表，避免每帧克隆与节点过期。

### 4.2 右键菜单构建

实现 `context_menu`：

- 仅 `db_node_type == Table` 时生效，否则原样返回。
- 通过 `filtered_rows` 之外的现有 `build_node_for_row(row_ix)` 取得当前节点（`context_menu` 收到的是可见行索引，与该函数语义一致）。
- 连接激活态按树的方式判定：`ActiveConnections::is_active(connection_id)`。
- 用 `build_context_menu_for` 生成 `ContextMenuItem`，按与树相同的规则渲染 `PopupMenuItem` / separator / submenu；`requires_active && !is_active` 时置灰。
- `DbTreeViewEvent` 转换为携带该节点的 `DatabaseObjectsEvent`（仅映射表相关动作）。
- 末尾追加「刷新」项，与树一致；刷新以 `DatabaseObjects::current_node` 发出 `Refresh` 事件。

### 4.3 事件与处理器接线

`DatabaseObjectsEvent` 新增：`RenameTable`、`CopyTable`、`TruncateTable`、`DumpSql { node, mode }`、`ImportData`、`ExportData`、`DataTransfer`。

`db_tree_event.rs` 的订阅里新增 match 分支，直接复用现有 handler：

- `handle_rename_table`、`handle_copy_table`、`handle_truncate_table`
- `handle_dump_sql_file`、`handle_import_data`、`handle_export_data`、`handle_data_transfer`

### 4.4 操作后的列表刷新（评审 R1）

- 复制表、删除表已有 `objects_panel.refresh`。
- `handle_rename_table` 目前只刷新树，新增 `objects_panel: Option<Entity<DatabaseObjectsPanel>>` 参数，成功后同时刷新列表；树的调用点同步传入。
- 截断表不改变表的存在性/名称，列表中的 Rows 列可能短暂偏旧，属可接受（不在本次扩大范围）。

### 4.5 过期节点防护（评审 R2）

- 表格未实现排序（`TableDelegate::perform_sort` 未使用），不存在排序索引映射问题。
- 菜单点击时重新校验该节点仍存在于当前列表（按 `node.id` 扫描 `build_node_for_row`）；不存在则忽略本次操作，避免对已被外部改名/删除的表执行破坏性命令。

## 5. 子任务

1. ✅ delegate 增加 `objects` 弱句柄并在两处同步点填充
2. ✅ 实现 `context_menu`（菜单构建、子菜单、禁用态、刷新项）
3. ✅ 新增 `DatabaseObjectsEvent` 变体与 `DbTreeViewEvent` 映射（穷尽匹配）
4. ✅ `db_tree_event` 接线复用现有 handler；`handle_rename_table` / `handle_truncate_table` 增加列表刷新
5. ✅ 单测：可见行映射、事件映射、非表事件不映射
6. ✅ 验证：`cargo test -p db_view --lib` 317 passed、`cargo check -p main` 通过、fmt 干净、新增代码无 clippy 告警
7. 🔄 外部评审（code）与文档归档

## 6. 验收标准

- MySQL / PostgreSQL / SQLite 的表列表右键菜单项、顺序、分隔符、转储子菜单与左侧树表节点完全一致。
- 右键作用于鼠标所在行，与当前多选无关；非表列表无此菜单。
- 连接未激活时需激活的项置灰。
- 重命名/复制/删除成功后，列表内容同步刷新。
- 节点在菜单打开期间失效时，不再执行该操作。
- 编译与测试通过。

## 7. 风险与回滚

- 复用树 handler，行为与树一致；新增事件变体为增量变更。
- `WeakEntity` 失效时菜单不显示或点击 no-op。
- 回滚：revert 单 commit，无持久化/数据库变更。

## 8. 外部评审记录

- Requirement 评审（2026-11-12）：**APPROVED**（条件 R1/R2 合并前解决，R3 submenu 开工前 spike）。处理：R1 增加 rename 后的列表刷新；R2 增加点击时节点存在性校验并确认表格未实现排序；R3 经 API 核查 `PopupMenu::build` + `PopupMenuItem::submenu` 与树菜单同一实现，可行。
- Plan 评审（2026-11-12）：**APPROVED**（附 6 项条件）。已落实：H1 共享行映射辅助函数；H2 早退降级路径；M2 树侧 rename/truncate 均传 `Some(objects_panel)`；M4 刷新项与树一致且复用 `Common.refresh`；M1/L4 存在性校验查未过滤 rows + 穷尽映射。
- Code 评审 Round 1（2026-11-12）：**REJECTED**。P0-1 菜单目标经实体映射，存在"画面未重绘时指向错误表"窗口；P0-2 `render_td` 未使用共享映射。修复：目标行改为从 delegate 自身的 `rows`/`filtered_rows` 解析，实体只提供父节点；`render_td` 统一走 `original_row_for_visible` + `.get`。
- Code 评审 Round 2（2026-11-12）：**APPROVED**。P1-3 已把 `is_connection_active` 收敛到 `database_view_plugin` 供树与列表共用；P1-4 补 `node_in_rows` 测试（存在/不存在/行被移除）。
- 已知限制：菜单禁用态为打开时快照（与树一致）；`context_menu` 的早退路径（非表列表、越界行）无 UI 级自动化测试，靠直通早退 + 评审走查覆盖。

## 9. 验收记录

- 自动化（2026-11-12）：`cargo test -p db_view --lib` 318 passed；`cargo check -p main` 通过；`cargo fmt --check` 干净；新增代码无 clippy 告警。
- 手工（待用户执行）：MySQL / PostgreSQL / SQLite 各打开一次表列表，逐项对照左侧树菜单（条目、顺序、转储子菜单、禁用态），并验证重命名/复制/删除/清空后的列表刷新。
