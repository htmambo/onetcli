## 项目上下文摘要（cloud-sync-server-plan）
生成时间：2026-03-24 15:35:05 +0800

### 1. 相似实现分析
- **实现1**: `crates/core/src/config.rs:88`
  - 模式：Supabase 配置通过 `SUPABASE_URL` 与 `SUPABASE_ANON_KEY` 注入，优先运行时环境变量，回退编译时环境变量
  - 可复用：服务端地址发现逻辑、客户端部署约束
  - 需注意：仓库内没有写死云同步服务器地址，不能臆造具体域名或机房

- **实现2**: `crates/core/src/cloud_sync/supabase.rs:208`
  - 模式：客户端把云端协议固定为 Supabase 三类入口：`/auth/v1`、`/rest/v1`、`/functions/v1`
  - 可复用：Auth、PostgREST、RPC/Edge Functions 的服务端分层
  - 需注意：服务端只要偏离这些路径约定，现有客户端就无法直接兼容

- **实现3**: `crates/core/src/cloud_sync/models.rs:203`
  - 模式：连接与工作区统一落在 `sync_data` 表，通过 `data_type` 区分，业务明文整体加密后存入 `encrypted_data`
  - 可复用：统一表模型、`checksum` 冲突判定、`version` 乐观并发、`deleted_at` 软删除
  - 需注意：服务端不能把连接和工作区拆成两套独立表，否则会破坏现有同步客户端

- **实现4**: `crates/core/src/cloud_sync/engine.rs:147`
  - 模式：同步前先拉团队列表，再同步工作区与连接；团队数据依赖服务端返回的团队与成员关系
  - 可复用：`teams` / `team_members` 的最小服务端对象集
  - 需注意：团队角色缓存依赖 `list_team_members`，团队 owner 最好在服务端显式出现在 `team_members`

- **实现5**: `crates/core/src/cloud_sync/supabase.rs:1530`
  - 模式：`list_sync_data` 默认不带 `team_id` 过滤，依赖服务端 RLS 自动过滤当前用户可见数据
  - 可复用：行级过滤设计、按 `updated_at` 增量拉取
  - 需注意：如果不配置 RLS，客户端会读到不属于当前用户或团队的数据

### 2. 项目约定
- **命名约定**: 云同步模型使用明确的表语义命名，如 `user_configs`、`user_subscriptions`、`sync_data`、`teams`、`team_members`
- **文件组织**: 协议抽象在 `crates/core/src/cloud_sync/client.rs`；Supabase 适配在 `crates/core/src/cloud_sync/supabase.rs`；同步流程在 `engine.rs`、`generic_sync.rs`、`connection_sync.rs`、`workspace_sync.rs`
- **代码风格**: 客户端假设服务端返回 RFC3339 时间字符串，并映射为本地时间戳
- **同步策略**: 个人数据与团队数据复用同一张 `sync_data` 表，通过 `owner_id` 与 `team_id` 区分归属

### 3. 可复用组件清单
- `crates/core/src/config.rs`：Supabase 配置读取方式
- `crates/core/src/cloud_sync/client.rs`：客户端要求的服务端接口契约
- `crates/core/src/cloud_sync/supabase.rs`：Supabase 表结构映射、查询参数、RPC 调用方式
- `crates/core/src/cloud_sync/service.rs`：blob 加密、校验和、`key_version` 选择逻辑
- `crates/core/src/cloud_sync/generic_sync.rs`：通用同步流程、软删除和增量拉取规则
- `crates/core/src/storage/models.rs`：本地工作区/连接与云端字段对应关系
- `crates/core/migrations/20260315000001_team_sync.sql`：本地团队同步缓存结构
- `crates/core/migrations/20260317000001_connection_owner.sql`：本地连接创建者字段

### 4. 测试策略
- **服务端验收重点**:
  - 认证成功后，`user_configs` / `user_subscriptions` / `sync_data` / `teams` / `team_members` 能被客户端直接读写
  - `sync_data` 更新必须支持版本冲突检测
  - 团队 owner、成员、个人数据三类访问边界必须符合客户端过滤预期
  - `deleted_at` 软删除后客户端仍能拉到删除标记并回放本地删除
- **建议验证方式**:
  - SQL 迁移验证
  - PostgREST / RPC 冒烟验证
  - 真实客户端联调验证

### 5. 依赖和集成点
- **认证依赖**: Supabase Auth，客户端使用邮箱密码、OTP、OAuth URL
- **数据依赖**: Supabase Postgres + PostgREST
- **团队扩展点**: `rpc/add_team_member_by_email` 需要访问 `auth.users`
- **订阅依赖**: `user_subscriptions` 需要由支付回调、管理后台或脚本维护，客户端只读
- **配置来源**: 实际服务端地址来自部署环境中的 `SUPABASE_URL`

### 6. 技术选型理由
- **为什么采用 Supabase**: 当前客户端已经把后端协议固化为 Supabase；继续复用官方能力能最小化改造成本
- **优势**:
  - 复用官方 Auth、PostgREST、RPC
  - 无需自研登录、会话、REST 层
  - 与现有 Rust 客户端直接兼容
- **劣势和风险**:
  - 具体服务端地域和域名不在仓库内，部署信息需要从环境变量或运维配置确认
  - 关键行为依赖数据库触发器和 RLS，若实现缺失会直接破坏同步一致性

### 7. 关键风险点
- **地址不透明**: 当前仓库与本机环境都未暴露 `SUPABASE_URL`，只能确定是某个 Supabase 项目实例
- **版本控制风险**: `update_sync_data` 依赖 `version=eq.<n>`，服务端必须在更新时自动递增版本号
- **团队角色风险**: 若创建团队时没有同步写入 owner 成员记录，客户端团队角色缓存会不完整
- **软删除风险**: 若服务端误做硬删除，客户端待删除回放和跨端删除同步会出现不一致
