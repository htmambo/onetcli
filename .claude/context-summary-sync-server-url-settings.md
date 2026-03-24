## 项目上下文摘要（sync-server-url-settings）
生成时间：2026-03-24 22:24:31 +0800

### 1. 相似实现分析
- **实现1**: `main/src/setting_tab.rs`
  - 模式：`AppSettings` 通过 `settings.json` 持久化，设置页使用 `SettingPage -> SettingGroup -> SettingItem -> SettingField` 描述。
  - 可复用：`AppSettings::load/save`、`SettingField::input`、字符串设置项的 `settings.save()` 回调模式。
  - 需注意：设置项回调只拿到 `&mut App`，不能直接依赖窗口上下文。

- **实现2**: `crates/ui/src/setting/fields/string.rs`
  - 模式：字符串设置项基于 `InputState`，每次 `InputEvent::Change` 都会立即写回全局状态。
  - 可复用：`SettingField<SharedString>::input`
  - 需注意：组件本身没有 placeholder，格式说明要通过描述文案补足。

- **实现3**: `main/src/auth.rs`
  - 模式：`AuthService` 全局单例持有 `SyncServerClient`，负责登录、登出、会话恢复和云端客户端暴露。
  - 可复用：`save_auth_data`、`clear_auth_data`、会话过期/令牌刷新回调
  - 需注意：如果同步地址要在设置页热更新，不能继续把地址在初始化时写死。

- **实现4**: `main/src/home_tab.rs`
  - 模式：首页通过 `show_login_dialog` 触发登录弹窗，`trigger_sync` 负责同步前置校验和反馈通知。
  - 可复用：错误弹窗、同步反馈通知、`add_settings_tab`
  - 需注意：未配置同步地址时，需要在登录入口和同步入口都给出准确提示，且不能走现有“认证失败后自动重开登录框”的循环。

### 2. 项目约定
- **命名约定**: 应用设置字段使用 `snake_case`，翻译键采用 `Settings.General.*` / `Auth.*` / `Home.*`
- **文件组织**: 设置持久化和设置页定义集中在 `main/src/setting_tab.rs`
- **导入顺序**: 标准库、第三方、`one_core`、`crate` 顺序
- **交互风格**: 设置项修改立即生效并持久化，不额外要求“保存”按钮

### 3. 可复用组件清单
- `main/src/setting_tab.rs::AppSettings`
- `crates/ui/src/setting/fields/string.rs::StringField`
- `main/src/home_tab.rs::add_settings_tab`
- `main/src/home_tab.rs::push_sync_notification`
- `crates/core/src/cloud_sync/sync_server.rs::SyncServerClient`

### 4. 测试策略
- **测试框架**: Rust 内置测试
- **验证方式**:
  - `cargo test -p main`
  - `cargo test -p one-core --no-run`
  - 关键词检索确认不再读取 `SYNC_SERVER_URL`
- **风险点**:
  - 动态更新同步地址后旧登录态是否残留
  - 登录提示是否会与现有 `auth_error` 重开逻辑冲突

### 5. 依赖和集成点
- **设置来源**: `main/src/setting_tab.rs::AppSettings`
- **认证入口**: `main/src/auth.rs::AuthService`
- **同步入口**: `main/src/home_tab.rs::trigger_sync`
- **AI 提供器集成**: `crates/core/src/llm/manager.rs::ProviderManager::set_cloud_client`

### 6. 技术选型理由
- **推荐方案**: 将同步地址存入 `AppSettings`，并让 `SyncServerClient` 支持运行时更新地址
- **原因**: 这样可以复用现有全局认证服务与 LLM provider 对同一个客户端对象的引用，避免地址变更后还要广播替换多个 `Arc`

### 7. 关键风险点
- **地址变更后登录态污染**: 必须清空本地认证数据和首页用户状态
- **提示循环**: 未配置地址时不能复用现有“认证失败后自动弹回登录”的逻辑
- **环境变量残留**: 需要删除 `SYNC_SERVER_URL` 的运行时/编译时读取，避免设置来源不一致
