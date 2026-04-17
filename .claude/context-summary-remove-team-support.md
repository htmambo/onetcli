## 项目上下文摘要（remove-team-support）
生成时间：2026-03-26 13:18:00 +0800

### 1. 相似实现分析
- **实现1**: `crates/core/src/cloud_sync/service.rs`
  - 模式：所有同步类型共用 `CloudSyncService` 做 blob 加解密和 `CloudSyncData` 组装。
  - 可复用：`prepare_sync_data_upload(...)`、`prepare_workspace_sync_data_upload(...)`、`prepare_certificate_sync_data_upload(...)`
  - 需注意：当前 `team_id` 贯穿加密、解密和 `key_version` 选择，删除时必须同步改 trait 签名和调用方。

- **实现2**: `crates/core/src/storage/repository.rs`
  - 模式：连接和凭证通过 `ConnectionRow` / `CertificateRow` 做数据库列到模型的统一映射。
  - 可复用：现有 `owner_id` 保持不变，`team_id` 可直接从 SQL 与模型中剔除。
  - 需注意：初始化 schema 和增量 migration 都含团队字段，不能只改运行时代码。

- **实现3**: `crates/core/src/cloud_sync/engine.rs`
  - 模式：`SyncEngine` 负责同步前准备、运行所有 handler、应用冲突解决。
  - 可复用：现有 `ensure_unlocked()` / `ensure_personal_key_config()` 保留。
  - 需注意：当前同步启动会拉团队列表并缓存，删除团队支持时要同时清理缓存字段、日志和 `CloudApiClient` 团队接口。

### 2. 项目约定
- **命名约定**: Rust 结构体和 trait 使用 PascalCase，方法和字段使用 snake_case。
- **文件组织**: 同步模型在 `crates/core/src/cloud_sync/`，持久化模型和仓库在 `crates/core/src/storage/`，各窗口保存逻辑分散在对应 crate 的 `*_form_window.rs`。
- **导入顺序**: 先 `std`，再第三方 crate，最后项目内模块；同文件内尽量合并同路径导入。
- **代码风格**: 错误信息和注释使用简体中文；同步失败统一返回 `SyncError` 或 `CloudApiError`。

### 3. 可复用组件清单
- `crates/core/src/cloud_sync/service.rs`: 同步数据的加解密与上传组装入口
- `crates/core/src/cloud_sync/sync_type.rs`: 通用同步 trait
- `crates/core/src/storage/repository.rs`: 本地数据库读写与仓库注册
- `crates/core/src/storage/migration.rs`: migration 注册顺序

### 4. 测试策略
- **测试框架**: Rust 内置 `cargo test`
- **测试模式**: 以 `one-core` 的单元测试和定向回归测试为主
- **参考文件**: `crates/core/src/cloud_sync/service.rs`、`crates/core/src/cloud_sync/engine.rs`
- **覆盖要求**: 至少覆盖同步服务加解密、冲突解决、迁移后仓库编译链路

### 5. 依赖和集成点
- **外部依赖**: `rusqlite`、`serde`、`gpui`
- **内部依赖**: `CloudApiClient` trait 被 `sync_server` 客户端和 `engine.rs` 中的测试 mock 同时实现
- **集成方式**: `SyncEngine` 调用 `SyncTypeHandler`；UI 表单直接构造 `StoredConnection` / `Certificate`
- **配置来源**: 本地 SQLite schema 来自 `crates/core/migrations/` 与 `20260225000001_init.sql`

### 6. 技术选型理由
- **为什么用这个方案**: 直接移除团队支持比继续保留空实现更符合当前产品实际，也能消除误导日志和无用字段。
- **优势**: 同步协议、数据库模型和 UI 状态更简单，减少无效分支和误报。
- **劣势和风险**: 属于破坏性清理，需要同时更新新库 schema 与旧库迁移脚本，避免运行时列不匹配。

### 7. 关键风险点
- **边界条件**: 旧数据库可能已有 `team_id` 列和 `team_key_cache` 表，需要迁移移除。
- **编译风险**: `CloudApiClient` trait 改签名后，所有实现和 mock 都必须同步更新。
- **行为风险**: `can_edit_connection(...)` 原先含团队权限分支，删掉后要保证连接卡片仍可编辑。
