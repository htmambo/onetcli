# SQL 转储支持在 CREATE 前输出 DROP TABLE

**Status**: ✅ Completed (completion: 2026-11-12)，自动化验证与外部评审通过，待用户手工回放验证
**Created**: 2026-11-12
**Creator**: Claude Code
**Branch**: refactor/remove-external-db-drivers

## 1. 目标与背景

「转储 SQL」生成的脚本目前只有 CREATE TABLE + INSERT，无法直接覆盖已有 schema 回放。增加一个可选开关：在每张表的 CREATE 前输出 `DROP TABLE IF EXISTS`，让脚本可用于重建已有数据库。

## 2. 范围（用户已确认）

- 结构导出（StructureOnly / StructureAndData）时，可在每张表 CREATE 前输出 DROP。
- 在 SqlDumpView 弹窗提供复选框，默认关闭，行为零变更。
- 纯数据模式（DataOnly）不渲染复选框，导出层也不会输出 DROP。
- 不做 `SET FOREIGN_KEY_CHECKS`、事务包裹、DROP DATABASE / CREATE DATABASE / USE，不做其他方言适配（本分支仅 MySQL/PostgreSQL/SQLite 三个插件）。

## 3. 现状（代码事实）

- `crates/db/src/import_export/formats/sql.rs:287`：只输出 `-- Table structure for X` + CREATE + `-- Data for table X` + INSERT。
- `crates/db/src/import_export/mod.rs:115`：`ExportConfig` 无 drop 字段。
- `crates/db/src/plugin.rs:2278`：已有 `drop_table`，但其默认实现带 database 限定（MySQL 风格），与导出 CREATE 的限定方式不一致。
- `crates/db/src/plugin.rs:1610`：`format_export_table_reference(database, schema, table)` 是导出 DDL 使用的表名限定（schema.table 或 table，无 database），MySQL 的 `SHOW CREATE TABLE` 产物同样不带库名。导出 DROP 应与它保持一致。
- `crates/db_view/src/import_export/sql_dump_view.rs:249`：导出配置构造点；`:697` 起是 Start 按钮，开关在点击前读取。
- 另一处构造点：`crates/db_view/src/import_export/table_export_view.rs:589`（仅数据导出）。

## 4. 方案

### 4.1 配置

`ExportConfig` 增加 `drop_before_create: bool`（默认 `false`）。全仓仅两处构造点，均同步补字段。

### 4.2 导出层

- 新增纯函数 `build_structure_block(plugin, config, table, schema_sql) -> String`：
  - `schema_sql` 为空时返回空串（避免"只删不建"）。
  - 输出顺序：`-- Table structure for X` → 可选 DROP → CREATE → 空行。
  - DROP 使用 `format_export_table_reference`，与 CREATE 的限定方式一致；语句为 `DROP TABLE IF EXISTS {ref};`。
  - 首张成功导出的结构块前输出一次警示注释：`-- DROP TABLE enabled: replaying this file replaces existing tables.`。
- `export_with_progress` 的结构分支改调该函数；流式模式仍走 `StructureExported` 事件，事件模型不变。
- 纵深防御：DROP 仅在 `include_schema && drop_before_create` 且 `schema_sql` 非空时生成，DataOnly 无路径可触发。

### 4.3 UI

- `SqlDumpView` 增加 `include_drop: Entity<bool>`（默认 false）。
- 仅在结构类模式渲染一行：`Checkbox`（label「建表前先删除已有表」）+ 灰色小字提示「回放会替换已有表；含外键依赖时可能失败」。
- `start_dump` 读取该值写入 `ExportConfig::drop_before_create`。
- Checkbox 组件无 tooltip 能力，故用可见的辅助文字承载外键风险提示。
- 底部按钮区常驻关闭按钮：开始前显示「取消」，结束后显示「关闭」，避免只能靠 Esc 退出（2026-11-12 用户反馈补充）。

### 4.4 i18n

`crates/db_view/locales/db_view.yml` 的 `SqlDump` 下新增：
- `include_drop_label`
- `include_drop_hint`

### 4.5 测试

`crates/db/src/import_export/formats/sql.rs` 单测（不依赖连接）：
- 关闭开关：无 DROP，输出与现状一致。
- 开启 + MySQL（schema=None）：`DROP TABLE IF EXISTS \`t\`;` 在 CREATE 前。
- 开启 + PostgreSQL（schema=public）：`DROP TABLE IF EXISTS "public"."t";`。
- `schema_sql` 为空：即使开关开启也无 DROP。
- 首张表带警示注释，后续表不带。

## 5. 风险与缓解

- **外键依赖**：删除被引用表会失败，回放中断且留下中间态。用户明确排除外键开关；通过复选框辅助文字与文件内警示注释明示。
- **跨库回放**：MySQL 的 DROP/CREATE 都不带库名，在另一数据库回放只会影响当前库，不会误删源库表。
- **CREATE 失败时**：结构块整体为空，不输出 DROP，避免只删不建。
- **回滚**：默认关闭，revert 即可。

## 6. 子任务

1. ✅ `ExportConfig` 增加 `drop_before_create`，更新两处构造点
2. ✅ 抽出 `build_structure_block` + `StructureBlockBuilder` 并在导出流程接入
3. ✅ SqlDumpView 复选框、提示文字与配置透传
4. ✅ i18n 文案（en / zh-CN / zh-HK）
5. ✅ 单测（开关、方言一致性、空 CREATE、DataOnly、首块警示顺延）6/6 通过
6. ✅ 验证：`cargo test -p db --lib` 410 passed（SQL 导出 6/6）+ `cargo check -p db_view` + `cargo check -p main` 通过；`cargo fmt --check` 干净；clippy 无本次新增告警
7. 🔄 外部评审（code）已完成；待用户手工验证后归档

## 7. 验收标准

- 默认导出输出与现状完全一致（无 DROP）。
- 勾选后，每张表结构块为：注释 → DROP TABLE IF EXISTS → CREATE。
- DataOnly 模式无复选框、输出无 DROP。
- MySQL/PostgreSQL/SQLite 的 DROP 表名限定与各自 CREATE 一致。
- 单测通过，`cargo check -p db_view -p main` 通过。

## 8. 外部评审记录

- Requirement 评审（2026-11-12）：**APPROVED**（3 项前置条件）。已落实：DROP 改用 `format_export_table_reference` 与 CREATE 保持同一限定；文件与 UI 均提示外键阻塞；三方言 `IF EXISTS` 已核实。
- Plan 评审（2026-11-12）：**APPROVED**。已落实：警示文案明确外键风险；流式与非流式共用 `build_structure_block`，事件类型不变；首块判定改为"已发出的块"计数并补测试。
- Code 评审（2026-11-12）：**APPROVED**，6 项要求全部满足。跟进项处理：
  - P3-1 流式回归测试未补：构造 `DbConnection`/`DatabasePlugin` 完整 mock 成本过高；流式与非流式共用同一结构块生成函数，差异仅 `mem::take`，已在实现层与评审中确认。
  - P3-2 空 CREATE 与旧行为一致性：旧实现同样输出空块，已补断言固化。
  - P3-3 单 schema 假设：`resolve_sql_dump_target` 每个目标只对应一个 database 或一个 schema，不存在跨 schema 混导。
- 用户反馈（2026-11-12）：导出对话框缺少可见关闭入口，已改为底部常驻「取消/关闭」按钮。

## 9. 验收记录

- 自动化（2026-11-12）：`cargo test -p db --lib` 410 passed；SQL 导出单测 6/6；`cargo check -p db_view`、`cargo check -p main` Finished；`cargo fmt --check` 干净；clippy 无本次新增告警。
- 手工（待用户执行）：勾选后转储一次，确认输出含 DROP 且首块带警示注释；确认底部关闭按钮可用。
