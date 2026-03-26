## 项目上下文摘要（connection-remote-delete-conflict）
生成时间：2026-03-26 16:18:00 CST

### 1. 相似实现分析
- **实现1**: `crates/core/src/cloud_sync/connection_sync.rs:29`
  - 模式：连接同步仍使用独立同步流程，包含待删除、软删除、同步计划和冲突解决。
  - 可复用：现有 `sync_connections()` 主流程、`process_cloud_soft_deleted_sync_data()` 和 `calculate_sync_plan()`。
  - 需注意：本地列表在软删除后没有刷新，后续计划仍拿旧快照。

- **实现2**: `crates/core/src/cloud_sync/generic_sync.rs:502`
  - 模式：通用同步在处理云端软删除时，会先判断本地是否存在未同步更新，必要时跳过删除。
  - 可复用：`should_keep_local_item_on_cloud_delete()` 体现了“删除和更新并发时以更新为准”的判定方式。
  - 需注意：连接同步没有复用这套逻辑，导致语义漂移。

- **实现3**: `crates/core/src/storage/repository.rs:689`
  - 模式：工作区和证书仓库都区分“本地更新”和“云端回写更新”，分别使用 `update()` 与 `update_from_cloud()`。
  - 可复用：`update_from_cloud()` 会保留云端更新时间和 `last_synced_at`，避免刚同步完又被判成本地修改。
  - 需注意：连接仓库缺少这套 API，当前云端回写仍走普通 `update()`。

- **实现4**: `crates/core/src/cloud_sync/service.rs:500`
  - 模式：工作区和证书在解密云端数据时，会把 `updated_at` 与 `last_synced_at` 一起回填成本地秒级时间。
  - 可复用：连接解密也应保持同样的时间基线。
  - 需注意：当前连接解密把 `last_synced_at` 设成毫秒时间且 `updated_at` 为空，存在量纲和基线问题。

### 2. 项目约定
- **命名约定**: 同步辅助函数使用 `process_*`、`calculate_*`、`update_from_cloud` 风格。
- **文件组织**: 连接同步策略在 `crates/core/src/cloud_sync/connection_sync.rs`，仓库时间戳处理在 `crates/core/src/storage/repository.rs`。
- **导入顺序**: 先标准库，再第三方 crate，最后项目模块。
- **代码风格**: 继续沿用简体中文日志前缀，如 `[同步计划]`、`[软删除]`。

### 3. 可复用组件清单
- `crates/core/src/cloud_sync/generic_sync.rs::should_keep_local_item_on_cloud_delete`
- `crates/core/src/storage/repository.rs::WorkspaceRepository::update_from_cloud`
- `crates/core/src/storage/repository.rs::CertificateRepository::update_from_cloud`
- `crates/core/src/cloud_sync/engine.rs` 测试中的 `MockCloudClient` / `setup_test_storage` 模式

### 4. 测试策略
- **测试框架**: Rust `#[test]` + `tokio::runtime::Runtime`
- **测试模式**: 连接同步回归测试 + 仓库层时间基线测试
- **参考文件**: `crates/core/src/cloud_sync/engine.rs` 现有冲突解决测试，`crates/core/src/storage/repository.rs` 现有仓库测试
- **覆盖要求**: 覆盖“远端软删除后不再生成假冲突”和“从云端回写时保留同步时间基线”

### 5. 依赖和集成点
- **外部依赖**: `tokio`、`async_trait`
- **内部依赖**: `connection_sync -> ConnectionRepository -> CloudSyncService`
- **集成方式**: 连接同步先处理远端软删除，再按刷新后的本地快照计算计划
- **配置来源**: 无新增配置

### 6. 技术选型理由
- **为什么优先修连接同步而不是改 UI**: 问题根因在核心同步计划和时间戳基线，UI 只是被动展示冲突结果。
- **优势**: 修复后 Web 删除、客户端自动同步和冲突对话框三条路径都会一起恢复正常。
- **劣势和风险**: 连接同步是独立实现，修复时要避免与现有手动冲突解决 API 行为打架。

### 7. 关键风险点
- **并发问题**: 删除后若本地存在真实未同步修改，仍需保留“更新优先”语义，不能直接删本地。
- **边界条件**: 软删除后必须刷新本地快照，否则会基于已删除连接继续算冲突。
- **性能瓶颈**: 远端删除命中时会额外重读一次本地连接列表，但成本很低。
- **安全考虑**: 无新增安全逻辑。
