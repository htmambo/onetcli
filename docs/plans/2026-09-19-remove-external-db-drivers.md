# 彻底移除外部数据库驱动（IPC / DatabaseType::External）支持 — 设计说明

- **日期**：2026-09-19
- **状态**：✅ 已完成 Phase 1+2（2026-09-20，方案 A：保留 External 变体兜底存量数据，
  能力全部移除；Phase 3 数据清理留远期评估）
- **关联**：`2026-09-18-fix-round-01.md` B4（收敛版闸门已随 ipc 子系统一并删除）

## 1. 背景

原内置的 DuckDB / MSSQL / Oracle / ClickHouse 驱动已从代码中移除，存量连接在
反序列化时统一落入 `DatabaseType::External` 变体（`crates/core/src/storage/models.rs:105`，
`#[serde(other)]` 兜底）。第三方数据库（如达梦 Dameng）通过**外部驱动子进程**
（`~/.config/omnihub/ipc-drivers/<driver>/driver.json` + 本地 socket JSON-RPC）接入，
即 `crates/db/src/ipc/` 子系统。

产品决策：彻底移除该扩展点，数据库支持收敛到内置 MySQL / PostgreSQL / SQLite。

## 2. 现状盘点（移除范围清单）

### 2.1 核心代码（整目录删除）

| 路径 | 行数 | 说明 |
|---|---|---|
| `crates/db/src/ipc/` | 4219 | client / connection / display / plugin / protocol / registry + AGENTS.md |
| `crates/db/tests/ipc_concurrency.rs` | - | 集成测试 |
| `crates/db/tests/ipc_mock_driver.rs` | - | 集成测试（mock 驱动） |

### 2.2 引用点（逐处清除）

**crates/db**
- `manager.rs:183-192`：`External` 分支与 `ExternalDatabasePlugin` 单例
- `plugin.rs`、`executor.rs`、`data_transfer/precheck.rs`：各 1 处 External 匹配分支
- `lib.rs`：`pub mod ipc` 及 re-export

**crates/db_view**（表单 / 树 / 数据网格的 External 分支）
- `database_view_plugin.rs`（3 处）、`db_tree_view.rs`（2 处）、
  `connection_form_window.rs`（2 处，含达梦标题测试）、`data_grid.rs`（2 处）、
  `database_tab.rs`、`table_designer_tab.rs:3475`、`filter_types.rs`
  （`IdentifierQuote::from_database_type` 的 External 匹配臂）

**main**
- `new_connection/connection_kind.rs`：外部驱动分类（`IpcDriverRegistry::load_default()`）
- `new_connection/form_page.rs:50`：External 表单入口
- `external_driver_display.rs`：驱动图标/展示（整文件删除）

**crates/core**
- `storage/models.rs`（5 处）：`DatabaseType::External` 变体本身、`as_str`、
  图标、以及两处 `#[serde(other)]` 兜底逻辑（见 §3 兼容性）

**测试**：上述文件内的 External 相关用例 + `crates/db/tests/` 两个 ipc 文件。

### 2.3 不涉及的

- 云同步：连接同步为通用 JSON 透传，无 External 专门逻辑（已 grep 确认）
- sync_server：零改动
- Redis / MongoDB / SSH / SFTP：与 IPC 驱动无关

## 3. 兼容性关键问题：`#[serde(other)]` 兜底

这是整个清理中**唯一有数据风险**的点：

- 现状：任何未知 database_type 字符串（含已删除的 `"DuckDB"` 等历史值）反序列化
  时都落入 `External`，连接可正常出现在列表里（连不上但数据不丢）
- 若直接删除 `External` 变体：存量连接反序列化**失败**，可能导致连接列表
  加载报错或数据丢失；云同步拉取到旧类型连接时同样失败

**必须二选一的迁移策略**：

- **方案 A（推荐）：保留变体，移除能力**。`DatabaseType::External` 与 serde 兜底
  保留，但不再可连接、不可新建；UI 显示为"已停止支持"的只读条目（图标置灰 +
  提示文案），允许用户删除。风险最小，可一个版本内完成。
- **方案 B：数据迁移后删除变体**。先写一个 migration 把存量 External 连接标记
  删除或转成墓碑记录，确认同步链路不再产生该类型后，再删除变体与兜底。
  需要跨版本窗口期（参考 A1 私钥迁移的方案 A 窗口期模式），周期长。

## 4. 分阶段方案（按方案 A）

1. **Phase 1：入口封堵**。新建连接页不再列出外部驱动分类
   （`connection_kind.rs` 删除 registry 扫描）；存量 External 连接在列表中
   置灰 + 提示"该数据库类型已停止支持"。此阶段即可发布。
2. **Phase 2：代码移除**。删除 `crates/db/src/ipc/`、`external_driver_display.rs`、
   两个集成测试与各引用点；`DatabaseType::External` 变体保留但所有
   `get_plugin(External)` 返回明确错误。
3. **Phase 3（可选，远期）**：观察 1-2 个版本后评估是否执行方案 B 的数据清理。

## 5. 风险与待决策问题

1. ~~**是否有真实用户在使用外部驱动**（达梦等）~~ → **已确认（2026-09-19）**：
   产品侧确认未使用达梦及任何外部驱动，移除无内部阻塞。仍建议 release notes
   提及弃用，以防下游用户自行放置过 ipc-drivers。
2. **onetcli-extensions 生态**：外部驱动若在扩展仓有发布物，需要同步下架/归档
3. **i18n 文案**：新增"已停止支持"提示需三语言（en/zh-CN/zh-HK），
   走各 crate 自己的 locales

## 6. 验证门禁

- `cargo check -p db -p db_view -p main -p one-core` 通过
- `cargo test -p db -p db_view` 全绿（移除后注意 External 相关用例同步删除）
- `cargo fmt --check` 干净
- 存量验证：手工构造一个 `database_type` 为 `"DuckDB"` 的连接记录，
  确认升级后列表正常展示（置灰）且不崩溃、可删除
- i18n：`t!()` 提取脚本核对无裸 key

## 7. 实施结果（2026-09-20）

实际落地点与 §2 清单的差异：

- **两个 `.expect` panic 炸弹**（`database_view_plugin.rs` 的 `create_connection_form`
  与 `manifest_plugin`）：改为返回 `Option` / 兜底空 Vec / Default，6 个 pub 入口
  全部防御化；存量 External 连接右键菜单为空，删除走首页列表入口（不依赖 plugin）
- **编辑/复制拦截**：在 `home_tab.rs` 的 `confirm_edit_connection` /
  `duplicate_connection_and_open_editor` 开头对 External 弹通知并 return，
  表单层不再可达 External
- **存量提示**：`connection_subtitle` 对 External 显示"外部驱动（已停止支持）"；
  连接打开时 `get_plugin(External)` 返回 `DbError::NotSupported`，经既有
  error_nodes 通路在树节点行尾显示警告三角 + 可复制 Popover
- **国产数据库分类**：`NewConnectionCategory::DomesticDatabase` 仅服务外部驱动，
  已一并移除（分类收敛为 4 个）
- **依赖清理**：`crates/db/Cargo.toml` 删除仅服务 ipc 的 `interprocess` /
  `serde_ignored` / `ipc` 三个依赖
- **存量兼容测试**：`models.rs::external_fallback_tests` 钉死 `"DuckDB"` / `"External"`
  反序列化落入 External 变体的兜底行为
- 新增 i18n：`Error.external_drivers_unsupported`（db）、
  `Home.external_driver_unsupported` / `Home.external_driver_subtitle`（main），
  均三语言
- 验证：check（5 crate）Finished；db 404+3 全绿；db_view 310 通过
  （3 个 sql_editor_completion 失败为既有）；main 104 通过
  （1 个 global_proxy locale 断言失败为既有，stash 对照确认）；fmt 干净

**遗留**：Phase 3（存量 External 连接的数据清理/墓碑化）观察 1-2 个版本后再评估；
release notes 需提及外部驱动弃用。
