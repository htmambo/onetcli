## 项目上下文摘要（cloud-sync-account-key-plan）
生成时间：2026-03-24 16:24:59 +0800

### 1. 相似实现分析
- **实现1**: `crates/core/src/cloud_sync/mod.rs:1`
  - 模式：现有云同步流程已经定义为“用户登录账号 -> 首次设置主密钥 -> 上传 `key_verification` -> 后续设备输入同一主密钥解锁同步”
  - 可复用：这套流程天然适合“同一账号保持一致”的目标
  - 需注意：当前同步模型本来就是“账号 + 主密钥”，不一定需要再增加设备授权层

- **实现2**: `main/src/auth.rs:80`
  - 模式：账号登录和会话恢复已由 Supabase Auth 完成
  - 可复用：账号身份确认可以完全继续沿用现有实现
  - 需注意：当前会话恢复只解决“你是谁”，不负责“你是否拥有正确同步密钥”

- **实现3**: `crates/core/src/cloud_sync/service.rs:162`
  - 模式：同步服务用 `key_verification` 验证主密钥，再解锁同步
  - 可复用：如果用户接受“账号密码或授权密钥”，优先复用现有主密钥机制最省事
  - 需注意：如果直接把登录密码作为同步密钥，需要处理密码修改后的全量重加密问题

- **实现4**: `crates/core/src/cloud_sync/supabase.rs:1399`
  - 模式：`user_configs` 已负责保存每个用户唯一的密钥验证信息，`sync_data` 已负责统一同步数据
  - 可复用：最小服务端完全可以只保留这两张核心表
  - 需注意：当前没有 `app_settings` 这种额外同步类型，需要时应扩展 `data_type`

- **实现5**: `crates/core/src/cloud_sync/models.rs:7`
  - 模式：`CloudUserConfig` + `CloudSyncData` 已经形成“账号级配置 + 账号级数据”的数据模型
  - 可复用：同一账号跨平台保持一致，本质上就是同一 `user_id` 共享同一套云端配置和数据
  - 需注意：不需要额外引入设备表，也不需要额外引入授权状态表

### 2. 项目约定
- **命名约定**: 账号级同步对象保持 `user_configs`、`sync_data` 这类明确语义命名
- **文件组织**: 账号认证在 `main/src/auth.rs`，同步核心在 `crates/core/src/cloud_sync/*`
- **代码风格**: 优先复用已有主密钥和统一 blob 同步模型，不引入额外服务层
- **同步策略**: 以同一 `user_id` 作为唯一同步命名空间

### 3. 可复用组件清单
- `crates/core/src/cloud_sync/mod.rs`：当前同步使用流程说明
- `main/src/auth.rs`：账号登录和会话恢复
- `crates/core/src/cloud_sync/service.rs`：主密钥验证与修改流程
- `crates/core/src/cloud_sync/models.rs`：`CloudUserConfig` 与 `CloudSyncData`
- `crates/core/src/cloud_sync/supabase.rs`：`user_configs` / `sync_data` 的 Supabase 访问逻辑
- `crates/core/src/cloud_sync/engine.rs`：同步主链路

### 4. 测试策略
- **文档任务校验**
  - 检查新方案是否明确只保留“账号 + 同步密钥”
  - 检查是否说明“账号密码”和“独立授权密钥”两种路径的差异
  - 检查是否把服务端最小范围收敛为 `user_configs` + `sync_data`
- **后续实现建议验证**
  - 同账号双端首次登录与首次设置同步密钥
  - 新设备输入同一同步密钥后解锁成功
  - `app_settings`、`workspace`、`connection` 跨端一致

### 5. 依赖和集成点
- **必须依赖**
  - Supabase Auth
  - `user_configs`
  - `sync_data`
- **可选增强**
  - 若后续想减少输入次数，可在客户端把登录密码派生为同步密钥
- **不再作为主路径**
  - `device_authorizations`
  - 设备注册 RPC

### 6. 技术选型理由
- **为什么进一步简化**: 用户最终要求是“保证每个账户一致”，而不是管理设备
- **为什么推荐“账号 + 同步密钥”**: 当前代码已经有主密钥机制，改造量最小
- **为什么不把设备授权作为主设计**: 对当前目标来说属于额外复杂度

### 7. 关键风险点
- **密码耦合风险**: 若直接使用登录密码作为同步密钥，改密码会触发全量重加密或跨端重新解锁
- **类型扩展风险**: 若要同步应用设置，需要补 `app_settings` 的本地序列化和同步处理
- **旧文档干扰风险**: 之前的完整版方案和设备授权方案都比当前目标更复杂，当前应以“账号 + 同步密钥”版本为准
