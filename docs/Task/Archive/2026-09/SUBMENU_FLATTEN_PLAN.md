# dump_sql 上下文菜单扁平化为 3 个独立 Item（MySQL/SQLite/PG 转储 SQL 文件 UI 直觉修复）

**Status**: ✅ Completed（完成于 2026-09-13，commit 待推送）
**创建时间**: 2026-09-13
**前置依赖**: 无

## 背景与目标

### 用户反馈（2026-09-13）
> 数据库和数据表的右键菜单中的`转储SQL文件`点击后为什么没有反应？
> 现在显示了，但是子菜单从第二项开始（含）往下和主菜单显示内容重叠，无法使用hover去选中点击。
> 算了，把这几个子菜单扁平化吧，这样至少它们都能看到，但是这些要做为一组显示，和其它菜单使用横线分隔开来

### 现象
- MySQL/SQLite/PostgreSQL 数据库/表右键菜单中"转储 SQL 文件"（带 chevron 的 Submenu）
- Submenu hover 展开后子菜单**与父 PopupMenu 重叠**，无法 hover 选中子项
- 点击 Submenu 标题本身无响应（hover-only 设计）

### 期望
dump_sql 三个 action 扁平化为 3 个独立 Item，由 `context_menu_group` 自然分组、用分隔线与其他菜单分隔。

## 根因分析

`crates/db_view/src/database_view_plugin.rs:312-357` 的 `build_context_menu` 把 3 个 dump_sql action 手动合并为 Submenu：
```rust
if is_dump_sql_action(actions[index].id) {
    let mut sub_items = Vec::new();
    while index < actions.len() && is_dump_sql_action(actions[index].id) {
        if let Some(item) = action_to_context_menu_item(actions[index], node_id) {
            sub_items.push(item);
        }
        index += 1;
    }
    if !sub_items.is_empty() {
        items.push(ContextMenuItem::submenu(
            translate("ImportExport.dump_sql_file"),
            sub_items,
        ));
    }
    continue;
}
```

而 `PopupMenuItem::Submenu` 渲染时：
1. `crates/ui/src/menu/popup_menu.rs:1239-1299` Submenu 渲染分支没有 on_click 监听器，只有 hover 触发的 `selected_index`
2. Submenu PopupMenu 作为父 PopupMenu 的 child 渲染，**被父 PopupMenu bounds clip**，无法在父区域外显示
3. 用 `deferred().with_priority(1)` 让 Submenu 脱离父 bounds 触发 GPUI assertion panic（ContextMenu 已用 deferred）

详见 `SUBMENU_HOVER_POSITION_PLAN.md`（同目录 Archive）。

## 解决方案（用户最终决策）

### dump_sql 三个 action 扁平化为 3 个独立 Item
- 每个 Item label 由 `action.label_i18n_key` 决定：`ImportExport.export_structure` / `ImportExport.export_data` / `ImportExport.export_structure_and_data`
- 顺序由 `context_menu_rank` 决定（Database 节点：40/41/42；Schema/Table 节点：40/41/42）
- 分组：`context_menu_group` 对 dump_sql 三 action 返回相同分组
  - Database 节点：`sql` 组（与 RunSqlFile 同组）
  - Schema 节点：`dump` 组
  - Table 节点：`dump` 组
- 分组变化时由 `build_context_menu` 自动插入分隔线（line 323-326 既有逻辑），无需新增手工分组代码

### 删除的内容
- `is_dump_sql_action()` 函数（dump_sql 合并逻辑唯一使用点）
- `build_context_menu` 中 dump_sql Submenu 合并分支
- `ContextMenuItem::submenu(...)` 在 dump_sql 上的调用

### 不动的内容
- `crates/ui/src/menu/popup_menu.rs` 通用 Submenu 渲染逻辑
- `context_menu_rank` 中 dump_sql 的排名
- 3 个 action 的 plugin 注册
- 现有测试 `mysql_table_context_menu_keeps_dump_sql_submenu` 和 `mysql_database_context_menu_restores_legacy_order_and_separators`（更新断言）

## 关键文件改动

### 修改（1 个）
- `crates/db_view/src/database_view_plugin.rs:312-357` — `build_context_menu` 去掉 dump_sql Submenu 合并
- 删除 `is_dump_sql_action` 函数（原 line 426-433）
- 更新两个测试断言以反映扁平化（dump_sql 3 个 Item 各自存在 + Database 节点预期列表）

## 验证（用户实测）

1. ✅ MySQL 数据库/表右键菜单中看到 3 个 dump_sql Item
2. ✅ 3 个 Item 与其他菜单通过 context_menu_group 自然分组 + 分隔线分隔
3. ✅ 点击任一 dump_sql Item 触发对应 handler（dump_sql handler 已修复 panic，能正常 connect + 转储）
4. ✅ cargo check / cargo fmt / cargo clippy 无新增警告
5. ✅ mysql_table_context_menu / mysql_database_context_menu 单测 3/3 通过

## 风险与回滚

| 风险 | 缓解 |
|---|---|
| 菜单布局变化（少一个 Submenu，多了 3 个 Item） | 用户明确要求"扁平化以便能看到"；分组保持（Database 节点 sql 组、Table/Schema 节点 dump 组），无突兀感 |
| 全方言一致行为 | 用户决策；MySQL/SQLite/PG/IPC 同步扁平化 |
| 后续想恢复 Submenu hover 展开 | 详见 `SUBMENU_HOVER_POSITION_PLAN.md` 后续任务规划（架构层面改造 ContextMenu + PopupMenu 多层支持） |

## 与 DB_IMPORT_EXPORT_TOKIO_REACTOR_PANIC_PLAN.md 的关系

两个独立 commit：
1. dump_sql 扁平化（本任务）
2. 数据库 import/export tokio reactor panic 修复（数据库 connect 不再 panic）

按"风险隔离"原则独立 commit，分别评审。

## 备注

- 决策过程：用户先后考虑过方案 1（点击触发首项）、扁平化、保持现状，最终选择扁平化作为妥协方案
- Submenu 完整 hover 修复留作 `SUBMENU_HOVER_POSITION_PLAN.md` 后续任务（架构层面）
- 后续 Submenu hover 任务可能需要：扩展 ContextMenuSharedState 支持多 PopupMenu + 让所有 PopupMenu 都用 deferred().with_priority(1) 渲染
