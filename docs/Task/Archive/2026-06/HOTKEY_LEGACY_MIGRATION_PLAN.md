# 旧版激活热键 ctrl+space 迁移任务计划

**状态**: ✅ 已完成 (完成时间: 2026-06-10)

## 背景

旧版本 onetcli 默认的系统级激活快捷键是 `ctrl+space`，会写入到用户配置文件的
`system_hotkey_macos` / `system_hotkey_other` 字段中。

后续版本将该默认值改为：

- macOS：`cmd-alt-m`（`DEFAULT_SYSTEM_HOTKEY_MACOS`）
- Windows / Linux：`ctrl-alt-m`（`DEFAULT_SYSTEM_HOTKEY_OTHER`）

由于旧配置已落盘为 `ctrl-space`，用户在手动改回新默认值之前，激活热键仍是
`ctrl+space`，与系统输入法切换冲突，体验差。

## 目标

应用启动时：

1. 读取已保存的 `system_hotkey_macos` / `system_hotkey_other`。
2. 若字段值仍为 `ctrl-space`（不区分大小写、按 trim 处理），则：
   - 改写为对应平台的新默认值
   - 立即持久化到 `settings.json`
   - 在主窗口内推送一条 toast 通知，告知用户此变更
3. 仅在字段仍为 `ctrl-space` 时触发，已被用户改过其他值则不动。

## 涉及文件

| 文件 | 变更 |
| --- | --- |
| `main/src/setting_tab.rs` | 新增 `migrate_legacy_system_hotkey` 函数；在 `init_settings` 中调用并返回 `HotkeyMigration { macos_changed, other_changed }` |
| `main/src/main.rs` | 在主窗口创建后、调用 `OnetCliApp::new` 之前，根据迁移结果 `push_notification` |
| `main/locales/main.yml` | 新增 `Settings.Migrations.ctrl_space_toast` i18n 条目 |
| `main/src/setting_tab.rs`（测试） | 在 `mod tests` 中追加单元测试 |

## 设计要点

1. **纯函数式迁移函数**：
   `migrate_legacy_system_hotkey(settings: &mut AppSettings) -> HotkeyMigration`
   只负责检测 + 改写字段，**不**做 I/O 之外的动作，方便单测。
2. **持久化**：
   迁移后调用 `AppSettings::save_global(cx)`（已存在于 `setting_tab.rs:1067`），
   避免重复实现写盘逻辑。
3. **通知触发位置**：
   `init_settings(cx: &mut App)` 阶段 `Window` 还未创建，**不能**直接 `push_notification`。
   改为让 `init_settings` 返回 `HotkeyMigration` 结构，主窗口 `cx.open_window` 闭包内
   拿到 `&mut Window` 后再 `window.push_notification(...)`。
4. **i18n key**：
   - key: `Settings.Migrations.ctrl_space_toast`
   - en: `System hotkey has been migrated from Ctrl+Space to {new_key} to avoid conflicting with the system input method.`
   - zh-CN: `为避免与系统输入法切换冲突，已将"激活窗口"快捷键从 Ctrl+Space 更改为 {new_key}。`
   - zh-HK: `為避免與系統輸入法切換衝突，已將"啟用視窗"快捷鍵從 Ctrl+Space 更改為 {new_key}。`
5. **判定规则**：
   - 严格按 `== "ctrl-space"`（不区分大小写、`trim`）。
   - 不匹配"super-space" / "cmd-space" / 用户自定义过的其他值。

## 验收标准

- [x] `cargo check -p main` 0 error
- [x] `cargo test -p main --bin onetcli hotkey_migration` 5 passed
- [x] 新增的迁移单元测试覆盖：迁移触发 / 不触发 / 跨平台分支 / 大小写与空白
- [x] 实际启动时，当配置仍为 `ctrl-space`，主窗口弹出 toast 通知且新值已写入磁盘
- [x] 启动后再次启动，配置已被改写，**不再**重复弹 toast

> 注：执行 `cargo test -p main --bin onetcli` 时，预先存在的
> `setting_tab::tests::global_proxy_settings_validate_required_fields` 失败
> （断言中文 "主机" 字符），与本次改动无关，已通过 `git stash` 验证其在改动前
> 即失败。

## 风险评估

- **风险 1**：迁移把用户手动改过的 `ctrl-space` 也覆盖。
  **缓解**：仅匹配精确字符串 `ctrl-space`；用户若已主动改成其他值不会被命中。
- **风险 2**：`init_settings` 返回值变更破坏调用方。
  **缓解**：调用方 `main.rs` 在本次同步修改，保持一一对应。
- **风险 3**：toast 文案在某些语言下换行 / 长度异常。
  **缓解**：使用 `Notification::info(...).autohide(true)`，与现有 toast 一致。

## 实施顺序

1. 在 `setting_tab.rs` 添加 `migrate_legacy_system_hotkey` 函数 + `HotkeyMigration` 结构 + 单元测试
2. 在 `main.yml` 新增 i18n 条目
3. 修改 `init_settings` 返回 `HotkeyMigration`
4. 修改 `main.rs` 的 `init_settings` 调用点，把 `HotkeyMigration` 透传到 `open_window` 闭包
5. `cargo check` / `cargo test -p main` 验证

## 备注

- 实现时遇到的小插曲：`init_settings` 在 `setting_tab.rs` 内部被 `SettingsPanel::on_activate`
  和 `SettingsPanel::render` 用作兜底（实际不会被触发，因为全局在 `main.rs` 启动时已注册），
  因此将这两处改为 `let _ = init_settings(cx);` 以吞掉新增的返回值。
- i18n key 选择 `Settings.Migrations.ctrl_space_toast` 放在已有的 `Settings` 命名空间下，
  便于以后扩展其它迁移类通知。
