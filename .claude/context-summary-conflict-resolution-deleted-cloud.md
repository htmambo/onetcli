## 项目上下文摘要（conflict-resolution-deleted-cloud）
生成时间：2026-03-26 11:14:46 +0800

### 1. 相似实现分析
- **实现1**: `crates/core/src/cloud_sync/engine.rs:384`
  - 模式：手动冲突解决统一走 `apply_conflict_resolutions(...) -> apply_single_conflict(...)`
  - 可复用：所有策略分支都应在引擎内按冲突类型做语义化处理
  - 需注意：原实现默认把 `conflict.cloud` 当成可更新、可解密的真实云端记录

- **实现2**: `crates/core/src/cloud_sync/conflict.rs:94`
  - 模式：`detect_local_modified_cloud_deleted(...)` 会构造占位 `CloudSyncData`
  - 可复用：`conflict_type == LocalModifiedCloudDeleted` 是区分“真实云端记录”和“占位删除冲突”的关键
  - 需注意：占位数据的 `encrypted_data=""`、`version=0`，不能继续走解密或更新接口

- **实现3**: `crates/core/src/cloud_sync/connection_sync.rs:768`
  - 模式：正常上传本地连接时会重新加密并创建云端记录，成功后回写 `cloud_id + last_synced_at`
  - 可复用：当云端记录已不存在时，“使用本地版本”应退化为重新创建云端记录
  - 需注意：若仍调用 `update_sync_data(...)`，服务端会因 `version=0` 拒绝请求

### 2. 项目约定
- **命名约定**: 冲突类型继续使用 `ConflictType::*`，同步状态推进继续复用 `mark_connection_synced`
- **文件组织**: 冲突判定在 `conflict.rs`，策略执行在 `engine.rs`
- **代码风格**: 不在 UI 里猜测冲突语义，全部下沉到同步引擎

### 3. 可复用组件清单
- `SyncEngine::ensure_personal_key_config(...)`
- `SyncEngine::mark_connection_synced(...)`
- `ConnectionRepository::delete(...)`
- `CloudSyncService::prepare_sync_data_upload(...)`

### 4. 测试策略
- **测试框架**: Rust 单元测试
- **测试模式**:
  - “云端已删除 + 使用本地版本”必须重建云端记录
  - “云端已删除 + 使用云端版本”必须删除本地记录
  - 保留原有 “BothModified + UseLocal” 回归测试
- **验证命令**:
  - `cargo test -p one-core use_local_deleted_cloud_conflict_recreates_remote_item --lib`
  - `cargo test -p one-core use_cloud_deleted_cloud_conflict_deletes_local_connection --lib`
  - `cargo test -p one-core use_local_conflict_resolution_updates_local_sync_status --lib`
  - `cargo check -p one-core -p main`

### 5. 依赖和集成点
- **内部依赖**:
  - `ConflictResolver::detect_local_modified_cloud_deleted(...)`
  - `SyncEngine::apply_single_conflict(...)`
  - `CloudApiClient::create_sync_data(...)`
  - `ConnectionRepository::update_sync_status(...)`
- **外部依赖**: 无新增依赖

### 6. 技术选型理由
- **为什么用这个方案**: 报错根因不是网络层，而是“云端已删除冲突”使用了占位 `CloudSyncData`，被误当成真实云端数据继续解密或更新
- **优势**: 修复点集中，只改冲突解决语义，不影响常规同步与真实云端记录更新
- **风险**: `KeepBoth` 在“云端已删除”场景下语义天然退化，本次按“保留本地并重建云端”处理

### 7. 关键风险点
- **删除语义**: “使用云端版本”在该场景下其实是接受云端删除，应删本地而不是解密占位数据
- **版本约束**: 占位 `version=0` 不能提交到 sync_server 的 `update` 接口
- **密钥状态**: 手动冲突解决也必须先同步 `user_config`，否则 `key_version` 可能仍为 0
