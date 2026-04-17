## 项目上下文摘要（cloud-sync-server-complete-plan）
生成时间：2026-03-24 16:04:47 +0800

### 1. 相似实现分析
- **实现1**: `/.claude/cloud-sync-server-development-plan.md`
  - 模式：从 Supabase 兼容角度定义云同步服务端边界
  - 可复用：总体架构、统一 `sync_data` 表、RLS、RPC、触发器思路
  - 需注意：该文档仍包含 `user_subscriptions` / Pro 的旧设定

- **实现2**: `crates/core/src/cloud_sync/client.rs:1`
  - 模式：当前客户端协议已经收敛为认证、用户配置、模型列表、统一同步、团队管理、聊天
  - 可复用：当前真正生效的服务端接口边界
  - 需注意：`get_subscription()` 已删除，完整版方案应把商业化订阅降为可选扩展而非基础必选

- **实现3**: `crates/core/src/cloud_sync/supabase.rs:209`
  - 模式：URL 结构固定为 Supabase 的 `/auth/v1`、`/rest/v1`、`/functions/v1`
  - 可复用：服务端必须继续兼容 Supabase 协议
  - 需注意：即使做“增强版”架构，也不能破坏这组协议入口

- **实现4**: `crates/core/src/cloud_sync/engine.rs:147`
  - 模式：同步流程先拉团队，再同步工作区，再同步连接
  - 可复用：服务端的数据组织顺序和联调验收顺序
  - 需注意：团队能力是基础功能，不是附加功能

- **实现5**: `crates/core/src/cloud_sync/models.rs:203`
  - 模式：连接和工作区统一映射到 `sync_data`
  - 可复用：统一 blob、`checksum`、`version`、`deleted_at`
  - 需注意：任何完整版方案都不能把这层重新拆回多表业务模型

### 2. 项目约定
- **命名约定**: 以当前客户端实际接口为准，基础对象包括 `user_configs`、`sync_data`、`teams`、`team_members`
- **文件组织**: 协议抽象在 `cloud_sync/client.rs`，Supabase 适配在 `cloud_sync/supabase.rs`
- **代码风格**: 方案文档需要同时说明“基础版必选”和“商业化扩展可选”

### 3. 可复用组件清单
- `/.claude/cloud-sync-server-development-plan.md`
- `crates/core/src/cloud_sync/client.rs`
- `crates/core/src/cloud_sync/supabase.rs`
- `crates/core/src/cloud_sync/engine.rs`
- `crates/core/src/cloud_sync/models.rs`
- `/.claude/context-summary-cloud-sync-server-plan.md`
- `/.claude/context-summary-delete-license-module.md`

### 4. 测试策略
- 文档任务校验：
  - 检查新方案是否覆盖目标、范围、技术选型、分阶段实施、验收、风险、部署
  - 检查是否明确区分“当前基础版”和“可选商业化扩展”

### 5. 依赖和集成点
- 当前代码基础依赖：
  - `user_configs`
  - `sync_data`
  - `teams`
  - `team_members`
  - `rpc/add_team_member_by_email`
- 可选扩展：
  - `model_list`
  - `user_subscriptions`（未来商业化恢复时）

### 6. 技术选型理由
- **为什么需要重新整理完整版方案**: 原始文档偏“架构草案”，缺少技术选型矩阵、阶段交付、资源与时间规划
- **为什么要把订阅改成可选扩展**: 当前代码已删除 License 模块，完整版方案必须与当前仓库真实状态一致

### 7. 关键风险点
- **文档偏差风险**: 如果仍按旧文档把 `user_subscriptions` 写成基础必选，会和当前代码不一致
- **选型过散风险**: 需要给出 3-4 组清晰方案，而不是泛泛列工具
