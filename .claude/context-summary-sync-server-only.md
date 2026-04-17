## 项目上下文摘要（sync-server-only）
生成时间：2026-03-24 21:53:39 +0800

### 1. 相似实现分析
- **实现1**: `main/src/auth.rs`
  - 模式：认证服务通过 `AuthBackend` 包装具体云端客户端，并向 UI 暴露统一的登录、登出、会话恢复接口。
  - 可复用：`finish_auth`、`save_auth_data`、`load_auth_data`、`show_password_auth_dialog`
  - 需注意：当前仍保留 `Supabase` 和 `sync_server` 双后端分支，OTP 流完全依赖旧后端。

- **实现2**: `main/src/home_tab.rs`
  - 模式：首页通过 `show_login_dialog` 触发认证弹窗，再在异步回调里更新 `current_user`、`auth_error` 和同步状态。
  - 可复用：`authenticate_with_password`、`show_login_dialog`、`trigger_sync`、现有 `sync_feedback` 展示链路
  - 需注意：`show_login_dialog` 仍按 `AuthMode` 分支，删除 OTP 后要直接收敛到密码登录。

- **实现3**: `crates/core/src/config.rs`
  - 模式：配置统一通过 `get()` 从运行时环境变量和编译期 `option_env!` 读取。
  - 可复用：`SyncServerConfig`、`normalize_url`
  - 需注意：`SupabaseConfig` 与 `SYNC_SERVER_URL` 并存，`build.rs` 也仍注入 `SUPABASE_*`。

- **实现4**: `crates/core/src/cloud_sync/client.rs` + `crates/core/src/cloud_sync/sync_server.rs`
  - 模式：`CloudApiClient` 定义统一接口，具体后端在实现中各自映射云端协议。
  - 可复用：`SyncServerClient` 全量认证和同步实现、`unsupported` 错误映射
  - 需注意：trait 里还有 `sign_in_with_otp` / `verify_otp`，`sync_server` 只是返回“不支持”。

### 2. 项目约定
- **命名约定**: Rust 类型使用 `PascalCase`，函数与字段使用 `snake_case`，UI 文案使用 `t!("命名空间.key")`
- **文件组织**: UI 认证逻辑在 `main/src/auth.rs` 与 `main/src/home_tab.rs`；底层同步和配置在 `crates/core/src`
- **导入顺序**: 先标准库，再第三方 crate，最后 `crate::...`
- **代码风格**: 维持现有链式 UI 构造和 `match` 分支写法，避免引入新的抽象层

### 3. 可复用组件清单
- `main/src/auth.rs::show_password_auth_dialog`：现有唯一保留的登录 UI
- `main/src/auth.rs::finish_auth`：认证成功后的统一落盘和用户信息收敛逻辑
- `crates/core/src/config.rs::normalize_url`：同步地址标准化逻辑
- `main/src/home_tab.rs::authenticate_with_password`：首页登录后的用户状态更新和自动同步

### 4. 测试策略
- **测试框架**: Rust 内置测试
- **测试模式**: 以纯逻辑单测和包级编译验证为主
- **参考文件**:
  - `main/src/home_tab.rs` 末尾 `summarize_sync_result_*` 测试
  - `crates/core/src/config.rs` 末尾配置测试
- **覆盖要求**:
  - `main` 包完整测试
  - `one-core` 至少完成 `--no-run` 编译验证
  - 清理后确认不存在 `Supabase` / OTP 接口残留引用

### 5. 依赖和集成点
- **外部依赖**: `gpui`、`gpui_component`、`rust_i18n`
- **内部依赖**:
  - `HomePage` 依赖 `AuthService`
  - `AuthService` 依赖 `one_core::config::SyncServerConfig` 与 `one_core::cloud_sync::SyncServerClient`
  - `OnetCliLLMProvider` 依赖 `CloudApiClient`
- **集成方式**: UI 通过 `AuthService` 统一调用云端客户端；同步与 AI 代理通过 `CloudApiClient` 抽象层访问
- **配置来源**: 当前仅发现 `SYNC_SERVER_URL`、`ONETCLI_UPDATE_URL`、`ONETCLI_UPDATE_DOWNLOAD_URL`

### 6. 技术选型理由
- **为什么用这个方案**: 用户已明确只保留 `sync_server`，因此应删除 Supabase 分支而不是继续维护双后端兼容
- **优势**: 减少认证模式分支、删除无效 OTP 接口、降低配置和文案复杂度
- **劣势和风险**: 这是破坏性清理，若仓库中仍有隐含 Supabase 依赖，会在编译阶段集中暴露

### 7. 关键风险点
- **接口收缩风险**: 删除 `CloudApiClient` 的 OTP 方法后，所有实现和调用点都必须同步收敛
- **文案残留风险**: `main/locales/main.yml` 与仓库说明若不清理，会继续暴露已下线能力
- **工作树风险**: 当前仓库存在其他未提交改动，必须避免误触 `sync_server/` 下无关文件
- **验证重点**: 清理后需重新跑 `cargo test -p main`，并对 `one-core` 做编译验证
