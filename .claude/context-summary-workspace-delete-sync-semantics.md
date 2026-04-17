## 项目上下文摘要（workspace-delete-sync-semantics）
生成时间：2026-03-25 16:17:25 +0800

### 1. 相似实现分析
- **实现1**: `main/src/home_tab.rs::delete_connection`
  - 模式：本地删除入口在 UI 层，删除后广播 `ConnectionDeleted`
  - 风险：此前先直接删云端、再删本地，删除语义分散在 UI 层

- **实现2**: `main/src/home_tab.rs::handle_delete_workspace`
  - 模式：工作区删除会同时处理工作区下的连接
  - 风险：此前连接和工作区都在 UI 层直接调用云端删除，和同步引擎的待删除机制并行，语义不统一

- **实现3**: `crates/core/src/certificate_manager.rs::delete_certificate`
  - 模式：本地删除后只登记 `PendingCloudDeletionRepository`，由同步引擎后续处理远端删除
  - 优点：删除语义统一由同步链路负责，和 `deleted_at` 识别闭环一致

- **实现4**: `crates/core/src/cloud_sync/generic_sync.rs`
  - 模式：同步开始先处理待删除队列，再拉取云端数据并识别 `deleted_at`
  - 结论：对工作区/连接也应尽量走这条统一路径

### 2. 技术结论
- 当前本地工作区/连接删除是物理删除，不存在本地 tombstone
- 远端 `sync_server` 删除语义是软删除，靠 `deleted_at` 保留 tombstone
- 最一致的策略是：本地物理删除成功后，登记待删除记录，并立即触发同步，让远端进入软删除状态
