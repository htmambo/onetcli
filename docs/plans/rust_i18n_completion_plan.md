# rust_i18n 补全实施计划

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** 将项目中所有未国际化的用户可见字符串（中文硬编码、UI 英文标签、错误提示）全部迁移到 rust_i18n 体系。

**Architecture:** 按 crate 逐个攻克：先补齐缺失的 `locales/*.yml` 和 Cargo 依赖，再将 `.rs` 中的硬编码字符串替换为 `t!("key")`，同步维护翻译 YAML。采用"产品代码优先、测试代码不强制"策略。

**Tech Stack:** rust, rust_i18n, gpui, YAML

---

## 阶段一：基础设施补齐

### Task 1: 为 `one_ui` 创建 locales 目录和翻译文件

**Objective:** `one_ui` 已依赖 rust_i18n 但缺 `locales/`，必须补齐。

**Files:**
- Create: `crates/one_ui/locales/one_ui.yml`
- Modify: `crates/one_ui/src/lib.rs` (若尚无 `i18n!("one_ui");则添加)

**Step 1:** 在 `crates/one_ui/src/lib.rs` 中搜索是否已有 `rust_i18n::i18n!` 宏调用。
**Step 2:** 若无，在合适位置添加 `rust_i18n::i18n!("one_ui", fallback = "en");`。
**Step 3:** 创建 `crates/one_ui/locales/one_ui.yml`，内容为空的 `en:` 根节点。
**Step 4:** 验证 `cargo check -p one_ui` 通过。
**Step 5:** 不提交。

### Task 2: 为 `terminal` 接入 rust_i18n

**Objective:** `terminal` crate 目前无 rust_i18n 依赖，需要接入并创建翻译文件。

**Files:**
- Modify: `crates/terminal/Cargo.toml`
- Create: `crates/terminal/locales/terminal.yml`
- Modify: `crates/terminal/src/lib.rs`

**Step 1:** 在 `crates/terminal/Cargo.toml` 的 `[dependencies]` 中添加 `rust-i18n = { workspace = true }`。
**Step 2:** 在 `crates/terminal/src/lib.rs` 顶部添加 `rust_i18n::i18n!("terminal", fallback = "en");`。
**Step 3:** 创建 `crates/terminal/locales/terminal.yml`，内容为 `en:` 根节点。
**Step 4:** 检查所有 `crates/terminal/src/**/*.rs`，如果有文件需要调用 `t!`，确保它们 `use rust_i18n::t;`。
**Step 5:** 验证 `cargo check -p terminal` 通过。

---

## 阶段二：数据库对象树节点标签国际化（`db` crate）

### Task 3: 抽取通用数据库树节点标签到 `db` locales

**Objective:** 将 `Schemas`, `Tables`, `Views`, `Functions`, `Procedures`, `Triggers`, `Sequences`, `Columns`, `Indexes`, `Databases` 等英文标签全部收录到 `db.yml`。

**Files:**
- Modify: `crates/db/locales/db.yml`
- Modify: `crates/db/src/oracle/plugin.rs`
- Modify: `crates/db/src/mssql/plugin.rs`
- Modify: `crates/db/src/clickhouse/plugin.rs`
- Modify: `crates/db/src/mysql/plugin.rs` (若有)
- Modify: `crates/db/src/duckdb/plugin.rs` (若有)
- Modify: `crates/db/src/postgresql/plugin.rs` (若有)
- Modify: `crates/db/src/sqlite/plugin.rs` (若有)

**Step 1:** 在 `db.yml` 中新增 `Tree:` 节点：
```yaml
Tree:
  schemas: Schemas
  tables: Tables
  views: Views
  functions: Functions
  procedures: Procedures
  triggers: Triggers
  sequences: Sequences
  columns: "Columns - {name}"
  indexes: "Indexes - {name}"
  databases: Databases
  stored_procedures: "Stored Procedures"
```
**Step 2:** 在以下文件中搜索并替换硬编码的英文标签为 `t!("db.Tree.xxx")` 调用：
- `crates/db/src/oracle/plugin.rs`
- `crates/db/src/mssql/plugin.rs`
- `crates/db/src/clickhouse/plugin.rs`
- 其他数据库 plugin.rs 文件
**Step 3:** 确保这些文件已经 `use rust_i18n::t;`（大多数已有）。
**Step 4:** 验证 `cargo check -p db` 通过。

### Task 4: 数据库连接错误提示国际化（`db` crate 中的 `Parse error`）

**Objective:** 数据库各连接文件中的 `"Parse error: {}"` 由于是用户可见错误，需要国际化。

**Files:**
- Modify: `crates/db/locales/db.yml`
- Modify: `crates/db/src/oracle/connection.rs`
- Modify: `crates/db/src/mysql/connection.rs`
- Modify: `crates/db/src/mssql/connection.rs`
- Modify: `crates/db/src/postgresql/connection.rs`
- Modify: `crates/db/src/clickhouse/connection.rs`
- Modify: `crates/db/src/duckdb/connection.rs`
- Modify: `crates/db/src/sqlite/connection.rs`

**Step 1:** 在 `db.yml` 新增 `Error.parse_error: "Parse error: {error}"`。
**Step 2:** 将以上连接文件中的所有 `"Parse error: {}"` / `format!("Parse error: {}", ...)` 改为 `t!("db.Error.parse_error", error = ...)`。
**Step 3:** 确保这些文件 `use rust_i18n::t;`。
**Step 4:** 验证 `cargo check -p db` 。

---

## 阶段三：`terminal` crate 中文硬编码迁移

### Task 5: 迁移终端核心中文字符串

**Objective:** 将 `terminal.rs` 中的用户可见中文全部迁移到 `terminal.yml`。

**Files:**
- Modify: `crates/terminal/locales/terminal.yml`
- Modify: `crates/terminal/src/terminal.rs`

**Step 1:** 在 `terminal.yml` 新增翻译：
```yaml
ShellIntegration:
  history_restored: "

[30;47m * [0m[97;100m History Restored [0m

"
  history_restored_brief: "*History Restored"
  temp_dir_failed: "Failed to create temp dir {path}, skipping Shell Integration"
  write_script_failed: "Failed to write shell_integration.sh: {error}"
  zsh_configured: "Configured zsh Shell Integration (ZDOTDIR={zd})"
  bash_configured: "Configured bash Shell Integration (--rcfile={rc})"
  unknown_shell: "Unknown shell type '{shell}', skipping Shell Integration injection"

Connection:
  local_pty_host_failed: "Failed to connect to local-pty-host"
  local_pty_spawn_failed: "hosted local PTY spawn failed"
  local_pty_attach_failed: "hosted local PTY attach failed"
  ssh_resize_sync: "SSH connected, syncing terminal size to remote: {cols}x{rows}"
  serial_connected: "Serial port connected"
  command_finished: "Command finished, exit code: {code}"
```
**Step 2:** 在 `terminal.rs` 中：
- 确保顶部有 `use rust_i18n::t;`
- 替换对应字符串为 `t!("terminal.ShellIntegration.xxx", ...)` 和 `t!("terminal.Connection.xxx", ...)`
**Step 3:** 对于带 ANSI 格式的 `history_restored`，将其作为 `t!` 返回值使用。
**Step 4:** 验证 `cargo check -p terminal` 。

### Task 6: 迁移 local-pty-host 和 ssh_backend 中文字符串

**Objective:** 迁移 `local_pty_host*.rs` 和 `ssh_backend.rs` 中的中文错误/日志。

**Files:**
- Modify: `crates/terminal/locales/terminal.yml`
- Modify: `crates/terminal/src/local_pty_host.rs`
- Modify: `crates/terminal/src/local_pty_host_unix.rs`
- Modify: `crates/terminal/src/local_pty_host_windows.rs`
- Modify: `crates/terminal/src/local_pty_client.rs`
- Modify: `crates/terminal/src/ssh_backend.rs`

**Step 1:** 在 `terminal.yml` 新增 `LocalPtyHost`, `LocalPtyClient`, `SshBackend` 节点，将所有中文字符串抽取成翻译键值。
**Step 2:** 在各 `.rs` 中 `use rust_i18n::t;` 并替换字符串。
**Step 3:** 验证 `cargo check -p terminal` 。

---

## 阶段四：`core` crate 用户可见中文迁移

### Task 7: 迁移 Agent 安全策略和 AI Chat 中文字符串

**Objective:** `core/src/agent/security.rs` 和 `core/src/ai_chat/*.rs` 中的用户直接可见中文全部国际化。

**Files:**
- Modify: `crates/core/locales/core.yml`
- Modify: `crates/core/src/agent/security.rs`
- Modify: `crates/core/src/ai_chat/engine.rs`
- Modify: `crates/core/src/ai_chat/panel.rs`
- Modify: `crates/core/src/ai_chat/stream.rs`
- Modify: `crates/core/src/ai_chat/services.rs`
- Modify: `crates/core/src/ai_chat/ask_ai.rs`
- Modify: `crates/core/src/ai_chat/types.rs`

**Step 1:** 在 `core.yml` 新增：
```yaml
Agent:
  Security:
    observe_only: "Observe Only"
    confirm_before_exec: "Confirm Before Execution"
    full_autonomy: "Full Autonomy"
    danger_recursive_rm: "Danger: recursive force delete"
    danger_format: "Danger: disk format"
    danger_write_device: "Danger: direct device write"
    danger_shutdown: "Danger: system shutdown/reboot"
    danger_fork_bomb: "Danger: Fork bomb"
    danger_system_dir: "Danger: modifying system core directory"
    danger_rce: "Danger: remote code execution risk"
    danger_delete_cron: "Danger: delete all cron jobs"
    danger_delete_auth: "Danger: delete authentication credentials"
    danger_auth_change: "Danger: password/auth config modification"
    danger_secure_erase: "Danger: secure erase"
    danger_python_exec: "Danger: Python code execution"
    danger_ruby_exec: "Danger: Ruby code execution"
    danger_perl_exec: "Danger: Perl code execution"
    danger_pattern_matched: "Command matched dangerous pattern: {pattern}"
    privilege_needs_confirm: "Privileged operation requires confirmation in observe mode"
    waiting_user_confirm: "Waiting for user confirmation"
  Chat:
    create_session_failed: "Failed to create session: {error}"
    save_assistant_failed: "Failed to save assistant message: {error}"
    stream_cancelled: "Stream cancelled by user"
    ask_ai_template: "AiChat.ask_ai_template"
    provider_not_found: "AiChat.stream_provider_not_found"
    api_error: "AiChat.stream_api_error"
    storage_error: "AiChat.stream_storage_error"
    session_repo_unavailable: "AiChat.session_repo_unavailable"
    session_not_found: "AiChat.session_not_found"
    session_storage_error: "AiChat.session_storage_error"
```
**Step 2:** 确保各目标文件 `use rust_i18n::t;` 并替换字符串。
**Step 3:** 对于已经是 `t!("AiChat.xxx")` 格式的调用（如 `ask_ai_template`），如果它们现在是直接写了字面量而非调用 t!，则替换为真正的 `t!` 调用。
**Step 4:** 验证 `cargo check -p one-core` 。

### Task 8: 迁移证书管理器和云同步中文字符串

**Objective:** `certificate_manager.rs` 和 `cloud_sync/**/*.rs` 中的用户可见中文。

**Files:**
- Modify: `crates/core/locales/core.yml`
- Modify: `crates/core/src/certificate_manager.rs`
- Modify: `crates/core/src/cloud_sync/blob_vault_driver.rs`
- Modify: `crates/core/src/cloud_sync/certificate_sync.rs`
- Modify: `crates/core/src/cloud_sync/client.rs`
- Modify: `crates/core/src/cloud_sync/connection_sync.rs`
- Modify: `crates/core/src/cloud_sync/engine.rs`
- Modify: `crates/core/src/cloud_sync/generic_sync.rs`
- Modify: `crates/core/src/cloud_sync/conflict.rs`

**Step 1:** 在 `core.yml` 新增 `Certificate`, `CloudSync`, `CloudSyncError` 等节点，收录主要中文字符串的翻译。
**Step 2:** 替换各文件中的中文硬编码为 `t!("core.xxx")`。
**Step 3:** 对于大量日志级别的云同步中文（如 `[同步] 上传失败...`），也全部抽取到 YAML。
**Step 4:** 验证 `cargo check -p one-core` 。

---

## 阶段五：View crates 中文硬编码迁移

### Task 9: 迁移 `terminal_view` 中文字符串

**Objective:** `terminal_view/src/**/*.rs` 中的用户可见中文。

**Files:**
- Modify: `crates/terminal_view/locales/terminal_view.yml`
- Modify: `crates/terminal_view/src/view.rs`
- Modify: `crates/terminal_view/src/ssh_form_window.rs`
- Modify: `crates/terminal_view/src/sidebar/file_manager_panel.rs`
- Modify: `crates/terminal_view/src/sidebar/settings_panel.rs`
- Modify: `crates/terminal_view/src/addon.rs`
- Modify: `crates/terminal_view/src/cd_completion.rs`
- Modify: `crates/terminal_view/src/serial_form_window.rs`

**Step 1:** 在 `terminal_view.yml` 新增节点收录中文字符串翻译（优先产品代码）。
**Step 2:** 各目标文件 `use rust_i18n::t;` 并替换。
**Step 3:** 验证 `cargo check -p terminal_view` 。

### Task 10: 迁移 `db_view` 中文字符串

**Objective:** `db_view/src/**/*.rs` 中的用户可见中文。

**Files:**
- Modify: `crates/db_view/locales/db_view.yml`
- Modify: `crates/db_view/src/**/*.rs` (含 database_form.rs, connection_form_window.rs, chatdb 等)

**Step 1:** 在 `db_view.yml` 新增节点收录中文字符串翻译。
**Step 2:** 替换各 `.rs` 中的中文硬编码。
**Step 3:** 验证 `cargo check -p db_view` 。

### Task 11: 迁移 `ui` 中英文 UI 标签

**Objective:** `ui/src/**/*.rs` 中的英文用户可见字符串（如 `inspector.rs` 中的 `"Reset"`）。

**Files:**
- Modify: `crates/ui/locales/ui.yml`
- Modify: `crates/ui/src/inspector.rs` (及其他含英文硬编码的文件)

**Step 1:** 在 `ui.yml` 新增 `Inspector.reset: Reset` 等。
**Step 2:** 替换 `inspector.rs` 中的英文硬编码。
**Step 3:** 验证 `cargo check -p gpui-component` 。

### Task 12: 迁移 `mongodb_view`, `redis_view`, `ssh`, `sftp`, `sftp_view` 中文字符串

**Objective:** 将这些 view crates 中的中文硬编码迁移到各自的 locales。

**Files:**
- Modify: `crates/mongodb_view/locales/mongodb_view.yml`
- Modify: `crates/redis_view/locales/redis_view.yml`
- Modify: `crates/ssh/locales/ssh.yml`
- Modify: `crates/sftp/locales/sftp.yml`
- Modify: `crates/sftp_view/locales/sftp_view.yml`
- Modify: 各 crate 的 `.rs` 产品代码

**Step 1:** 在各 `locales/*.yml` 中新增节点收录中文字符串翻译。
**Step 2:** 替换各 `.rs` 中的中文硬编码。
**Step 3:** 分别验证 `cargo check -p <crate_name>` 。

---

## 阶段六：`main` 应用层补全

### Task 13: 迁移 `main/src` 中的未国际化字符串

**Objective:** 将 `main/src/**/*.rs` 中的中文/英文硬编码（设置页面、更新对话框、认证等）迁移到 `main.yml`。

**Files:**
- Modify: `main/locales/main.yml`
- Modify: `main/src/**/*.rs`

**Step 1:** 在 `main.yml` 新增必要节点。
**Step 2:** 替换各 `.rs` 中的硬编码字符串。
**Step 3:** 验证 `cargo check -p main` 。

---

## 阶段七：最终检查与修复

### Task 14: 重新扫描未国际化字符串

**Objective:** 运行脚本扫描，确认产品代码中不再有明显的未国际化中文/英文 UI 字符串。

**Files:**
- Run: 重新运行扫描脚本（可用 Python grep 脚本）

**Step 1:** 运行脚本检查各 crate 产品代码中是否还有中文字符串。
**Step 2:** 检查是否还有明显的 UI 英文标签（如 `Label::new("Reset")` 类似模式）。
**Step 3:** 对于测试代码中的中文，可保留或根据情况处理。
**Step 4:** 记录任何残留问题。

### Task 15: 全局编译验证

**Objective:** 确保整个 workspace 编译通过。

**Run:** `cargo check --workspace`

**Expected:** 无错误。

### Task 16: 更新 CLAUDE.md 或 AGENTS.md (可选)

**Objective:** 记录本次国际化经验，以便后续不再重复。

**Files:**
- Modify: `CLAUDE.md` (若存在 i18n 相关约定则更新)

---

## 执行策略

- 采用 `subagent-driven-development`，每个 Task 派发一个子代理执行。
- 每完成一个 Task 后进行 `cargo check`。
- 不在任何时候执行 `git commit` 或 `git push`（用户确授规则）。
