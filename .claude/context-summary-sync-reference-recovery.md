## 项目上下文摘要（sync-reference-recovery）
生成时间：2026-03-25 20:08:00 +0800

### 1. 相似实现分析
- **实现1**: `crates/core/src/cloud_sync/workspace_sync.rs`
  - 模式：工作区通过 `WorkspaceSyncType` 接入 `generic_sync`，上传成功后把云端 `cloud_id` 回写到本地 `workspaces` 表。
  - 可复用：连接同步恢复工作区归属时，应复用工作区仓储中的 `cloud_id` 作为稳定映射键。
  - 需注意：工作区同步先于连接同步执行，连接恢复可以依赖工作区已经落地。

- **实现2**: `crates/core/src/cloud_sync/connection_sync.rs`
  - 模式：连接同步是手写专用流程，上传/下载/更新本地都集中在此文件。
  - 可复用：工作区引用的上传回填和下载恢复都应落在这里处理，而不是散落到 UI 或仓储层。
  - 需注意：当前上传和恢复都没有把 `workspace_cloud_id` 走通，导致跨端无法恢复工作区归属。

- **实现3**: `crates/core/src/storage/models.rs`
  - 模式：`CertificateReference` 已经同时保存 `local_id` 和 `cloud_id`，UI 恢复与连接参数快照同步都复用 `matches_certificate` / `sync_with_certificate`。
  - 可复用：凭证引用已经具备双 ID 结构，无需新增第二套引用模型。
  - 需注意：当前匹配逻辑会优先按 `local_id` 命中，跨设备时可能错误匹配到其他证书。

- **实现4**: `crates/core/src/storage/repository.rs`
  - 模式：`ConnectionRepository` 已有 `get_by_cloud_id`；`WorkspaceRepository` 仅有 `update_cloud_id`，缺少按 `cloud_id` 反查本地工作区的能力。
  - 可复用：新增 `WorkspaceRepository::get_by_cloud_id` 可与连接仓储的现有模式保持一致。
  - 需注意：恢复链路缺少这个查询能力时，只能把 `workspace_id` 留空。

### 2. 项目约定
- **命名约定**: 同步相关远端主键统一命名为 `cloud_id`
- **文件组织**: 数据模型在 `storage/models.rs` 和 `cloud_sync/models.rs`，同步流程在 `cloud_sync/*_sync.rs`
- **代码风格**: 优先在既有同步类型与仓储方法上增量扩展，不新建额外状态层

### 3. 可复用组件清单
- `crates/core/src/cloud_sync/workspace_sync.rs::on_uploaded`
- `crates/core/src/storage/models.rs::CertificateReference`
- `crates/core/src/storage/repository.rs::ConnectionRepository::get_by_cloud_id`
- `crates/core/src/cloud_sync/service.rs::prepare_sync_data_upload`

### 4. 测试策略
- **测试框架**: Rust 内联单元测试
- **参考位置**:
  - `crates/core/src/cloud_sync/service.rs` 现有 blob 加解密测试
  - `crates/core/src/storage/models.rs` 现有串口与模型 roundtrip 测试
- **本次覆盖**:
  - 连接同步明文数据必须保留 `workspace_cloud_id`
  - `CertificateReference` 在存在 `cloud_id` 时必须优先按 `cloud_id` 匹配

### 5. 依赖和集成点
- **外部依赖**: `serde_json`、`rusqlite`
- **内部依赖**:
  - `SyncEngine::sync` 的执行顺序：工作区 → 证书 → 连接
  - `sync_connections_for_certificate` 会在证书上传成功后回写连接参数中的证书引用
- **集成方式**: 连接上传前解析工作区 `cloud_id`，连接下载/更新本地时按工作区 `cloud_id` 反查本地工作区 `id`

### 6. 技术选型理由
- **为什么使用 `cloud_id`**: 它是跨设备稳定且唯一的远端标识，本地自增 `id` 只在单设备内有效
- **优势**: 不会因不同设备的本地 ID 重号而误恢复
- **风险**: 若上游工作区/证书尚未拿到 `cloud_id`，应显式保留空值或延后恢复，不能伪造映射

### 7. 关键风险点
- **误匹配风险**: 证书引用若继续优先按 `local_id` 匹配，跨设备可能套用错误证书
- **丢关联风险**: 连接若继续上传 `workspace_cloud_id = None`，同步后必然失去工作区归属
- **验证限制**: 当前没有完整端到端同步集成测试，本次以关键模型与匹配逻辑的单元测试兜底
