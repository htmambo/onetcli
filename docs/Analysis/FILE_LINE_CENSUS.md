# 全仓库 .rs 文件行数普查

**生成时间**: 2026-06-08
**统计范围**: 全仓库（排除 `target/`、`vendor/`、`.omc/`、`.git/`）
**关联任务**: `fullauto:file-line-census`

## 总览

| 指标 | 数值 |
|---|---|
| `.rs` 文件总数 | **677** |
| 代码总行数 | **317,339** |
| > 600 行文件 | **146**（21.6%） |
| > 1000 行文件 | **81**（12.0%） |
| > 1500 行文件 | **45**（6.6%） |
| AGENTS.md 硬门禁（≤ 300 行） | **违反率 21.6%** |

> **关键判断**：硬门禁"文件 ≤ 300 行"在当前仓库中**普遍被违反**。这不是 bug，而是**历史包袱**——大多数核心 crate 在硬门禁建立前就已成型。**门禁应作为"新增 / 修改"约束，对存量文件作为"软目标"**。

## 按 crate 分组（> 1000 行文件的占比）

| Crate | 累加行数 | 文件数 | 平均行数 | 总占比 |
|---|---|---|---|---|
| `db_view` | 35,811 | 16 | 2238 | 11.3% |
| `db` | 30,515 | 15 | 2034 | 9.6% |
| `ui` | 17,141 | 12 | 1428 | 5.4% |
| `terminal_view` | 16,773 | 7 | 2396 | 5.3% |
| `main` | 14,197 | 4 | 3549 | 4.5% |
| `core` | 12,012 | 7 | 1716 | 3.8% |
| `redis_view` | 10,908 | 5 | 2182 | 3.4% |
| `mongodb_view` | 6,697 | 3 | 2232 | 2.1% |
| `sftp_view` | 5,952 | 2 | 2976 | 1.9% |
| `terminal` | 5,046 | 2 | 2523 | 1.6% |
| `story` | 3,341 | 3 | 1114 | 1.1% |
| `one_ui` | 3,182 | 1 | 3182 | 1.0% |
| `ssh` | 1,742 | 1 | 1742 | 0.5% |
| `sftp` | 1,549 | 1 | 1549 | 0.5% |
| `remote_file_editor` | 1,119 | 1 | 1119 | 0.4% |

`db_view` + `db` + `ui` + `terminal_view` 合计占 31.6%——**数据库与终端 UI 是体量核心**。

## Top 25 最大单文件

| 行数 | 路径 |
|---:|---|
| 7525 | `main/src/home_tab.rs` |
| 5515 | `crates/terminal_view/src/view.rs` |
| 4455 | `main/src/setting_tab.rs` |
| 4446 | `crates/db_view/src/table_designer_tab.rs` |
| 4136 | `crates/sftp_view/src/lib.rs` |
| 3780 | `crates/mongodb_view/src/collection_view.rs` |
| 3756 | `crates/db/src/mysql/plugin.rs` |
| 3589 | `crates/db_view/src/db_tree_event.rs` |
| 3549 | `crates/terminal/src/terminal.rs` |
| 3412 | `crates/core/src/tab_container.rs` |
| 3300 | `crates/redis_view/src/key_value_view.rs` |
| 3211 | `crates/db/src/plugin.rs` |
| 3182 | `crates/one_ui/src/edit_table/state.rs` |
| 3174 | `crates/db_view/src/table_data/data_grid.rs` |
| 3028 | `crates/db_view/src/table_data/results_delegate.rs` |
| 3017 | `crates/db/src/mssql/plugin.rs` |
| 3015 | `crates/terminal_view/src/sidebar/file_manager_panel.rs` |
| 2944 | `crates/db_view/src/db_tree_view.rs` |
| 2875 | `crates/db_view/src/common/db_connection_form.rs` |
| 2765 | `crates/db/src/postgresql/plugin.rs` |
| 2700 | `crates/ui/src/input/state.rs` |
| 2670 | `crates/db/src/manager.rs` |
| 2637 | `crates/db/src/oracle/plugin.rs` |
| 2607 | `crates/redis_view/src/redis_tree_view.rs` |
| 2325 | `crates/db_view/src/table_data/filter_editor.rs` |

完整 81 个 > 1000 行文件清单见附表 §附表 A。

## 拆分难度分类

按本次 `setting_tab.rs` 拆分尝试的硬经验（详见 `AGENTS.md:344` 经验 3），对 Top 25 进行拆分难度分级：

### 🔴 不可在 fullauto 模式下拆分（结构耦合）

| 文件 | 行数 | 阻碍 |
|---|---:|---|
| `main/src/home_tab.rs` | 7525 | 同 `setting_tab.rs` 模式，`mod home_tab;` 单文件 |
| `crates/terminal_view/src/view.rs` | 5515 | hot path 32x，Render impl + cx.listener 深度耦合 |
| `main/src/setting_tab.rs` | 4455 | 已中停 2 次（commit f7884478 / state.json failed） |
| `crates/db_view/src/table_designer_tab.rs` | 4446 | UI 模式同上 |
| `crates/sftp_view/src/lib.rs` | 4136 | 库入口 + UI 混合 |
| `crates/terminal/src/terminal.rs` | 3549 | alacritty_terminal 包装，外部 trait 表面广 |
| `crates/core/src/tab_container.rs` | 3412 | 全局状态机，多 `cx.listener` 闭包 |
| `crates/one_ui/src/edit_table/state.rs` | 3182 | Editable State 状态机 |
| `crates/db_view/src/table_data/data_grid.rs` | 3174 | hot path 12x |
| `crates/db_view/src/table_data/results_delegate.rs` | 3028 | delegate 模式 |
| `crates/db/src/*/plugin.rs` (×6) | 1700-3800 | 各数据库 driver 适配，跨文件 trait 一致性 |

**总数**: 18 个 `mod xxx;` 形式的"巨无霸单文件"——按经验 3 都不能在 zero-ask 模式下硬拆。

### 🟡 内部有 `mod tests` 块，可能仅拆测试到 `_tests.rs`

| 文件 | 备注 |
|---|---|
| `crates/db_view/src/sql_editor_completion_tests.rs` (1730) | **已经是 tests 文件**，无 mod.rs 阻碍 |
| `crates/ui/src/input/state.rs` (2700) | 含 1 个内联 `mod tests` |
| `crates/ui/src/table/state.rs` (1993) | 含 1 个内联 `mod tests` |
| `crates/ui/src/input/element.rs` (1905) | 含 1 个内联 `mod tests` |
| `crates/ui/src/menu/popup_menu.rs` (1361) | 含 1 个内联 `mod tests` |

测试拆出**单文件可 fullauto 完成**（创建 `xxx/_tests.rs` + 路径调整 + 删原 mod tests）—— 但需先验证 5 个文件每个的 mod tests 是相对独立的。

### 🟢 已经是多文件目录结构（单文件可优化）

- `crates/db_view/src/table_data/` 内 `data_grid.rs` / `results_delegate.rs` / `filter_editor.rs` —— 内部已是模块化组织，但单文件仍 > 3000 行。可在不改外部 API 前提下逐步拆 impl 块。
- `crates/ui/src/dock/tiles.rs` (1223) / `tab_panel.rs` (1213) — 类似情形

## 优先级建议

按"低风险 / 高收益"原则，未来重构任务建议顺序：

| 优先级 | 任务 | 理由 | 风险 |
|---|---|---|---|
| **P0-1** | 拆 `crates/db_view/src/sql_editor_completion_tests.rs` 命名（已经是 tests 文件，路径迁移） | 仅 1 个 commit 的命名修正 + 路径迁移 | 极低 |
| **P0-2** | 在 `crates/ui/src/input/state.rs` 等 5 个文件**内部**把 `mod tests` 抽到 `_tests.rs` 子文件 | 不动生产代码，5 个独立 PR | 低 |
| **P1-1** | `crates/db_view/src/table_data/{data_grid,results_delegate,filter_editor}.rs` 拆 impl 块 | 已是目录结构，重复 PR 累积 | 中 |
| **P1-2** | `crates/one_ui/src/edit_table/state.rs` 拆 Editable State 状态机 | 3182 行集中 | 中 |
| **P2** | `main/src/home_tab.rs` (7525) | 最大单文件，需用 `#[path]` 绕过 mod.rs 限制 | 高 |
| **P2** | `crates/terminal_view/src/view.rs` (5515) | hot path，需详细 impact 分析 | 高 |
| **P2** | `main/src/setting_tab.rs` (4455) | 9 轮路线图已制定（commit f7884478） | 高 |
| **P3** | 各数据库 driver plugin（6 个文件） | trait 一致性约束，需要并行改动 | 高 |
| **P3** | `sftp_view` / `mongodb_view` / `redis_view` 多个文件 | 整体性拆分 | 中-高 |

## 风险与权衡

- **本普查本身不触发任何代码改动**，仅作决策依据
- **AGENTS.md:387 硬门禁**：不应被理解为"必须立刻拆完所有 > 300 行文件"——而是"新增/修改时遵守，存量渐进收敛"
- **`#[path]` 绕过 mod.rs 限制**（经验 3 提到）值得在 P1-1 / P1-2 验证可行性
- **本普查本身属于 fullauto 适用场景**：纯数据 + 文档，0 代码风险

## 附表 A：完整 > 1000 行文件清单（81 个）

| 行数 | 路径 |
|---:|---|
| 7525 | `main/src/home_tab.rs` |
| 5515 | `crates/terminal_view/src/view.rs` |
| 4455 | `main/src/setting_tab.rs` |
| 4446 | `crates/db_view/src/table_designer_tab.rs` |
| 4136 | `crates/sftp_view/src/lib.rs` |
| 3780 | `crates/mongodb_view/src/collection_view.rs` |
| 3756 | `crates/db/src/mysql/plugin.rs` |
| 3589 | `crates/db_view/src/db_tree_event.rs` |
| 3549 | `crates/terminal/src/terminal.rs` |
| 3412 | `crates/core/src/tab_container.rs` |
| 3300 | `crates/redis_view/src/key_value_view.rs` |
| 3211 | `crates/db/src/plugin.rs` |
| 3182 | `crates/one_ui/src/edit_table/state.rs` |
| 3174 | `crates/db_view/src/table_data/data_grid.rs` |
| 3028 | `crates/db_view/src/table_data/results_delegate.rs` |
| 3017 | `crates/db/src/mssql/plugin.rs` |
| 3015 | `crates/terminal_view/src/sidebar/file_manager_panel.rs` |
| 2944 | `crates/db_view/src/db_tree_view.rs` |
| 2875 | `crates/db_view/src/common/db_connection_form.rs` |
| 2765 | `crates/db/src/postgresql/plugin.rs` |
| 2700 | `crates/ui/src/input/state.rs` |
| 2670 | `crates/db/src/manager.rs` |
| 2637 | `crates/db/src/oracle/plugin.rs` |
| 2607 | `crates/redis_view/src/redis_tree_view.rs` |
| 2325 | `crates/db_view/src/table_data/filter_editor.rs` |
| 2303 | `crates/db_view/src/chatdb/chat_panel.rs` |
| 2100 | `crates/redis_view/src/redis_cli_view.rs` |
| 2071 | `crates/terminal_view/src/sidebar/server_monitor_panel.rs` |
| 2019 | `crates/core/src/storage/repository.rs` |
| 1993 | `crates/ui/src/table/state.rs` |
| 1952 | `crates/db/src/clickhouse/plugin.rs` |
| 1905 | `crates/ui/src/input/element.rs` |
| 1828 | `crates/db/src/sqlite/plugin.rs` |
| 1816 | `crates/sftp_view/src/file_list_panel.rs` |
| 1797 | `crates/mongodb_view/src/mongo_tree_view.rs` |
| 1779 | `crates/terminal_view/src/ssh_form_window.rs` |
| 1751 | `crates/core/src/storage/models.rs` |
| 1746 | `crates/redis_view/src/connection.rs` |
| 1742 | `crates/ssh/src/ssh.rs` |
| 1730 | `crates/db_view/src/sql_editor_completion_tests.rs` |
| 1716 | `crates/db/src/duckdb/plugin.rs` |
| 1670 | `crates/db/src/postgresql/connection.rs` |
| 1642 | `crates/terminal_view/src/terminal_element.rs` |
| 1549 | `crates/sftp/src/russh_impl.rs` |
| 1497 | `crates/terminal/src/ssh_backend.rs` |
| 1482 | `crates/terminal_view/src/sidebar/settings_panel.rs` |
| 1480 | `crates/db_view/src/sql_editor.rs` |
| 1404 | `crates/db_view/src/sql_editor_view.rs` |
| 1381 | `crates/db_view/src/import_export/table_import_view.rs` |
| 1381 | `crates/db_view/src/database_objects_tab.rs` |
| 1367 | `crates/core/src/cloud_sync/generic_sync.rs` |
| 1361 | `crates/ui/src/menu/popup_menu.rs` |
| 1361 | `crates/db_view/src/import_export/table_export_view.rs` |
| 1343 | `crates/ui/src/text/node.rs` |
| 1296 | `crates/core/src/ai_chat/panel.rs` |
| 1270 | `crates/db_view/src/sql_result_tab.rs` |
| 1269 | `crates/terminal_view/src/addon.rs` |
| 1223 | `crates/ui/src/dock/tiles.rs` |
| 1213 | `crates/ui/src/dock/tab_panel.rs` |
| 1190 | `crates/one_ui/src/edit_table/view.rs` |
| 1166 | `crates/db_view/src/import_export/csv_importer.rs` |
| 1148 | `crates/redis_view/src/redis_data_view.rs` |
| 1119 | `crates/remote_file_editor/src/lib.rs` |
| 1104 | `crates/db/src/sqlite/connection.rs` |
| 1090 | `crates/redis_view/src/key_view.rs` |
| 1080 | `crates/db_view/src/import_export/import_export_manager.rs` |
| 1071 | `crates/sftp_view/src/file_editor.rs` |
| 1062 | `crates/db_view/src/database_form.rs` |
| 1053 | `crates/terminal_view/src/terminal_view.rs` |
| 1050 | `crates/db_view/src/import_export/json_importer.rs` |
| 1045 | `crates/db_view/src/connection_form_dialog.rs` |
| 1037 | `crates/db_view/src/ssh_tunnel_dialog.rs` |
| 1031 | `crates/db_view/src/import_export/sql_importer.rs` |
| 1023 | `crates/sftp_view/src/file_view.rs` |
| 1020 | `crates/db/src/mysql/connection.rs` |
| 1018 | `crates/terminal_view/src/terminal_panel.rs` |
| 1008 | `crates/redis_view/src/redis_cli_panel.rs` |
| 1004 | `crates/db_view/src/import_export/excel_importer.rs` |
| 1001 | `crates/db_view/src/import_export/json_exporter.rs` |

## 附表 B：按 crate 拆解的 1000+ 文件分布

```
db_view:        16 个文件，35,811 行
db:             15 个文件，30,515 行
ui:             12 个文件，17,141 行
terminal_view:   7 个文件，16,773 行
main:            4 个文件，14,197 行
core:            7 个文件，12,012 行
redis_view:      5 个文件，10,908 行
mongodb_view:    3 个文件， 6,697 行
sftp_view:       2 个文件， 5,952 行
terminal:        2 个文件， 5,046 行
story:           3 个文件， 3,341 行
one_ui:          1 个文件， 3,182 行
ssh:             1 个文件， 1,742 行
sftp:            1 个文件， 1,549 行
remote_file_editor: 1 个文件， 1,119 行
```

## 验证

- 普查命令: `find /home/hoping/htdocs/onetcli -path target -prune -o -path vendor -prune -o -type f -name '*.rs' -print | xargs wc -l | sort -rn`
- 总计行数: 317,339（与 `awk` 计算一致）
- > 600 行文件数: 146
- > 1000 行文件数: 81
- > 1500 行文件数: 45
