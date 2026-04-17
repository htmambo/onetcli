# 云同步服务端开发方案

生成时间：2026-03-24 15:35:05 +0800

## 1. 结论先行

### 1.1 云同步服务器在哪里
- 当前客户端的云同步服务端不是写死在仓库中的固定地址，而是运行时或编译时注入的 `SUPABASE_URL`。
- 代码证据：
  - `crates/core/src/config.rs` 明确约定 `SUPABASE_URL` / `SUPABASE_ANON_KEY` 的读取优先级是“运行时环境变量 > 编译时环境变量”。
  - `crates/core/src/cloud_sync/supabase.rs` 用该 URL 拼接 `/auth/v1`、`/rest/v1`、`/functions/v1`。
- 本地结论：
  - 当前仓库源码中没有具体 Supabase 项目域名。
  - 当前执行环境中 `SUPABASE_URL` 与 `SUPABASE_ANON_KEY` 也为空。
  - 因此现在只能确认“服务器是某个 Supabase 项目实例”，不能确认其具体地域、机房或域名。

### 1.2 推荐实现路线
- 直接采用 **Supabase 官方托管架构** 作为云同步服务端。
- 不建议自研认证、REST API、会话、同步网关。
- 服务端开发的核心不是“写一个新后端”，而是“按当前客户端协议补齐 Supabase 的表、RLS、RPC、触发器、订阅回写”。

## 2. 目标与范围

### 2.1 目标
- 为当前客户端提供可直接兼容的云同步服务端。
- 支持个人同步与团队共享同步。
- 支持 Pro 用户订阅状态读取。
- 支持连接与工作区两类数据的统一同步。
- 保持与当前客户端零协议改造兼容。

### 2.2 范围
- 包含：Supabase 项目、数据库表、RLS、SQL RPC、必要触发器、订阅回写链路、联调与验收。
- 不包含：自研认证中心、自研网关、自研同步协议、自研对象存储。

## 3. 与现有客户端的兼容约束

### 3.1 必须兼容的 URL 规则
- 认证：`{SUPABASE_URL}/auth/v1/...`
- 数据：`{SUPABASE_URL}/rest/v1/...`
- 函数：`{SUPABASE_URL}/functions/v1/...`

### 3.2 客户端已经固定依赖的对象
- 表：`user_configs`
- 表：`user_subscriptions`
- 表：`sync_data`
- 表：`teams`
- 表：`team_members`
- RPC：`rpc/add_team_member_by_email`

### 3.3 客户端已经固定依赖的数据语义
- `sync_data` 是统一同步表，不再拆 `connections` / `workspaces` 两张云端业务表。
- `data_type` 当前至少支持：
  - `connection`
  - `workspace`
- `encrypted_data` 存的是整个业务对象的加密 blob，而不是局部字段。
- `checksum` 用于明文内容一致性/冲突识别。
- `version` 用于乐观并发控制。
- `deleted_at` 用于软删除。
- `team_id` 为 `NULL` 表示个人数据，非 `NULL` 表示团队共享数据。

## 4. 服务端总体架构

### 4.1 架构选型
- **Auth 层**：Supabase Auth
- **数据层**：Supabase Postgres
- **数据访问层**：Supabase PostgREST
- **服务扩展层**：Postgres RPC 为主，Edge Functions 仅用于支付回调或后台任务

### 4.2 组件职责
- **Supabase Auth**
  - 负责邮箱密码登录
  - 负责 OTP 验证
  - 负责 OAuth 跳转 URL
- **Postgres**
  - 持久化同步元数据、团队、订阅状态、用户密钥配置
- **PostgREST**
  - 承接客户端对表的直接 CRUD
- **RPC**
  - 负责 `add_team_member_by_email`
  - 负责需要访问 `auth.users` 或封装事务的一次性逻辑
- **支付回调/后台任务**
  - 负责把外部订阅状态回写进 `user_subscriptions`

### 4.3 推荐部署方式
- 第一优先：Supabase 官方托管版
- 区域选择：与主要用户所在区域一致
- 域名形态：`https://<project-ref>.supabase.co`
- 环境管理：开发、预发、生产三套独立项目或至少独立 schema/密钥

## 5. 数据模型设计

### 5.1 `user_configs`
- 用途：存储用户主密钥验证信息与密钥版本。
- 建议字段：
  - `id bigint generated always as identity primary key`
  - `user_id uuid not null unique`
  - `key_verification text not null`
  - `key_version integer not null default 1`
  - `updated_at timestamptz not null default now()`
- 约束：
  - `user_id` 唯一，支持客户端 `on_conflict=user_id` upsert。

### 5.2 `user_subscriptions`
- 用途：存储客户端可读取的订阅状态。
- 建议字段：
  - `id uuid primary key default gen_random_uuid()`
  - `user_id uuid not null unique`
  - `plan text not null`
  - `status text not null`
  - `expires_at timestamptz null`
  - `created_at timestamptz not null default now()`
  - `updated_at timestamptz not null default now()`
- 约束：
  - `user_id` 唯一。
- 说明：
  - 客户端只读，不应该允许普通用户自行写订阅状态。

### 5.3 `sync_data`
- 用途：统一存储工作区与连接的云端同步记录。
- 建议字段：
  - `id uuid primary key default gen_random_uuid()`
  - `owner_id uuid not null`
  - `team_id uuid null`
  - `data_type text not null`
  - `encrypted_data text not null`
  - `key_version integer not null default 1`
  - `checksum text not null default ''`
  - `version integer not null default 1`
  - `created_at timestamptz not null default now()`
  - `updated_at timestamptz not null default now()`
  - `deleted_at timestamptz null`
- 约束：
  - `data_type` 先限制为 `connection` / `workspace`
  - `owner_id` 必须存在
  - `team_id` 可空
- 说明：
  - 服务端只存密文 blob 和同步元数据，不解析业务字段。

### 5.4 `teams`
- 用途：团队元信息与团队级密钥版本。
- 建议字段：
  - `id uuid primary key default gen_random_uuid()`
  - `name text not null`
  - `owner_id uuid not null`
  - `description text null`
  - `key_verification text null`
  - `key_version integer not null default 1`
  - `created_at timestamptz not null default now()`
  - `updated_at timestamptz not null default now()`

### 5.5 `team_members`
- 用途：团队成员与角色映射。
- 建议字段：
  - `id uuid primary key default gen_random_uuid()`
  - `team_id uuid not null`
  - `user_id uuid not null`
  - `role text not null default 'member'`
  - `joined_at timestamptz not null default now()`
- 约束：
  - `unique(team_id, user_id)`
  - `role` 先限制为 `owner` / `member`

## 6. 必须补齐的数据库规则

### 6.1 `updated_at` 触发器
- 目标：所有可更新表在更新后自动刷新 `updated_at`。
- 涉及表：
  - `user_configs`
  - `user_subscriptions`
  - `sync_data`
  - `teams`

### 6.2 `sync_data.version` 自动递增触发器
- 目标：匹配客户端的乐观并发协议。
- 原因：
  - 客户端更新时会请求 `PATCH /sync_data?id=eq.<id>&version=eq.<current_version>`。
  - 客户端发送的 payload 不包含新版本号。
  - 因此数据库必须在成功更新时把 `version = old.version + 1`。
- 缺失后果：
  - 客户端会一直停留在旧版本号，下一次更新会错误冲突或覆盖逻辑异常。

### 6.3 团队 owner 成员自动补齐
- 目标：团队创建后，`team_members` 中自动生成一条 owner 记录。
- 原因：
  - 客户端缓存团队角色时会调用 `list_team_members`。
  - 若 owner 不在 `team_members` 中，角色缓存会不完整。
- 建议实现：
  - `after insert on teams` 触发器自动插入 `(team_id, owner_id, role='owner')`。

### 6.4 删除策略
- `sync_data` 只做软删除，不做硬删除。
- `teams` / `team_members` 是否允许硬删除可按业务决定，但必须保证不会破坏现有成员访问逻辑。

## 7. 索引设计

### 7.1 基础唯一约束
- `user_configs(user_id)`
- `user_subscriptions(user_id)`
- `team_members(team_id, user_id)`

### 7.2 读写热点索引
- `sync_data(owner_id, data_type, updated_at desc)`
- `sync_data(team_id, data_type, updated_at desc)`
- `sync_data(deleted_at)`
- `teams(owner_id, updated_at desc)`
- `team_members(team_id, joined_at asc)`

### 7.3 说明
- 客户端最常见的同步查询是：
  - 按 `data_type` 拉取
  - 按 `team_id` 拉取
  - 按 `updated_at` 增量拉取
- 因此索引设计必须优先服务这三类过滤。

## 8. 访问控制与过滤规则

本节只描述“为了兼容当前客户端必须具备的归属过滤逻辑”，不扩展自研鉴权系统。

### 8.1 `user_configs`
- 仅当前用户可读自己的行。
- 仅当前用户可写自己的行。

### 8.2 `user_subscriptions`
- 仅当前用户可读自己的订阅。
- 普通客户端用户不可直接写。
- 写入应来自支付回调、后台任务或服务角色。

### 8.3 `teams`
- 当前用户是 owner 或成员时可读。
- 仅 owner 可更新、删除团队。
- 创建团队时 `owner_id` 应与 `auth.uid()` 一致。

### 8.4 `team_members`
- 当前用户属于该团队时可读成员列表。
- 仅 owner 可添加或移除成员。
- `add_team_member_by_email` 内部也必须校验调用者是团队 owner。

### 8.5 `sync_data`
- 个人数据：
  - `team_id is null`
  - 仅 `owner_id = auth.uid()` 可见、可写
- 团队数据：
  - 调用者是团队 owner 或成员时可见
  - 创建时至少要校验调用者属于该团队
  - 更新/软删除时也要校验归属

## 9. 核心接口设计

### 9.1 客户端直连接口
- `GET /auth/v1/user`
- `POST /auth/v1/token?grant_type=password`
- `POST /auth/v1/signup`
- `POST /auth/v1/otp`
- `POST /auth/v1/verify`
- `POST /auth/v1/logout`
- `GET /rest/v1/user_configs`
- `POST /rest/v1/user_configs?on_conflict=user_id`
- `GET /rest/v1/user_subscriptions`
- `GET /rest/v1/sync_data?...`
- `POST /rest/v1/sync_data`
- `PATCH /rest/v1/sync_data?id=eq.<id>&version=eq.<version>`
- `PATCH /rest/v1/sync_data?id=eq.<id>`（软删除）
- `GET /rest/v1/teams`
- `POST /rest/v1/teams`
- `PATCH /rest/v1/teams?id=eq.<id>`
- `DELETE /rest/v1/teams?id=eq.<id>`
- `GET /rest/v1/team_members?team_id=eq.<id>`
- `POST /rest/v1/team_members`
- `DELETE /rest/v1/team_members?id=eq.<id>`

### 9.2 必做 RPC：`add_team_member_by_email`
- 输入：
  - `p_team_id text`
  - `p_email text`
- 输出：
  - 新增或已存在的 `team_members` 记录
- 服务端职责：
  - 校验调用者是否为团队 owner
  - 通过邮箱查找 `auth.users`
  - 拒绝不存在邮箱
  - 避免重复添加
  - 返回符合客户端字段结构的 JSON

### 9.3 订阅回写接口
- 不要求客户端直写。
- 建议由支付服务 webhook 或后台任务写入 `user_subscriptions`。
- 至少保证客户端在登录后调用 `get_subscription()` 能拿到最新记录。

## 10. 数据流与时序

### 10.1 首次启用云同步
1. 用户登录 Supabase Auth。
2. 客户端读取 `user_configs`，没有则初始化。
3. 客户端读取 `user_subscriptions`，决定是否解锁 Pro。
4. 客户端拉取团队列表与成员列表，建立本地团队缓存。
5. 客户端按 `workspace`、`connection` 两类依次同步 `sync_data`。

### 10.2 常规同步
1. 先处理本地待删除队列。
2. 拉取 `sync_data`，按 `data_type` 过滤。
3. 依赖服务端 RLS 自动过滤当前用户可见数据。
4. 客户端解密 blob，建立名称映射，做上传/下载/更新规划。
5. 更新时使用 `id + version` 做乐观并发控制。

### 10.3 冲突处理
1. 客户端通过 `checksum`、更新时间、版本差异识别冲突。
2. 用户选择 `UseCloud` / `UseLocal` / `KeepBoth`。
3. 服务端只需要保证版本控制、团队归属和软删除语义正确。

### 10.4 软删除回放
1. 客户端删除云端记录时，服务端只更新 `deleted_at`。
2. 其他客户端下次拉取到该记录后，在本地执行删除。
3. 不应该直接硬删除 `sync_data` 记录。

## 11. 分阶段实施计划

### 阶段 A：项目初始化
- 新建 Supabase 项目。
- 配置 `SUPABASE_URL`、`SUPABASE_ANON_KEY`、服务角色密钥。
- 打通 Auth 基础能力。

### 阶段 B：数据库建模
- 创建 `user_configs`
- 创建 `user_subscriptions`
- 创建 `sync_data`
- 创建 `teams`
- 创建 `team_members`
- 建立唯一约束与索引

### 阶段 C：数据库行为补齐
- 增加 `updated_at` 触发器
- 增加 `sync_data.version` 自增触发器
- 增加团队 owner 自动成员化触发器

### 阶段 D：访问控制与 RPC
- 配置 RLS
- 实现 `add_team_member_by_email`
- 用真实 JWT 验证个人/团队访问边界

### 阶段 E：订阅回写
- 接入支付回调或后台脚本
- 回写 `user_subscriptions`
- 验证登录后订阅可被客户端读取

### 阶段 F：联调与验收
- 使用当前客户端联调
- 验证首次同步、更新、冲突、软删除、团队共享
- 形成上线前验收记录

## 12. 云同步正确实现清单

- [ ] 已确定真实的 `SUPABASE_URL`，并能说明生产环境所在的 Supabase 项目与区域
- [ ] `SUPABASE_URL` / `SUPABASE_ANON_KEY` 已注入客户端运行环境或构建环境
- [ ] `user_configs` 已支持 `user_id` 唯一 upsert
- [ ] `user_subscriptions` 已能被客户端只读获取
- [ ] `sync_data` 已按统一表实现，而不是拆成多张业务表
- [ ] `sync_data.data_type` 至少支持 `connection` 与 `workspace`
- [ ] `sync_data.deleted_at` 已按软删除实现
- [ ] `sync_data.version` 已由数据库自动递增
- [ ] `sync_data.updated_at` 已由数据库自动更新
- [ ] `teams` 已支持创建、更新、删除、列表查询
- [ ] `team_members` 已支持查询、增加、删除
- [ ] 创建团队后 owner 会自动进入 `team_members`
- [ ] `rpc/add_team_member_by_email` 已实现并可返回成员记录
- [ ] 个人数据 RLS 已验证：用户只能看到自己的个人数据
- [ ] 团队数据 RLS 已验证：只有团队 owner/成员能看到团队数据
- [ ] 非团队成员无法读取或写入该团队 `sync_data`
- [ ] 订阅回写链路已验证：升级 Pro 后客户端可读到最新订阅状态
- [ ] 使用真实客户端完成以下联调：
  - [ ] 首次同步
  - [ ] 修改后再次同步
  - [ ] 跨端删除回放
  - [ ] 冲突解决
  - [ ] 团队共享连接同步
  - [ ] 团队共享工作区同步

## 13. 验收标准

### 13.1 功能验收
- 当前客户端不修改协议即可连上服务端。
- Free 用户无法通过订阅接口误拿到 Pro 状态。
- Pro 用户登录后能正常读取订阅并启用同步。
- 连接和工作区都能进入 `sync_data`。
- 团队 owner 可以添加成员并共享数据。

### 13.2 一致性验收
- 同一条 `sync_data` 连续更新时，版本号单调递增。
- 删除操作在多端都表现为软删除回放，而不是随机丢失。
- 团队数据不会泄露到非成员账号。

### 13.3 兼容性验收
- 兼容当前 Rust 客户端已有 `SupabaseClient` 实现。
- 不要求改造客户端 URL、表名、字段名。

## 14. 已知风险与对接建议

### 14.1 当前无法确认真实服务器位置
- 这是现状限制，不是架构问题。
- 需要从实际部署环境、打包配置或运维平台确认生产 `SUPABASE_URL`。

### 14.2 订阅状态回流要单独建设
- 当前客户端只读 `user_subscriptions`。
- 如果支付成功后服务端没有及时回写，客户端仍会表现为 Free。

### 14.3 团队 owner 记录不能漏
- 若 `team_members` 里没有 owner 记录，客户端团队角色缓存会不完整。

### 14.4 不要把云同步做成自研网关
- 当前客户端已经与 Supabase 协议强绑定。
- 自研网关只会放大维护成本和兼容风险。
