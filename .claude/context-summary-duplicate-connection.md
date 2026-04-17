## 项目上下文摘要（duplicate-connection）
生成时间：2026-03-26 13:47:00 +0800

### 1. 相似实现分析
- **实现1**: `main/src/home_tab.rs:1648`
  - 模式：主页统一负责打开各类连接表单窗口。
  - 可复用：`show_connection_form(...)`、`show_ssh_form(...)`、`show_redis_form(...)`、`show_mongodb_form(...)`、`show_serial_form(...)`
  - 需注意：这些入口默认依赖 `editing_connection_id` 从当前列表取连接。

- **实现2**: `crates/db_view/src/connection_form_window.rs:37`
  - 模式：数据库连接窗口通过 `ConnectionFormWindowConfig.editing_connection` 决定是否进入编辑态。
  - 可复用：直接传 `editing_connection: Some(connection)` 即可打开编辑窗。
  - 需注意：窗口标题和保存事件会随编辑态切换。

- **实现3**: `main/src/home_tab.rs:210`
  - 模式：连接创建/更新事件会刷新列表，并在登录且主密钥已解锁时自动触发同步。
  - 可复用：复制后的最终保存仍可沿用 `ConnectionUpdated` 既有链路。
  - 需注意：如果复制动作本身发出 `ConnectionCreated`，会导致未编辑完成前就自动同步。

### 2. 项目约定
- **命名约定**: 主页辅助方法使用 `verb_object` 风格，如 `confirm_delete_connection`、`show_ssh_form`
- **文件组织**: 连接卡片交互在 `main/src/home_tab.rs`，各类表单窗口分散在对应 crate
- **代码风格**: 用户可见文案走 `main/locales/main.yml`；错误弹窗使用 `window.open_dialog(...)`

### 3. 可复用组件清单
- `ConnectionRepository::insert(...)`
- `open_popup_window(...)`
- `ConnectionFormWindowConfig` / `SshFormWindowConfig` / `RedisFormWindowConfig` / `MongoFormWindowConfig` / `SerialFormWindowConfig`

### 4. 测试策略
- `cargo fmt --all`
- `cargo check -p main`

### 5. 风险点
- 复制后用户若直接关闭编辑窗，本地副本会保留，这是当前需求明确接受的简化行为。
- 为避免复制动作立即触发云同步，本次不发 `ConnectionCreated` 事件，而是等编辑保存后走 `ConnectionUpdated`。
