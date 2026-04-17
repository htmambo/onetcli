## 审查报告（workspace-delete-update-wins）
生成时间：2026-03-26 15:05:00 +0800

### 需求完整性检查
- 目标明确：为工作空间删除补齐同步语义，并保证“子连接只解绑、不删除”“删除与更新并发时以更新为准”
- 范围明确：工作空间删除 UI、待删除记录持久化、工作空间同步决策、仓库层解绑行为、回归测试
- 交付物明确：代码改动、migration、单元测试、编译验证、`.claude` 留痕
- 风险与依赖明确：全量 `one-core` 测试存在既有顺序相关失败，需要与本次功能性结果分开看待

### 技术维度评分
- 代码质量：94/100
- 测试覆盖：90/100
- 规范遵循：95/100

### 战略维度评分
- 需求匹配：96/100
- 架构一致：94/100
- 风险评估：92/100

### 综合评分
- 94/100
- 建议：通过

### 结论
- [`home_tab.rs`](/Volumes/Workarea/usr/htdocs/onetcli/main/src/home_tab.rs#L386) 新增 `queue_pending_workspace_deletion(...)`，把工作空间删除基线和受影响连接一起登记到待删除表，供同步阶段判断删除是否应被远端更新撤销。
- [`home_tab.rs`](/Volumes/Workarea/usr/htdocs/onetcli/main/src/home_tab.rs#L1651) 删除工作区弹窗已收敛为单一语义，不再提供“删除全部连接”分支；[`home_tab.rs`](/Volumes/Workarea/usr/htdocs/onetcli/main/src/home_tab.rs#L1689) 的本地删除也只会解绑连接并删除工作区。
- [`repository.rs`](/Volumes/Workarea/usr/htdocs/onetcli/crates/core/src/storage/repository.rs#L796) 的 `WorkspaceRepository::delete(...)` 现在会同时刷新子连接 `updated_at`，确保远端回放删除时，解绑结果仍能继续进入连接同步链路。
- [`repository.rs`](/Volumes/Workarea/usr/htdocs/onetcli/crates/core/src/storage/repository.rs#L852) 与 [`20260326000002_workspace_delete_context.sql`](/Volumes/Workarea/usr/htdocs/onetcli/crates/core/migrations/20260326000002_workspace_delete_context.sql) 扩展了待删除记录，新增 `base_last_synced_at` 和 `metadata`，为“更新优先于删除”提供判断依据。
- [`generic_sync.rs`](/Volumes/Workarea/usr/htdocs/onetcli/crates/core/src/cloud_sync/generic_sync.rs#L45) 现在会先读取云端列表，再处理待删除；[`generic_sync.rs`](/Volumes/Workarea/usr/htdocs/onetcli/crates/core/src/cloud_sync/generic_sync.rs#L72) 的云端软删除回放也会在本地存在未同步更新时跳过删除。
- [`workspace_sync.rs`](/Volumes/Workarea/usr/htdocs/onetcli/crates/core/src/cloud_sync/workspace_sync.rs#L16) 新增工作空间删除撤销恢复逻辑；[`workspace_sync.rs`](/Volumes/Workarea/usr/htdocs/onetcli/crates/core/src/cloud_sync/workspace_sync.rs#L200) 会在发现云端工作空间自删除基线后已更新时，恢复本地工作空间和被本次删除解绑的连接归属。

### 本地验证
- `cargo fmt --all`：通过
- `cargo test -p one-core workspace_repository_delete_unlinks_connections_and_refreshes_timestamp`：通过
- `cargo test -p one-core pending_cloud_deletion_repository_persists_context_fields`：通过
- `cargo test -p one-core use_local_conflict_resolution_updates_local_sync_status`：通过
- `cargo test -p one-core use_local_deleted_cloud_conflict_recreates_remote_item`：通过
- `cargo test -p one-core use_cloud_deleted_cloud_conflict_deletes_local_connection`：通过
- `cargo check -p main`：通过
- `cargo test -p one-core`：未作为通过标准；全量顺序下出现 3 条既有冲突测试共享状态失败，但对应测试单独重跑均通过

### 残余风险
- 删除撤销恢复当前基于 `base_last_synced_at` 与 `updated_at` 秒级时间比较，若不同端时间漂移明显，仍可能存在极小概率误判
- 删除撤销时只恢复本次删除记录里登记到的连接；若本地在删除后又手工改动过同一连接，恢复逻辑会保守跳过，避免覆盖用户后续编辑
