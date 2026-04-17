# 云同步服务端轻量重设计方案（账号 + 设备授权）

生成时间：2026-03-24 16:21:31 +0800

## 1. 重设计结论

你当前真正需要的不是“复杂 SaaS 平台”，而是一个**同一账号、多设备、多平台自动同步**的轻量云同步服务。

因此方案应收敛为：
- **账号**：继续使用 `Supabase Auth`
- **授权**：新增“设备授权”层，而不是 Pro/License
- **同步**：继续使用 `user_configs` + `sync_data`
- **范围**：只同步个人配置和个人数据，不做团队、支付、订阅、后台

一句话概括：

> 用“账号确认是谁”，用“设备授权确认这台设备能不能同步”，用 `sync_data` 负责真正的跨端内容同步。

## 2. 目标与边界

### 2.1 目标
- 支持同一账号在 macOS、Windows、Linux 等多个平台登录
- 支持登录后自动同步个人配置
- 支持设备级授权与撤销
- 支持按内容类型做同步范围控制
- 保持服务端足够简单，优先快速上线

### 2.2 当前必须做
- 账号登录
- 会话恢复
- 主密钥配置同步
- 配置/工作区/连接自动同步
- 设备注册
- 设备授权预检
- 设备撤销

### 2.3 当前明确不做
- Pro / Free 区分
- License 授权
- 订阅、支付、订单
- 团队共享与成员管理
- 后台管理系统
- 复杂审计与报表
- 单独的业务 BFF

## 3. 与当前代码的真实关系

### 3.1 已经具备的能力
- `main/src/auth.rs` 已有账号登录、会话恢复、令牌刷新
- `crates/core/src/cloud_sync/supabase.rs` 已有 `user_configs`、`sync_data` 的 Supabase 读写
- `crates/core/src/cloud_sync/models.rs` 已有统一加密 blob 同步模型
- `crates/core/src/cloud_sync/engine.rs` 已有自动同步主链路

### 3.2 还缺什么
- 没有设备唯一标识 `device_id`
- 没有设备注册接口
- 没有设备授权状态
- 没有设备撤销能力
- 没有“同步前做授权预检”的逻辑
- 如果要同步“应用设置”，还缺一个新的 `data_type`

### 3.3 当前不需要强行兼容的部分
- `teams` / `team_members`
- `rpc/add_team_member_by_email`
- `model_list`
- `ai-proxy`

说明：
- 当前 `SyncEngine::sync()` 在 `list_teams()` 失败时会降级为只同步个人数据
- 所以第一阶段完全可以只做个人同步闭环
- 团队能力后续如果不需要，可以持续不启用

## 4. 推荐架构

## 4.1 主推荐方案
- **认证**：Supabase Auth
- **数据库**：Supabase Postgres
- **数据访问**：PostgREST
- **轻量授权扩展**：Postgres RPC
- **部署方式**：Supabase Cloud

这是当前最合适的方案，因为：
- 与现有客户端协议最接近
- 实现成本最低
- 不需要额外维护一套后端服务

## 4.2 为什么不建议现在上 BFF
- 你的核心需求不是复杂业务编排
- 设备授权只需要轻量状态管理
- 现阶段引入 NestJS / Go / Fastify 只会提高维护成本

只有当你后面明确需要以下能力时，再考虑 BFF：
- 强制设备级服务端拦截每一次同步请求
- 复杂后台
- 支付或商业化
- 多租户/组织管理

## 5. 核心设计：账号 + 设备授权

## 5.1 设计原则
- **账号决定身份**
- **设备决定授权**
- **内容决定同步范围**

也就是说：
- 用户登录成功，只说明“这个人是谁”
- 设备授权通过后，才说明“这台设备可以同步什么”
- 同步内容仍然走统一的 `sync_data`

## 5.2 设备授权不是 License

这里的“授权”含义是：
- 这台设备是否被当前账号允许参与同步
- 这台设备可以同步哪些内容类型

这里不再包含：
- 付费授权
- 套餐授权
- Pro 功能授权

## 5.3 推荐授权策略

第一阶段建议采用最简单策略：
- 首次登录时自动注册设备
- 默认授权该设备参与同步
- 账号侧保留“撤销设备”能力
- 每次同步前先检查设备是否仍为 `active`

这样做的好处是：
- 用户体验简单
- 服务端实现轻
- 仍然保留了“设备可撤销”的控制点

## 6. 数据模型

## 6.1 保留表：`user_configs`

用途：
- 保存用户主密钥验证信息

建议字段：
- `id bigint generated always as identity primary key`
- `user_id uuid not null unique`
- `key_verification text not null`
- `key_version integer not null default 1`
- `updated_at timestamptz not null default now()`

## 6.2 保留表：`sync_data`

用途：
- 保存所有待跨端同步的密文数据

建议字段：
- `id uuid primary key default gen_random_uuid()`
- `owner_id uuid not null`
- `data_type text not null`
- `encrypted_data text not null`
- `key_version integer not null default 1`
- `checksum text not null default ''`
- `version integer not null default 1`
- `last_modified_by_device_id text null`
- `created_at timestamptz not null default now()`
- `updated_at timestamptz not null default now()`
- `deleted_at timestamptz null`

### `data_type` 第一阶段建议值
- `connection`
- `workspace`
- `app_settings`

说明：
- `connection`、`workspace` 与当前代码一致
- 如果你要同步“配置信息”，建议直接增加 `app_settings`
- 不要为设置单独新建一张表，继续复用统一 blob 模型

## 6.3 新增表：`device_authorizations`

用途：
- 记录当前账号下有哪些设备被允许参与同步

建议字段：
- `id uuid primary key default gen_random_uuid()`
- `user_id uuid not null`
- `device_id text not null`
- `device_name text not null`
- `platform text not null`
- `app_version text null`
- `status text not null default 'active'`
- `granted_scopes text[] not null default array['connection','workspace','app_settings']::text[]`
- `first_authorized_at timestamptz not null default now()`
- `last_seen_at timestamptz not null default now()`
- `last_sync_at timestamptz null`
- `created_at timestamptz not null default now()`
- `updated_at timestamptz not null default now()`

建议约束：
- `unique(user_id, device_id)`
- `status in ('active', 'revoked')`

## 6.4 为什么只新增这一张表

因为这已经足够解决你的核心问题：
- 一个账号多台设备
- 每台设备有独立状态
- 可撤销
- 可控制同步范围

不需要再加：
- `user_subscriptions`
- `teams`
- `team_members`
- `device_tokens`
- `device_audit_logs`

## 7. 最小接口设计

## 7.1 继续保留的现有接口
- `/auth/v1/...`
- `/rest/v1/user_configs`
- `/rest/v1/sync_data`

## 7.2 新增 RPC：`register_device`

用途：
- 登录后注册当前设备
- 如果设备已存在则刷新状态和心跳

输入建议：
- `p_device_id`
- `p_device_name`
- `p_platform`
- `p_app_version`
- `p_requested_scopes`

输出建议：
- `device_id`
- `status`
- `granted_scopes`

## 7.3 新增 RPC：`check_sync_access`

用途：
- 每次同步前校验当前设备是否仍然允许同步

输入建议：
- `p_device_id`

输出建议：
- `allowed boolean`
- `status text`
- `granted_scopes text[]`

## 7.4 新增 RPC：`revoke_device`

用途：
- 用户在账号页面撤销某台设备

输入建议：
- `p_device_id`

输出建议：
- `success boolean`

## 7.5 为什么推荐 RPC 而不是 Edge Function

因为当前需求足够简单：
- 设备注册本质上就是一次数据库 upsert
- 设备预检本质上就是一次状态查询
- 设备撤销本质上就是一次状态更新

这些都很适合放在 Postgres RPC，不需要额外的函数网关层

## 8. 自动同步策略

## 8.1 同步触发点

建议保留以下 5 个触发点：
1. 启动后恢复会话成功
2. 用户输入主密钥解锁成功
3. 本地配置发生变更
4. 应用从后台回到前台
5. 定时轮询

## 8.2 推荐时机
- 启动恢复成功：立即做一次增量同步
- 解锁主密钥后：立即做一次完整同步
- 本地变更后：3 到 5 秒防抖上传
- 轮询：60 到 300 秒一次
- 网络恢复后：立即补一次增量同步

## 8.3 同步顺序
1. 检查会话有效
2. 调用 `check_sync_access`
3. 如果未授权，则停止同步并提示设备已被撤销
4. 如果已授权，则开始同步 `user_configs`
5. 再同步 `app_settings`
6. 再同步 `workspace`
7. 最后同步 `connection`

## 8.4 为什么把 `app_settings` 放前面
- 它通常体量最小
- 冲突代价最低
- 对多端体验感知最明显

## 9. “智能同步内容”应该怎么做

你提到更倾向于“账号 + 授权”来**智能同步内容**，这里建议不要把“智能”理解成复杂规则引擎，而是做成这三层：

### 9.1 第一层：按设备是否授权
- 未授权设备：不能参与同步
- 已授权设备：允许参与同步

### 9.2 第二层：按设备授权范围
- 全量设备：同步 `app_settings`、`workspace`、`connection`
- 轻量设备：只同步 `app_settings`

### 9.3 第三层：按内容类型决定合并方式
- `app_settings`：最后写入优先，必要时按字段合并
- `workspace`：继续沿用当前 `version + checksum`
- `connection`：继续沿用当前 `version + checksum`

### 9.4 第一阶段推荐的现实做法
- 全部桌面端设备默认使用同一组范围
- 不做复杂平台差异策略
- 只保留“是否可同步”与“是否同步设置”两个层级

这样已经足够满足 90% 的使用场景

## 10. 访问控制建议

## 10.1 `user_configs`
- 仅当前用户可读写自己的数据

## 10.2 `sync_data`
- 第一阶段仅按 `auth.uid()` 控制归属
- 不做设备级硬阻断

原因：
- 当前客户端直连 PostgREST
- 设备 ID 还没有进入每次 `sync_data` 请求
- 如果强行在第一阶段把设备授权做成每请求强校验，会明显扩大客户端改造面

## 10.3 `device_authorizations`
- 当前用户可读自己的设备列表
- 当前用户可撤销自己的设备
- 当前用户只能注册到自己的账号下

## 10.4 设备授权的强度分级

### 当前推荐：轻量阻断
- 登录后 `register_device`
- 同步前 `check_sync_access`
- 若设备已撤销，则客户端不再发起同步

### 后续增强：严格阻断
- 给每次同步请求带 `device_id`
- 或把 `sync_data` 全部改走 RPC / BFF
- 服务端对每个同步请求强校验设备状态

结论：
- **第一阶段做轻量阻断最合适**
- **只有明确需要强安全控制时，才升级到严格阻断**

## 11. 客户端最小改造清单

虽然这份文档重点是服务端，但要落地“账号 + 设备授权”，客户端至少要补以下内容：

### 11.1 新增本地持久化 `device_id`
- 首次启动生成一次
- 后续一直复用
- 卸载重装可视为新设备

### 11.2 登录成功后注册设备
- OTP 登录成功
- 密码登录成功
- 会话恢复成功

这三种入口都要调用 `register_device`

### 11.3 每次同步前做授权预检
- 调用 `check_sync_access`
- 非 `active` 直接停止同步

### 11.4 如果需要同步应用设置
- 新增 `app_settings` 的序列化和上传/下载逻辑
- 继续走 `sync_data`

## 12. 开发分期

## 第 1 期：最小闭环
- 建 `device_authorizations` 表
- 建 `register_device` / `check_sync_access` / `revoke_device` RPC
- 完成 `user_configs` / `sync_data` / `device_authorizations` 的 RLS
- 完成 `sync_data.version` 自动递增触发器

交付结果：
- 服务端已经能支撑“账号 + 设备授权 + 个人同步”

## 第 2 期：客户端接入
- 本地生成并持久化 `device_id`
- 登录/恢复会话后注册设备
- 自动同步前做授权预检
- 撤销设备后的提示与停止同步

交付结果：
- 同一账号双端可自动同步
- 被撤销设备无法继续参与同步

## 第 3 期：体验补强
- 设备列表页
- 设备名称编辑
- 最近同步时间展示
- 只同步设置的轻量设备策略

交付结果：
- 方案可运营、可管理，但仍保持轻量

## 13. 验收清单

- 同一账号在 macOS 与 Windows 登录后都能看到各自设备记录
- 第 1 台设备新建连接后，第 2 台设备能自动拉到
- 第 2 台设备修改工作区后，第 1 台设备能自动拉到
- `app_settings` 修改后可跨端生效
- 撤销某台设备后，该设备下一次同步前预检失败
- 设备撤销不影响其他已授权设备
- 断网恢复后会自动补同步
- 版本冲突不会导致数据静默丢失

## 14. 最终推荐

如果你的目标真的是：
- 同账号多端使用
- 自动同步配置
- 不想做复杂额外功能

那么最合适的落地方式就是：

### 推荐落地组合
- `Supabase Auth`
- `user_configs`
- `sync_data`
- `device_authorizations`
- 3 个 RPC：`register_device`、`check_sync_access`、`revoke_device`

### 不建议现在做的东西
- `user_subscriptions`
- `teams`
- `team_members`
- 支付回调
- BFF
- 后台管理系统

### 一句话建议
- **把“授权”从 Pro/License 改成“设备授权”**
- **把“同步”收敛成个人配置同步**
- **把实现方式收敛成 Supabase 表 + RPC**

这就是当前最简单、最贴合你实际需求的方案。
