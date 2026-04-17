## 项目上下文摘要（workspace-sync-upload）
生成时间：2026-03-26 09:54:59 +0800

### 1. 相似实现分析
- **实现1**: `crates/core/src/cloud_sync/connection_sync.rs:548-578`
  - 模式：连接同步不直接比较本地/云端时间，而是比较 `updated_at` 是否晚于 `last_synced_at`
  - 可复用：`local_changed / cloud_changed` 判定思路
  - 需注意：同步成功后必须回写 `last_synced_at`

- **实现2**: `crates/core/src/cloud_sync/certificate_sync.rs:92-117`
  - 模式：上传成功后通过 `repo.update_sync_status(...)` 一次性回写 `cloud_id + last_synced_at`
  - 可复用：同步完成后的状态落库方式
  - 需注意：如果只更新 `cloud_id`，后续无法稳定识别“已同步”

- **实现3**: `crates/core/src/storage/repository.rs:694-789`
  - 模式：工作区的本地更新入口统一走 `WorkspaceRepository`
  - 可复用：将“本地修改后进入待同步状态”的逻辑直接落在仓储层
  - 需注意：云端回写和本地编辑必须分开处理，避免互相覆盖

### 2. 项目约定
- **命名约定**: 同步状态统一使用 `cloud_id`、`last_synced_at`、`updated_at`
- **文件组织**: 数据模型在 `storage/models.rs`，数据库读写在 `storage/repository.rs`，同步编排在 `cloud_sync/*`
- **导入顺序**: 维持现有模块内排序，未额外引入新依赖
- **代码风格**: 优先在通用同步层和仓储层补齐状态，不在 UI 或调用方打补丁

### 3. 可复用组件清单
- `crates/core/src/cloud_sync/generic_sync.rs`: 通用同步计划与上传/下载执行流
- `crates/core/src/cloud_sync/sync_type.rs`: 各同步类型统一抽象
- `crates/core/src/storage/repository.rs`: 工作区仓储与现有 `update_sync_status` 模式
- `crates/core/src/storage/migration.rs`: 本地数据库迁移注册入口

### 4. 测试策略
- **测试框架**: Rust 内置单元测试
- **测试模式**: 仓储回归测试 + 同步判定纯函数测试
- **参考文件**: `crates/core/src/storage/repository.rs`、`crates/core/src/cloud_sync/generic_sync.rs`
- **覆盖要求**: 覆盖同步状态回写、本地编辑清空同步状态、同时间戳下本地优先上传

### 5. 依赖和集成点
- **外部依赖**: 无新增依赖
- **内部依赖**:
  - `Workspace` 模型新增 `last_synced_at`
  - `WorkspaceRepository` 负责持久化与清理同步状态
  - `WorkspaceSyncType` 负责上传成功后的状态回写
  - `generic_sync` 负责统一比较与更新云端后的回调
- **配置来源**: `crates/core/migrations/*.sql`

### 6. 技术选型理由
- **为什么用这个方案**: 现有工作区同步已经接入通用流程，最小安全修复应补齐同步状态字段并复用现有回写模式
- **优势**: 改动集中、兼容现有同步链路、可通过单元测试直接覆盖
- **劣势和风险**: 旧数据首次升级后，部分已绑定云端但缺少 `last_synced_at` 的工作区可能触发一次补偿上传

### 7. 关键风险点
- **边界条件**: 本地编辑与云端更新时间落在同一秒时，必须偏向本地，避免漏传
- **状态一致性**: 更新云端成功后如果不回写同步状态，会导致后续重复上传
- **迁移兼容**: 既要补新迁移，也要更新初始化建表 SQL，避免新库缺字段
- **工具限制**: 仓库规范要求优先使用 `sequential-thinking`、`desktop-commander`、`context7`、`github.search_code`，当前执行环境未提供，只能基于源码检索、`cargo` 本地验证与 `.claude` 留痕完成修复
