## 项目上下文摘要（unused-warning-audit）
生成时间：2026-03-29 13:40:48 +0800

### 1. 相似实现分析
- **实现1**: `crates/ui/src/title_bar.rs:30-40`
  - 模式：按平台拆分函数实现，为非目标平台提供稳定返回值。
  - 可复用：`linux_prefers_system_window_controls()` 的 `#[cfg(target_os = "linux")]` / `#[cfg(not(target_os = "linux"))]` 双实现方式。
  - 需注意：平台差异放在定义层，调用方可以保持公共入口，不会在非目标平台留下私有死代码。

- **实现2**: `main/src/update.rs:631-666`
  - 模式：公共入口函数内部只做平台分派，具体实现函数本身带 `#[cfg(...)]`。
  - 可复用：`start_install_update(...)`、`apply_update_helper(...)` 与 `spawn_windows_helper(...)` / `apply_update_unix(...)` 的分层方式。
  - 需注意：只有真实会被当前目标平台编译到的实现会进入符号表。

- **实现3**: `crates/sftp_view/src/lib.rs:303-329`
  - 模式：公共函数保留，平台专属 helper 使用 `#[cfg(...)]` 隔离。
  - 可复用：`format_local_permissions(...)` + `format_windows_permissions(...)` 的组合方式。
  - 需注意：共享入口必须在所有平台都真实被调用，否则私有 helper 仍可能形成 dead code。

### 2. 当前 warning 清单
- `crates/ui/src/window_ext.rs:5`
  - `process::Command` 未使用
- `crates/ui/src/window_ext.rs:8`
  - `MacTitlebarDoubleClickAction` 从未使用
- `crates/ui/src/window_ext.rs:14`
  - `resolve_macos_titlebar_double_click_action` 从未使用
- `main/src/home_tab.rs:3218`
  - `connection_list_view_mode_label` 从未使用
- `main/src/setting_tab.rs:87`
  - `SettingsPanelPage::Certificate` 从未构造
- `main/src/setting_tab.rs:633`
  - `set_main_window_bounds` 从未使用

### 3. 分类结论
- **确认属于平台专用逻辑落在公共区**
  - `crates/ui/src/window_ext.rs`
  - 原因：`MacTitlebarDoubleClickAction`、`resolve_macos_titlebar_double_click_action(...)`、`Command` 只在 `handle_titlebar_double_click()` 的 `#[cfg(target_os = "macos")]` 分支中使用；Linux 构建时调用点被裁掉，但私有定义仍被保留，因此形成 dead code。

- **不属于平台问题，属于通用残留代码**
  - `main/src/home_tab.rs:3218`
  - 原因：调用点 `main/src/home_tab.rs:2737` 被注释掉，函数自然失去引用。
  - `main/src/setting_tab.rs:87`
  - 原因：证书页本体仍存在于 `main/src/setting_tab.rs:1291-1298`，但外部只请求了 `SettingsPanelPage::Account`，没有任何地方请求 `Certificate`。
  - `main/src/setting_tab.rs:633`
  - 原因：窗口 bounds 更新链路已经改成 `snapshot_main_window_bounds(...)` + `set_global_main_window_bounds(...)`，旧实例方法被完全绕过。

### 4. 调用链证据
- `crates/ui/src/window_ext.rs:216-234`
  - `handle_titlebar_double_click()` 在 macOS 才会读取系统偏好并使用 `MacTitlebarDoubleClickAction`。
- `crates/ui/src/title_bar.rs:339`
  - 标题栏双击统一调用 `window.handle_titlebar_double_click()`，说明公共入口保留是合理的，问题在于 macOS 解析逻辑未被一起下沉。
- `main/src/home_tab.rs:2732-2739`
  - `connection-view-mode-button` 只保留图标和 tooltip，`label(...)` 已被注释。
- `main/src/home/home_tabs.rs:804`
  - 当前仅有 `SettingsPanelPage::Account` 被主动请求。
- `main/src/onetcli_app.rs:425,467,495`
  - 主窗口 bounds 现在通过快照和全局更新链路保存，不再走 `set_main_window_bounds(...)`。

### 5. 可复用组件清单
- `crates/ui/src/title_bar.rs::linux_prefers_system_window_controls`
- `main/src/update.rs::start_install_update`
- `main/src/update.rs::apply_update_helper`
- `crates/sftp_view/src/lib.rs::format_local_permissions`

### 6. 测试与验证方式
- 构建命令：`cargo check -p main --message-format short`
- 用途：同时覆盖 `main` 自身与其依赖 crate 的 `unused` / `dead_code` warning。
- 本轮结果：确认 `main` 3 条、`gpui-component` 3 条，总计 6 条当前相关 warning。

### 7. 建议的整理顺序
- 第一步：处理 `crates/ui/src/window_ext.rs`，把 macOS 专属类型、helper 和 import 收敛到 `#[cfg(any(target_os = "macos", test))]`。
- 第二步：处理 `main/src/setting_tab.rs::set_main_window_bounds(...)`，直接删除或恢复调用链，二选一，不建议继续悬空。
- 第三步：处理 `SettingsPanelPage::Certificate`，要么补导航入口，要么删除枚举分支与索引映射。
- 第四步：处理 `connection_list_view_mode_label(...)`，要么恢复按钮文案，要么删除 helper。
