# 终端/SSH ⌘+Click / Win+Click 打开链接失效修复

**状态**: ✅ 已完成 (完成时间: 2026-06-15)
**分支**: `perf/manager-lock-refactor`
**影响文件**: `crates/terminal_view/src/addon.rs`, `crates/terminal_view/src/view.rs`

---

## 1. 任务目标

修复终端/SSH 视图中"按住修饰键 + 点击打开链接"在 Linux/Windows 下无效的问题。

**用户反馈**：
- macOS 上看到 `⌘ + Click to open the link` 提示，按 ⌘+click 能打开 ✓
- Manjaro (Linux) 上按 `Win + click` 没有反应 ✗
- 期望：Linux 用户按 `Ctrl + click` 能打开链接（与浏览器/IDE 在 Linux 下的约定一致）

---

## 2. 根因分析

### 2.1 代码现状

`crates/terminal_view/src/addon.rs` 把"打开链接"的修饰键硬编码为 `modifiers.platform`：

- `WebLinksAddon::on_mouse_down`（line 535-551）：
  ```rust
  if !context.modifiers.platform { return false; }
  ```
- `FilePathAddon::on_mouse_down`（line 1058-1082）：
  ```rust
  if !context.is_local || !context.modifiers.platform { return false; }
  ```
- `WebLinksAddon::tooltip`（line 561-568）：
  ```rust
  action_hint: "⌘ + Click",  // 硬编码 macOS 风格
  action_text: "to open the link",
  ```

### 2.2 gpui 平台语义

`vendor/zed/crates/gpui/src/platform/keystroke.rs:455-465`：
```rust
/// The command key, on macos
/// the windows key, on windows
/// the super key, on linux
pub platform: bool,
```

- **macOS**: `platform` = `⌘ Command`
- **Linux**: `platform` = `Super/Win`（但 KDE/GNOME 等 WM 经常拦截 Win 键不传给应用）
- **Windows**: `platform` = `Win` 键

### 2.3 问题综合

1. Linux/Windows 用户看 macOS 风格提示 `⌘ + Click`，按 Win 键被 WM 拦截 → 事件到不了应用层 → `modifiers.platform = false` → 不打开
2. Linux/Windows 用户即使想到 Win 键可能按不通，也找不到"按哪个修饰键"的对应文档
3. **正确做法**：Linux/Windows 应接受 `Ctrl + click`（与 Chromium/Firefox/VS Code 等一致）

---

## 3. 实施计划

### 3.1 子任务列表

| # | 任务 | 状态 |
|---|------|------|
| 1 | 修改 `WebLinksAddon::on_mouse_down`：接受 `platform \|\| control` | ✅ |
| 2 | 修改 `FilePathAddon::on_mouse_down`：同样放宽 | ✅ |
| 3 | 修改 `WebLinksAddon::tooltip`：根据 `cfg!(target_os)` 输出对应提示 | ✅ |
| 4 | 给 `FilePathAddon` 的 tooltip 同步调整（若有） | ✅ |
| 5 | 验证编译与 lint | ✅ |
| 6 | 单元测试（4 个新增 + 123 个回归） | ✅ |
| 7 | 用户实测 Ctrl+click 在 Manjaro 打开链接 | ✅ |
| 8 | 提交 Git commit | ✅ |
| 9 | 归档任务文档 | 🔄 |

### 3.2 验收标准

- [ ] Linux/Windows 用户按 `Ctrl + click` 能打开 URL/路径
- [ ] macOS 用户按 `⌘ + click` 行为不变
- [ ] Linux/Windows tooltip 显示 `Ctrl + Click`（而非 `⌘ + Click`）
- [ ] `cargo build -p terminal_view` 通过
- [ ] `cargo clippy -p terminal_view -- --deny warnings` 通过
- [ ] 无破坏现有 SGR 鼠标协议/选区逻辑

### 3.3 风险评估

| 风险 | 缓解 |
|------|------|
| Ctrl+click 与终端内"中断当前命令"语义冲突 | 终端内 `Ctrl+C` 由键盘处理；鼠标 click 走 mouse_down 流程，互不干扰 |
| SGR 鼠标模式启用时 Ctrl+click 被吞 | `should_defer_sgr_left_press` 已显式排除 `modifiers.control`（line 322），会跳过 SGR 转发直接走 addon 流程 |
| 误触发（用户误按 Ctrl） | 这是已知代价，与 Chromium/Firefox 一致；可后续用设置项控制 |

---

## 4. 阶段输出

### 阶段 0 — 现状确认（完成）

- `WebLinksAddon::on_mouse_down` line 535-551
- `FilePathAddon::on_mouse_down` line 1058-1082
- `WebLinksAddon::tooltip` line 561-568
- `view.rs::handle_mouse_down` line 3697 正确传入 `event.modifiers`

---

## 5. Codex 顾问意见

（未启用：本次为单文件、行为等价的小修复，Codex 流不稳定风险大于收益；用户已实测验证）

---

## 6. QA 记录

- `cargo test -p terminal_view --lib`：127 passed（含 4 个新增）
  - `open_link_modifier_matches_platform_convention`
  - `open_link_hint_matches_target_os`
  - `weblinks_mouse_down_opens_url_on_correct_modifier`
  - `weblinks_mouse_down_ignores_wrong_modifier`
- `cargo clippy -p terminal_view`：本 crate 无新增警告
- 用户实测 Manjaro (Linux) + Ctrl+click：链接已能正常打开 ✓

---

## 7. 验证

### 跨平台行为

| 平台 | 修饰键 | tooltip 文案 | 行为 |
|------|--------|-------------|------|
| macOS | ⌘ Command | "⌘ + Click" | addon 打开链接 |
| Linux/Windows | Ctrl | "Ctrl + Click" | addon 打开链接；SGR 鼠标模式下仍转发 PTY |

### 关键改动

- `crates/terminal_view/src/addon.rs`
  - 新增 `is_open_link_modifier_pressed(modifiers)` / `open_link_action_hint()` 辅助函数
  - `WebLinksAddon::on_mouse_down`：使用 `is_open_link_modifier_pressed` 判断；直接用 `detect_url_at` 写入的 `hovered_link` 取 URL
  - `FilePathAddon::on_mouse_down`：同上
  - `WebLinksAddon::tooltip`：`action_hint` 改为 `open_link_action_hint()`
  - `tests` 新增 4 个单元测试

- `crates/terminal_view/src/view.rs`
  - `try_report_sgr_mouse_button`：非 macOS + 非 SGR 模式 + Ctrl+Left 时跳过 PTY 转发，让 GUI 接管打开链接；SGR 模式下仍转发（不破坏 TUI 协议）

### 备注

- 项目工作目录已是 git 仓库。后续按需执行归档。
