## 项目上下文摘要（terminal-sidebar-paste-focus）
生成时间：2026-03-25 21:00:29 +0800

### 1. 相似实现分析
- **实现1**: `crates/terminal_view/src/sidebar/mod.rs`
  - 模式：AI 聊天代码块动作通过 `CodeBlockAction::new("paste-to-terminal")` 注册，点击后发出 `TerminalSidebarEvent::PasteCodeToTerminal(code)`。
  - 可复用：事件总线已经打通，不需要改 AI 面板内部逻辑。
  - 需注意：这里只负责发事件，不持有终端焦点句柄。

- **实现2**: `crates/terminal_view/src/view.rs::handle_sidebar_event`
  - 模式：侧边栏动作统一在 `TerminalView` 侧消费，`PasteCodeToTerminal` 当前只调用 `paste_code_block(...)`。
  - 可复用：这里同时拿到 `window`、`cx` 和终端视图本身，适合补显式聚焦。
  - 需注意：不能在 sidebar 层硬编码终端焦点，应该由 `TerminalView` 自己聚焦自己。

- **实现3**: `crates/terminal_view/src/view.rs::handle_mouse_down`
  - 模式：用户点击终端时通过 `window.focus(&self.focus_handle, cx)` 把输入焦点交给终端。
  - 可复用：这是终端现有、最直接的聚焦方式。
  - 需注意：AI 代码块粘贴后想要继续输入，本质上也应该复用同一套焦点 API。

- **实现4**: `crates/terminal_view/src/view.rs::paste_text / paste_code_block`
  - 模式：所有终端粘贴都统一走 bracketed paste 逻辑，并可能弹出多行/高危确认框。
  - 可复用：如果把聚焦放在 `paste_code_block(...)` 或其调用点，就能复用现有粘贴保护逻辑。
  - 需注意：若弹出确认框，焦点切换不能破坏确认流程。

### 2. 项目约定
- **命名约定**: 侧边栏事件在 `TerminalSidebarEvent`，消费逻辑集中在 `handle_sidebar_event(...)`。
- **文件组织**: 终端输入、粘贴与焦点行为都放在 `crates/terminal_view/src/view.rs`，sidebar 仅发意图事件。
- **代码风格**: 优先复用 `focus_handle` 和 `window.focus(...)` 现有模式，不新增新的事件或状态字段。

### 3. 可复用组件清单
- `crates/terminal_view/src/sidebar/mod.rs::TerminalSidebarEvent::PasteCodeToTerminal`
- `crates/terminal_view/src/view.rs::handle_sidebar_event`
- `crates/terminal_view/src/view.rs::paste_code_block`
- `crates/terminal_view/src/view.rs::handle_mouse_down`
- `crates/terminal_view/src/view.rs::focus_handle`

### 4. 测试策略
- **测试框架**: 以 `cargo check` / `cargo test` 为主
- **验证方式**: 编译验证终端视图主路径不回归；手工确认“点击 AI 代码块的粘贴到终端后可直接继续输入”
- **覆盖重点**: 粘贴事件仍能到达终端；聚焦调用不破坏现有多行/高危确认流程

### 5. 依赖和集成点
- **外部依赖**: `gpui::Window::focus`
- **内部依赖**: `TerminalView.focus_handle`、`TerminalSidebarEvent`
- **集成方式**: 在 `PasteCodeToTerminal` 处理路径上补一条显式聚焦

### 6. 技术选型理由
- **为什么用这个方案**: 焦点问题属于终端视图自己的职责边界，应该在 `TerminalView` 消费 sidebar 事件时解决。
- **优势**: 改动最小、行为可预测、不会耦合 AI 面板和终端内部实现。
- **风险**: 如果粘贴会触发确认弹窗，需要确认补焦点不会影响弹窗交互。

### 7. 工具说明
- 仓库说明优先使用 `desktop-commander`、`context7`、`github.search_code`，但本次会话未提供这些工具；已改用本地代码检索和 Rust 编译验证留痕。
