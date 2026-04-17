## 项目上下文摘要（sync-server-rust-integration）
生成时间：2026-03-24 17:31:54 +0800

### 1. 相似实现分析
- **实现1**: `crates/core/src/cloud_sync/supabase.rs`
  - 模式：云端客户端自行维护认证状态、自动刷新令牌、401 后重试
  - 可复用：`build_request`、`send_request`、本地 `auth_state`、回调通知模式
  - 需注意：现有实现强绑定 Supabase 路径与响应结构，不能直接复用协议层

- **实现2**: `main/src/auth.rs`
  - 模式：认证服务负责读取环境配置、恢复本地 `auth.json`、保存刷新后的 token
  - 可复用：会话恢复、`save_auth_data` / `load_auth_data`、登录成功后的用户加载流程
  - 需注意：当前只支持 OTP UI，且内部 concrete type 是 `SupabaseClient`

- **实现3**: `main/src/home_tab.rs`
  - 模式：首页通过 `AuthService` 获取 `CloudApiClient`，再交给 `SyncEngine`
  - 可复用：登录成功后更新 `GlobalCurrentUser` 并触发自动同步
  - 需注意：`show_login_dialog` 目前写死 OTP 登录弹窗

- **实现4**: `sync_server/server/src/http/routes/auth.ts`
  - 模式：自定义 REST 认证接口，注册/登录/刷新均返回统一会话数据
  - 可复用：`/api/v1/auth/register`、`/login`、`/refresh`、`/me`、`/logout`
  - 需注意：当前 TypeScript 改动存在编译错误，需先修复

- **实现5**: `sync_server/server/src/http/routes/sync.ts`
  - 模式：用户配置与同步项 API 已经与客户端需要的 `user_config + sync_data` 语义对齐
  - 可复用：`/api/v1/sync/config`、`/api/v1/sync/items`
  - 需注意：服务端当前不支持团队数据，Rust 客户端应降级返回空团队列表

### 2. 项目约定
- **命名约定**: Rust 配置结构沿用 `XxxConfig`，云端客户端沿用 `XxxClient`
- **文件组织**: 云同步客户端放在 `crates/core/src/cloud_sync/`
- **认证持久化**: 继续使用本地 `auth.json`，不新建额外会话存储
- **UI 模式**: 保留现有 OTP 弹窗，同时为密码模式增加单独对话框

### 3. 可复用组件清单
- `crates/core/src/cloud_sync/client.rs`
- `crates/core/src/cloud_sync/models.rs`
- `crates/core/src/cloud_sync/supabase.rs`
- `crates/core/src/config.rs`
- `main/src/auth.rs`
- `main/src/home_tab.rs`
- `sync_server/server/src/http/routes/auth.ts`
- `sync_server/server/src/http/routes/sync.ts`

### 4. 测试策略
- `npm run check --workspace server`
- `npm run build --workspace server`
- `npm run build`
- `cargo check -p one-core`
- `cargo check -p main`

### 5. 依赖和集成点
- **Rust 配置入口**: `crates/core/src/config.rs`
- **Rust 云端抽象**: `crates/core/src/cloud_sync/client.rs`
- **Rust 认证与 UI**: `main/src/auth.rs`、`main/src/home_tab.rs`
- **服务端接口**: `sync_server/server/src/http/routes/auth.ts`、`sync_server/server/src/http/routes/sync.ts`
- **运行时配置**: 新增 `SYNC_SERVER_URL`，保留 `SUPABASE_URL` / `SUPABASE_ANON_KEY` 兼容旧模式

### 6. 技术选型理由
- **为何新增 `SyncServerClient`**: 当前 Rust 客户端路径和响应都写死 Supabase，不能靠改环境变量切换
- **为何保留 `CloudApiClient` 抽象**: 同步引擎、LLM Provider 已依赖此抽象，最小改造面就是新增第二个实现
- **为何 UI 分模式而不是强行统一**: Supabase 仍是 OTP，`sync_server` 是密码登录，直接分支最清晰

### 7. 关键风险点
- **协议差异风险**: `sync_server` 返回 ISO 时间，Rust 侧需要统一换算成时间戳
- **会话恢复风险**: 若不处理 refresh 回调，本地 `auth.json` 会保留旧 token 导致下次恢复失败
- **团队能力降级**: `sync_server` 不支持团队，同步引擎必须接受空团队列表
- **工作树风险**: 仓库存在用户已有未提交改动，改造时不能回退无关变更
