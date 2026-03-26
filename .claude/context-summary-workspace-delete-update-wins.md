## 项目上下文摘要（workspace-delete-update-wins）
生成时间：2026-03-26 15:05:00 +0800

### 1. 相似实现分析
- **实现1**: `main/src/home_tab.rs:1572`
  - 模式：工作空间删除入口位于 UI 层，当前支持“解绑连接”与“删除全部连接”两种分支。
  - 可复用：删除后统一走 `queue_pending_cloud_deletion(...)` 登记云端删除。
  - 需注意：现有“删除全部连接”分支与本次语义冲突，必须移除。

- **实现2**: `crates/core/src/storage/repository.rs:795`
  - 模式：`WorkspaceRepository::delete(...)` 会先把 `connections.workspace_id` 置空，再删除工作空间。
  - 可复用：仓库层已经具备“删除父对象时解绑子对象”的基础能力。
  - 需注意：当前只解绑，不会把子连接标记为待同步，远端回放删除后可能无法继续传播解绑结果。

- **实现3**: `crates/core/src/cloud_sync/generic_sync.rs:446`
  - 模式：通用同步流程先处理待删除，再根据云端 `deleted_at` 执行本地删除。
  - 可复用：工作空间已复用该流程，不需要重新造同步主流程。
  - 需注意：当前云端软删除优先级高于本地更新，不符合“更新优先于删除”。

- **实现4**: `crates/core/src/cloud_sync/connection_sync.rs:709`
  - 模式：连接同步依赖 `workspace_id <-> workspace.cloud_id` 映射恢复归属。
  - 可复用：工作空间同步完成后，连接同步会自动读取最新归属关系。
  - 需注意：如果工作空间删除被远端更新撤销，必须在连接同步前恢复本地连接归属。

### 2. 项目约定
- **命名约定**: 逻辑方法使用 `handle_*`、`process_*`、`restore_*` 风格；同步类型沿用 `*SyncType`。
- **文件组织**: UI 入口放 `main/src/home_tab.rs`，同步策略放 `crates/core/src/cloud_sync/`，存储结构放 `crates/core/src/storage/`。
- **导入顺序**: 先标准库，再外部 crate，最后项目模块；同组内按语义聚合。
- **代码风格**: 注释使用简体中文；同步日志沿用 `[同步]`、`[软删除]`、`[删除]` 前缀。

### 3. 可复用组件清单
- `main/src/home_tab.rs::queue_pending_cloud_deletion(...)`：统一登记待删除云端记录。
- `crates/core/src/storage/repository.rs::WorkspaceRepository::delete(...)`：工作空间删除与本地解绑基础能力。
- `crates/core/src/cloud_sync/generic_sync.rs::calculate_sync_plan(...)`：工作空间同步主计划计算。
- `crates/core/src/cloud_sync/service.rs::decrypt_sync_data_workspace(...)`：从云端记录恢复工作空间实体。
- `crates/core/src/storage/repository.rs::ConnectionRepository::list_by_workspace(...)`：按工作空间获取受影响连接。

### 4. 测试策略
- **测试框架**: Rust 内置 `#[test]` + `cargo test -p one-core`
- **测试模式**: 以仓库层和同步策略单元测试为主
- **参考文件**: `crates/core/src/storage/repository.rs` 现有 `workspace_repository_*` 测试
- **覆盖要求**: 覆盖本地删除解绑、待删除元数据保存、删除被远端更新撤销后的恢复

### 5. 依赖和集成点
- **外部依赖**: `rusqlite`、`serde`、`serde_json`
- **内部依赖**: `home_tab -> PendingCloudDeletionRepository -> generic_sync/workspace_sync -> ConnectionRepository`
- **集成方式**: 本地先删工作空间并登记待删除，再由同步引擎处理云端删除或撤销
- **配置来源**: 无新增配置，沿用现有同步地址与主密钥解锁流程

### 6. 技术选型理由
- **为什么用现有待删除机制扩展**: 现有删除链路已经围绕 `pending_cloud_deletions` 和 `deleted_at` 形成闭环，改动面最小。
- **优势**: 不需要新增全局同步模式，只扩展工作空间删除元数据与同步决策。
- **劣势和风险**: 需要新增 migration，并在同步开始阶段做一次更精细的待删除判断。

### 7. 关键风险点
- **并发问题**: 删除与更新并发时，如果不记录删除基线，会误删较新的云端工作空间。
- **边界条件**: 删除后如果远端更新胜出，必须在连接同步前恢复本地连接归属。
- **性能瓶颈**: 工作空间同步会在待删除判断阶段额外读取一次云端列表，但数据量通常较小。
- **安全考虑**: 无新增安全逻辑，本次仅调整同步与删除语义。
