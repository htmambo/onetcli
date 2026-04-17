## 项目上下文摘要（sync-server-implementation）
生成时间：2026-03-24 16:24:59 +0800

### 1. 相似实现分析
- **实现1**: `crates/core/src/storage/migration.rs:1`
  - 模式：通过 `include_str!` 内嵌 SQL migration，使用 `_migrations` 表记录版本
  - 可复用：`sync_server` 可以沿用同样的 SQLite migration 组织方式
  - 需注意：迁移应用必须幂等，避免重复启动时报错

- **实现2**: `crates/core/src/storage/connection.rs:1`
  - 模式：SQLite 连接初始化时统一设置 `WAL`、`foreign_keys`、`busy_timeout`
  - 可复用：独立服务的 SQLite 连接也应启用同样的 pragma
  - 需注意：服务端请求并发下要避免把单个 `rusqlite::Connection` 直接跨线程共享

- **实现3**: `crates/core/src/storage/manager.rs:1`
  - 模式：启动时创建数据库目录、打开 SQLite、执行 migration
  - 可复用：`sync_server` 启动时也应该自动初始化数据库文件
  - 需注意：独立项目需要自己的默认数据库路径与配置读取逻辑

- **实现4**: `crates/core/src/cloud_sync/mod.rs:1`
  - 模式：当前产品同步语义已经明确为“账号登录 + 主密钥验证 + 云端同步”
  - 可复用：服务端 API 只需围绕账号、同步密钥配置、同步数据展开
  - 需注意：当前目标已经从 Supabase 兼容收敛为“独立部署 + SQLite 默认存储”

- **实现5**: `crates/core/src/cloud_sync/supabase.rs:838`
  - 模式：客户端现有同步核心模型就是 `user_configs` 和 `sync_data`
  - 可复用：独立服务的数据表仍应沿用这两个核心对象，降低后续接入成本
  - 需注意：当前最小服务不再做团队、订阅和设备授权

### 2. 项目约定
- **命名约定**: 目录名和项目名使用 `sync_server`
- **文件组织**: 独立项目不加入当前 workspace，避免影响主工程
- **代码风格**: 保持 Rust 项目常规模块拆分，数据库结构尽量贴近现有同步模型
- **默认存储**: 数据库默认使用 SQLite，本地单文件部署优先

### 3. 可复用组件清单
- `crates/core/src/storage/migration.rs`
- `crates/core/src/storage/connection.rs`
- `crates/core/src/storage/manager.rs`
- `crates/core/src/cloud_sync/mod.rs`
- `crates/core/src/cloud_sync/models.rs`
- `crates/core/src/cloud_sync/service.rs`

### 4. 测试策略
- `cargo check --manifest-path sync_server/Cargo.toml`
- `cargo test --manifest-path sync_server/Cargo.toml`
- 静态检查：
  - 核心文件存在性
  - migration 是否包含 `users`、`auth_sessions`、`user_configs`、`sync_data`
  - README 是否包含独立部署说明

### 5. 依赖和集成点
- **数据库**: SQLite + `rusqlite`
- **HTTP 服务**: 独立服务框架需新引入
- **认证模型**: 账号注册/登录 + Bearer Token Session
- **同步模型**: `user_configs` + `sync_data`

### 6. 技术选型理由
- **为什么做成独立项目**: 用户已明确要求 `sync_server` 独立部署
- **为什么默认 SQLite**: 用户已明确指定默认数据库为 SQLite
- **为什么先做基础 API 纵向切片**: 先把注册、登录、同步配置、同步数据打通，才能继续做客户端对接

### 7. 关键风险点
- **协议偏差风险**: 当前客户端仍是 Supabase 风格，后续接独立服务需要额外客户端改造
- **SQLite 并发风险**: 服务端必须用正确的连接打开方式和 pragma
- **验证限制**: 当前仓库无本地 PostgreSQL，但 SQLite 验证可完整本地执行
