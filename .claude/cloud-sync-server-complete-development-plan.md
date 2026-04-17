# 云同步服务端完整开发方案

生成时间：2026-03-24 16:04:47 +0800

## 1. 文档说明

本文基于现有的 [cloud-sync-server-development-plan.md](/Volumes/Workarea/usr/htdocs/onetcli/.claude/cloud-sync-server-development-plan.md) 进一步扩展，补齐以下内容：
- 技术选型备选组
- 推荐方案与适用场景
- 完整的实施分期
- 交付物清单
- 部署与运维规划
- 联调与验收方案

同时需要说明一个现实差异：
- 原始文档仍包含 `user_subscriptions` / Pro 订阅能力
- 当前代码已经删除 License 授权模块
- 因此本文将方案拆成：
  - **基础版必选能力**：账号登录、个人同步、团队同步
  - **增强版可选能力**：模型管理、订阅/商业化、后台运营

## 2. 目标与范围

### 2.1 项目目标
- 为当前客户端提供一个可直接接入的云同步服务端
- 保持现有 Supabase 协议兼容
- 支持账号登录、用户配置、统一同步、团队共享
- 支持从“快速上线”平滑演进到“增强版运营平台”

### 2.2 基础版范围
- Supabase Auth
- `user_configs`
- `sync_data`
- `teams`
- `team_members`
- `rpc/add_team_member_by_email`
- SQL 迁移、RLS、触发器、索引
- 基础监控、备份、联调验收

### 2.3 可选增强范围
- `model_list`
- `user_subscriptions`
- 支付回调
- 后台管理服务
- 更完整的审计、报表、自动化任务

## 3. 当前客户端兼容边界

### 3.1 协议边界
- 认证：`{SUPABASE_URL}/auth/v1/...`
- 数据：`{SUPABASE_URL}/rest/v1/...`
- 扩展：`{SUPABASE_URL}/functions/v1/...`

### 3.2 当前基础版必选对象
- 表：`user_configs`
- 表：`sync_data`
- 表：`teams`
- 表：`team_members`
- RPC：`rpc/add_team_member_by_email`

### 3.3 当前增强版可选对象
- 表：`model_list`
- 表：`user_subscriptions`

### 3.4 当前必须保持的数据语义
- `sync_data` 使用单表统一存储连接与工作区
- `data_type` 至少支持：
  - `connection`
  - `workspace`
- `encrypted_data` 为整体加密 blob
- `checksum` 用于冲突识别
- `version` 用于乐观并发控制
- `deleted_at` 用于软删除回放
- `team_id = null` 表示个人数据

## 4. 技术选型备选组

### 方案 A：标准托管版（推荐）
- **定位**：最快上线、最小研发投入、优先兼容当前客户端
- **核心组合**：
  - 身份认证：Supabase Auth
  - 数据库：Supabase Postgres
  - 数据访问：PostgREST
  - 扩展逻辑：Postgres RPC + 少量 Edge Functions
  - 部署方式：Supabase Cloud
  - 运维方式：Supabase 控制台 + GitHub Actions + SQL Migration
  - 监控方式：Supabase Logs + Sentry
- **优点**：
  - 与当前客户端完全对齐
  - 上线速度最快
  - 研发和运维面最小
- **缺点**：
  - 后台运营能力偏弱
  - 复杂业务逻辑容易堆积在 SQL/RPC
- **适用场景**：
  - 先把同步能力稳定上线
  - 团队规模 1-5 人
  - 3-6 周内交付 MVP 到正式可用版本

### 方案 B：托管增强版（平衡型）
- **定位**：保留 Supabase 协议核心，同时引入轻量业务中台
- **核心组合**：
  - 身份认证：Supabase Auth
  - 数据库：Supabase Postgres
  - 数据访问：PostgREST
  - 扩展逻辑：NestJS / Fastify BFF
  - 后台任务：Supabase Edge Functions 或独立 Worker
  - 支付回调：BFF Webhook
  - 运维方式：Supabase + GitHub Actions + Docker 部署 BFF
  - 监控方式：Sentry + Grafana Cloud
- **优点**：
  - 复杂业务逻辑不必全部塞进 SQL
  - 后续接支付、后台、运营更顺手
  - 仍保持客户端协议兼容
- **缺点**：
  - 系统组件数量比方案 A 多
  - 初期成本和维护面上升
- **适用场景**：
  - 预计后续会加支付、套餐、后台运营
  - 团队规模 3-8 人
  - 希望基础版上线后继续迭代 SaaS 化能力

### 方案 C：自托管 Supabase 版（成本/私有化优先）
- **定位**：需要私有部署、内网部署或强成本控制
- **核心组合**：
  - 平台：Self-hosted Supabase
  - 编排：Docker Compose 或 Kubernetes
  - 网关：Kong / Caddy / Nginx
  - 数据库：PostgreSQL
  - 对象与日志：按需接 MinIO / Loki / Prometheus
  - 运维方式：自建 CI/CD + 迁移脚本
- **优点**：
  - 可私有化部署
  - 可自控网络与成本
  - 仍能复用 Supabase 协议模型
- **缺点**：
  - 运维复杂度显著上升
  - 故障排查与升级成本高
- **适用场景**：
  - 必须部署在自有服务器或企业网络
  - 有专职运维/平台能力
  - 对云托管依赖有明确限制

### 方案 D：企业增强版（高可用与运营中台）
- **定位**：中长期正式 SaaS / 企业版
- **核心组合**：
  - 身份认证：Supabase Enterprise 或托管 Supabase
  - 数据库：高可用 Postgres
  - 协议入口：PostgREST + RPC
  - 业务平台：Go / NestJS 后台服务
  - 任务系统：Temporal / BullMQ / Cloud Scheduler
  - 监控：Prometheus + Grafana + Sentry + OpenTelemetry
  - 配置与部署：Terraform + GitHub Actions / ArgoCD
- **优点**：
  - 最适合多团队协作、运营与报表
  - 演进空间最大
  - 更适合大规模用户和多环境治理
- **缺点**：
  - 初始建设周期最长
  - 组件复杂度最高
- **适用场景**：
  - 已确认商业化路线
  - 需要更强后台、报表、审计和多环境治理
  - 预计长期运营

## 5. 推荐结论

### 5.1 首选推荐
- **推荐采用方案 A 作为第一阶段正式落地方案**
- 原因：
  - 当前客户端协议已经与 Supabase 强绑定
  - 当前代码基础版只要求登录、用户配置、同步和团队管理
  - 方案 A 最适合尽快把服务端稳定落地

### 5.2 次选推荐
- **如果你明确要做商业化和后台，推荐方案 B**
- 原因：
  - 仍保留 Supabase 主协议
  - 可以把支付、后台、运营任务逐步转移到 BFF

### 5.3 不推荐作为第一阶段的方案
- **方案 C**：除非你明确需要私有化
- **方案 D**：除非你已经要做长期 SaaS 平台

## 6. 推荐方案 A 的完整架构

### 6.1 架构图文字版
- 客户端 `OnetCLI`
- `Supabase Auth`
- `Supabase PostgREST`
- `Supabase Postgres`
- `Postgres RPC`
- `Edge Functions（仅可选）`
- `Sentry`
- `GitHub Actions`

### 6.2 组件职责
- **Auth**
  - 邮箱密码登录
  - OTP 登录
  - OAuth 授权 URL
- **Postgres**
  - 保存同步元数据和团队关系
- **PostgREST**
  - 承接客户端表级 CRUD
- **RPC**
  - 封装 `add_team_member_by_email`
  - 封装需要访问 `auth.users` 的逻辑
- **Edge Functions**
  - 仅用于未来支付、后台任务、清理任务
- **Sentry**
  - 收集服务端异常
- **GitHub Actions**
  - 执行 SQL 迁移和部署脚本

## 7. 数据模型设计

### 7.1 基础版必选表

#### `user_configs`
- 用途：存储用户主密钥验证信息
- 字段建议：
  - `id bigint generated always as identity primary key`
  - `user_id uuid not null unique`
  - `key_verification text not null`
  - `key_version integer not null default 1`
  - `updated_at timestamptz not null default now()`

#### `sync_data`
- 用途：统一存储工作区与连接
- 字段建议：
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

#### `teams`
- 用途：团队元信息
- 字段建议：
  - `id uuid primary key default gen_random_uuid()`
  - `name text not null`
  - `owner_id uuid not null`
  - `description text null`
  - `key_verification text null`
  - `key_version integer not null default 1`
  - `created_at timestamptz not null default now()`
  - `updated_at timestamptz not null default now()`

#### `team_members`
- 用途：团队成员映射
- 字段建议：
  - `id uuid primary key default gen_random_uuid()`
  - `team_id uuid not null`
  - `user_id uuid not null`
  - `role text not null default 'member'`
  - `joined_at timestamptz not null default now()`

### 7.2 增强版可选表

#### `model_list`
- 用途：云端模型可用列表
- 建议放到第二阶段或第三阶段

#### `user_subscriptions`
- 用途：未来恢复商业化订阅时使用
- 当前不是基础版必选
- 只有在方案 B / D 并进入商业化阶段时再启用

## 8. 数据库规则与触发器

### 8.1 必选触发器
- `updated_at` 自动刷新触发器
- `sync_data.version` 自增触发器
- `teams -> team_members(owner)` 自动插入触发器

### 8.2 删除策略
- `sync_data` 必须软删除
- `deleted_at` 必须可被客户端重新拉取

### 8.3 索引建议
- `user_configs(user_id)`
- `sync_data(owner_id, data_type, updated_at desc)`
- `sync_data(team_id, data_type, updated_at desc)`
- `sync_data(deleted_at)`
- `teams(owner_id, updated_at desc)`
- `team_members(team_id, user_id)`

## 9. 访问控制规则

本节只保留“正确归属过滤”要求，不展开额外授权设计。

### 9.1 `user_configs`
- 当前用户只能读写自己的配置

### 9.2 `sync_data`
- 个人数据：仅 `owner_id = auth.uid()`
- 团队数据：当前用户必须属于该团队

### 9.3 `teams`
- owner 和成员可读
- 仅 owner 可更新、删除

### 9.4 `team_members`
- 团队成员可读成员列表
- 仅 owner 可添加或删除成员

## 10. RPC 与服务扩展

### 10.1 必做 RPC
- `rpc/add_team_member_by_email`

### 10.2 输入输出
- 输入：
  - `p_team_id text`
  - `p_email text`
- 输出：
  - 新建或已有的成员记录

### 10.3 行为要求
- 校验调用者是 owner
- 按邮箱查询 `auth.users`
- 防重入
- 返回符合客户端字段的 JSON

### 10.4 可选 Edge Functions
- 支付回调
- 定时清理
- 模型列表同步

## 11. 环境规划

### 11.1 环境划分
- `dev`
- `staging`
- `prod`

### 11.2 基础环境变量
- `SUPABASE_URL`
- `SUPABASE_ANON_KEY`
- `SUPABASE_SERVICE_ROLE_KEY`
- `SENTRY_DSN`（可选）
- `APP_ENV`

### 11.3 发布要求
- 每个环境独立数据库
- 迁移脚本必须可重复执行
- 生产环境禁止手工改表

## 12. 开发分期

### 阶段 0：确认边界与选型
- 明确采用方案 A / B / C / D
- 确定部署区域
- 确定是否需要商业化扩展
- 交付物：
  - 技术选型确认单
  - 环境清单

### 阶段 1：基础设施搭建
- 创建 Supabase 项目
- 配置环境变量
- 配置基础日志与告警
- 交付物：
  - Supabase 项目
  - 环境变量清单
  - 部署文档

### 阶段 2：数据库建模
- 创建 `user_configs`
- 创建 `sync_data`
- 创建 `teams`
- 创建 `team_members`
- 创建索引、约束、触发器
- 交付物：
  - SQL Migration
  - 数据字典

### 阶段 3：RLS 与 RPC
- 配置 RLS
- 实现 `add_team_member_by_email`
- 验证 owner / member / personal 三类场景
- 交付物：
  - RLS 脚本
  - RPC 脚本
  - 冒烟验证记录

### 阶段 4：客户端联调
- 登录联调
- 用户配置联调
- 同步联调
- 团队联调
- 冲突与软删除联调
- 交付物：
  - 联调问题清单
  - 修复记录

### 阶段 5：上线准备
- 压测
- 备份校验
- 迁移彩排
- 上线窗口计划
- 交付物：
  - 上线清单
  - 回滚预案

### 阶段 6：增强版扩展（可选）
- `model_list`
- `user_subscriptions`
- 支付回调
- 后台管理服务

## 13. 工作分解结构（WBS）

### 13.1 后端/平台
- 环境搭建
- SQL Migration
- RLS
- RPC
- 监控配置

### 13.2 客户端联调
- 登录联调
- 同步联调
- 团队联调
- 异常回放

### 13.3 QA
- API 冒烟
- 数据一致性验证
- 删除与冲突验证

### 13.4 运维
- 备份
- 发布
- 告警

## 14. 预计排期

### 方案 A 推荐排期
- 第 1 周：
  - 环境搭建
  - 数据库建模
  - 触发器和索引
- 第 2 周：
  - RLS
  - RPC
  - 冒烟验证
- 第 3 周：
  - 客户端联调
  - 缺陷修复
  - 上线准备

### 方案 B 推荐排期
- 第 1-2 周：
  - Supabase 基础能力
- 第 3 周：
  - BFF 雏形
- 第 4 周：
  - 支付回调 / 后台任务基础
- 第 5 周：
  - 联调与上线准备

## 15. 交付物清单

### 必选交付物
- 架构说明文档
- 环境清单
- SQL Migration
- RLS 脚本
- RPC 脚本
- 联调记录
- 上线与回滚文档

### 可选交付物
- BFF 服务代码
- 后台管理界面
- 支付回调模块
- 模型管理模块

## 16. 联调与验收

### 16.1 基础验收
- 能登录
- 能保存用户配置
- 能同步工作区
- 能同步连接
- 能创建团队
- 能添加团队成员

### 16.2 一致性验收
- 版本号递增正确
- 软删除能跨端回放
- 冲突不会覆盖错误版本
- 非成员不能看到团队数据

### 16.3 回归验收
- `workspace` 同步
- `connection` 同步
- 团队 owner 角色缓存
- `add_team_member_by_email`

## 17. 风险与应对

### 风险 1：RLS 配置不完整
- 影响：客户端读到越权数据或读不到应有数据
- 应对：用 owner/member/non-member 三组账号做完整回归

### 风险 2：`version` 未自增
- 影响：更新冲突逻辑失效
- 应对：强制用 SQL 测试验证两次连续更新

### 风险 3：团队 owner 未同步进入 `team_members`
- 影响：客户端角色缓存不完整
- 应对：用触发器自动补齐

### 风险 4：文档与代码边界不一致
- 影响：服务端做了客户端根本不需要的对象
- 应对：以 `crates/core/src/cloud_sync/client.rs` 为最终契约源

## 18. 最终建议

### 如果你现在就要开工
- 选 **方案 A**
- 第一批只做：
  - `user_configs`
  - `sync_data`
  - `teams`
  - `team_members`
  - `rpc/add_team_member_by_email`

### 如果你已经确定会做 SaaS 商业化
- 选 **方案 B**
- 在方案 A 的基础上再加：
  - BFF
  - 支付回调
  - `user_subscriptions`
  - 后台任务

### 如果你必须私有化
- 选 **方案 C**
- 但不建议作为第一阶段默认方案
