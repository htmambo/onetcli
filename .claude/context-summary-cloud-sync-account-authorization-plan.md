## 项目上下文摘要（cloud-sync-account-authorization-plan）
生成时间：2026-03-24 16:21:31 +0800

### 1. 相似实现分析
- **实现1**: `main/src/auth.rs:80`
  - 模式：当前账号体系已经基于 Supabase Auth，支持登录、会话恢复、刷新令牌和本地持久化
  - 可复用：账号登录、会话恢复、登录后自动进入同步前置流程
  - 需注意：当前只有“用户会话”，没有“设备授权”概念

- **实现2**: `crates/core/src/cloud_sync/client.rs:59`
  - 模式：云端能力统一收敛在 `CloudApiClient` trait 中，认证、用户配置、同步、团队和聊天接口已分组定义
  - 可复用：新增设备授权时应沿用同一客户端抽象，而不是旁路新增一套独立云端 SDK
  - 需注意：当前 trait 没有设备注册、设备授权预检接口

- **实现3**: `crates/core/src/cloud_sync/supabase.rs:1399`
  - 模式：用户配置通过 `user_configs` 表 upsert，统一同步通过 `sync_data` 表 CRUD，团队扩展通过 RPC
  - 可复用：设备授权最适合复用现有 Supabase 模式，新增一张表 + 一个 RPC，而不是引入新后端
  - 需注意：当前 `sync_data` 的读写只基于用户身份，没有设备维度约束

- **实现4**: `crates/core/src/cloud_sync/engine.rs:147`
  - 模式：同步入口固定为 `sync()`，先拿团队，再同步工作区和连接；`list_teams()` 失败时会降级为只同步个人数据
  - 可复用：当前“个人数据优先”的主链路可以直接承接简化版方案
  - 需注意：如果第一阶段不做团队，当前同步主链仍可工作，但团队 UI 需要隐藏或返回空数据

- **实现5**: `crates/core/src/cloud_sync/models.rs:191`
  - 模式：`sync_data` 已经是统一 blob 同步模型，支持 `checksum`、`version`、`deleted_at`
  - 可复用：跨平台自动同步仍应继续复用统一表，不要拆分多张业务表
  - 需注意：如果要同步“应用设置”，需要新增一个 `data_type`，而不是另造新表

- **实现6**: `crates/core/src/cloud_sync/service.rs:500`
  - 模式：现有测试聚焦 blob 加解密、校验和、团队密钥
  - 可复用：后续若实现设备授权，应补设备注册与同步预检测试，而不是重新设计同步加密层
  - 需注意：当前没有设备授权、设备撤销、设备范围同步的测试样例

### 2. 项目约定
- **命名约定**: 云同步相关对象采用清晰表语义命名，如 `user_configs`、`sync_data`、`teams`、`team_members`
- **文件组织**: 认证入口在 `main/src/auth.rs`，协议抽象在 `crates/core/src/cloud_sync/client.rs`，Supabase 实现在 `crates/core/src/cloud_sync/supabase.rs`
- **代码风格**: 优先复用 Supabase Auth、PostgREST、RPC；不额外发明自研鉴权服务
- **同步策略**: 现有主链路以个人同步为核心，团队是附加层，统一数据模型是 `sync_data`

### 3. 可复用组件清单
- `main/src/auth.rs`：账号登录、会话恢复、令牌刷新
- `crates/core/src/cloud_sync/client.rs`：云端接口契约
- `crates/core/src/cloud_sync/supabase.rs`：Supabase 表访问和 RPC 调用方式
- `crates/core/src/cloud_sync/engine.rs`：同步编排入口
- `crates/core/src/cloud_sync/models.rs`：统一同步数据结构
- `crates/core/src/cloud_sync/service.rs`：加密 blob 与校验和逻辑
- `.claude/cloud-sync-server-complete-development-plan.md`：上一版完整版方案，作为“应删繁就简”的对照

### 4. 测试策略
- **文档任务校验**
  - 检查新方案是否明确收敛到“账号 + 设备授权 + 自动同步”
  - 检查是否显式排除 Pro、订阅、支付、团队后台等复杂能力
  - 检查是否说明了当前代码可直接复用的边界与仍需补充的客户端改动
- **后续实现建议验证**
  - 设备注册 RPC 冒烟
  - 同账号双端增量同步冒烟
  - 已撤销设备同步预检阻断

### 5. 依赖和集成点
- **现有依赖**
  - Supabase Auth：账号登录
  - `user_configs`：主密钥验证信息
  - `sync_data`：统一同步数据
- **新增集成点**
  - 设备授权表，例如 `device_authorizations`
  - 设备注册/预检 RPC，例如 `rpc/register_device`、`rpc/check_sync_access`
- **客户端补充点**
  - 持久化 `device_id`
  - 登录或恢复会话后注册设备
  - 每次自动同步前做一次设备授权预检

### 6. 技术选型理由
- **为什么改成“账号 + 设备授权”**: 用户的核心诉求是同一账号在多个平台之间自动同步，而不是订阅、支付或团队协作
- **为什么继续使用 Supabase**: 当前登录和同步协议已经与 Supabase 紧耦合，继续复用成本最低
- **为什么推荐“表 + RPC”而不是 BFF**: 对当前目标来说，设备授权只需要一个轻量授权层，不值得引入完整后端服务

### 7. 关键风险点
- **严格阻断风险**: 当前 `sync_data` 直接走 PostgREST，第一阶段若不改客户端请求头或不改为 RPC/BFF，设备撤销更适合做“同步前预检阻断”而不是“每个请求强制阻断”
- **范围同步风险**: 若要按平台差异只同步部分内容，客户端必须声明 `device_id` 和同步范围
- **兼容风险**: 团队接口和模型列表接口仍在代码里，若第一阶段完全不做，需要通过 UI 隐藏或返回空数据降低影响
