# DataGrid 列可见性筛选

**状态**: ✅ 已完成 (完成时间: 2026-04-10)

## 目标

DataGrid 工具栏添加列可见性按钮，允许用户勾选/取消勾选要展示的列。主键列始终可见不可取消。配置本地持久化（不同步到远程）。

## 改动文件

| 文件 | 改动 |
|------|------|
| `crates/db_view/src/table_data/data_grid.rs` | 工具栏按钮 + 菜单 + 隐藏状态持久化加载/保存 |
| `crates/db_view/src/table_data/results_delegate.rs` | 可见列映射 + `render_th`/`render_td`/`column`/`columns_count`/`perform_sort` 索引映射 |
| `crates/ui/src/icon.rs` | 新增 `IconName::ListCheck` 图标 |
| `crates/assets/assets/icons/list-check.svg` | 图标资源（用户提供） |
| `crates/db_view/locales/db_view.yml` | i18n 翻译 |
| `crates/core/src/storage/models.rs` | `KeyValue` model（已有） |
| `crates/core/src/storage/repository.rs` | `KeyValueRepository`（get_by_key/set/delete） |
| `crates/core/src/storage/migration.rs` | 注册 key_values 表迁移 |
| `crates/core/migrations/20260410000002_key_value.sql` | 建表 |

## 关键设计点

- **列过滤在 Delegate 层**：通过 `visible_column_indices: Vec<usize>` 维护可见列索引映射
- **所有访问点使用映射**：`columns_count()`、`column()`、`render_td`、`render_th`、`perform_sort` 均通过 `map_visible_to_original()` 获取真实列索引
- **持久化**：SQLite `key_values` 表，key 为 `column_visibility:{connection_id}:{database_name}:{table_name}`，value 为隐藏列名的 JSON 数组
- **菜单状态**：直接从 delegate 的 `visible_column_indices` 推导隐藏列，避免异步加载时的状态不一致

## 备注

- `update_data()` 会清空 `visible_column_indices`，由调用方决定是否重新应用
- 工具栏菜单使用 `IconName::ListCheck` 图标
