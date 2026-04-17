## 项目上下文摘要（Windows SSH 连字补查）
生成时间：2026-03-28 23:56:47 +08:00

### 1. 相似实现分析
- **实现1**: `main/src/home/home_tabs.rs`
  - 模式：新建 SSH 终端和从连接恢复对话框恢复的 SSH 终端都会走 `open_ssh_terminal(...)`，随后调用 `setup_terminal_view(...)`。
  - 可复用：终端设置同步链路对 SSH 没有单独分支。
  - 需注意：这说明“恢复的 SSH 没收到终端设置”不是高概率根因。

- **实现2**: `main/src/home_tab.rs`
  - 模式：连接恢复对话框的确认动作最终会调用 `restore_saved_connection_sessions(...) -> restore_connection_restore_item(...) -> open_ssh_terminal(...)`。
  - 可复用：恢复 SSH 与手动打开 SSH 共用同一入口。
  - 需注意：恢复逻辑本身没有绕过 `setup_terminal_view(...)`。

- **实现3**: `crates/terminal_view/src/view.rs`
  - 模式：本地终端与 SSH 终端共用同一 `TerminalView`，字体、连字、字宽测量逻辑一致。
  - 可复用：终端主体层已经修成显式控制 `liga/clig/calt`，SSH 终端文字本体理论上应同样受益。
  - 需注意：若恢复 SSH 仍异常，范围更可能在平台层默认字体特性处理。

- **实现4**: `vendor/zed/crates/gpui/src/platform/windows/direct_write.rs`
  - 模式：Windows 所有字体特性最终都经由 `apply_font_features(...)` 注入 `IDWriteTypography`。
  - 可复用：这里是平台统一入口，适合修复空 `FontFeatures` 导致的默认连字丢失。
  - 需注意：此前空特性列表会直接返回，最后仍把空 Typography 绑定到文本布局。

- **实现5**: `vendor/zed/crates/gpui/src/text_system/font_features.rs`
  - 模式：`FontFeatures::default()` 广泛用于通用 UI 文本；`disable_ligatures()` 之前只关闭 `calt`。
  - 可复用：这里适合统一“关闭连字”的三标签语义。
  - 需注意：在 Windows 下只关 `calt` 仍可能留下 `liga/clig`。

### 2. 项目约定
- **命名约定**: 平台 helper 使用 `snake_case`，测试名使用行为描述式命名。
- **文件组织**: 终端业务逻辑留在 `terminal_view`，Windows 文本差异尽量收敛在 `vendor/zed/crates/gpui/src/platform/windows/`。
- **代码风格**: 优先修公共平台入口，不继续在 SSH 业务层打特判。

### 3. 可复用组件清单
- `main/src/home/home_tabs.rs::setup_terminal_view`
- `main/src/home/home_tabs.rs::open_ssh_terminal`
- `main/src/home_tab.rs::restore_saved_connection_sessions`
- `vendor/zed/crates/gpui/src/platform/windows/direct_write.rs::apply_font_features`
- `vendor/zed/crates/gpui/src/text_system/font_features.rs::disable_ligatures`

### 4. 测试策略
- **测试框架**: Rust 内联单元测试
- **参考文件**:
  - `vendor/zed/crates/gpui/src/platform/windows/direct_write.rs`
  - `vendor/zed/crates/gpui/src/text_system/font_features.rs`
- **验证重点**:
  - 空 `FontFeatures` 在 Windows 上也会解析出默认 `liga/clig/calt = 1`
  - 显式覆盖可以替换默认值
  - `disable_ligatures()` 会同时关闭 `liga/clig/calt`

### 5. 依赖和集成点
- **外部依赖**: 无新增依赖
- **内部依赖**:
  - `gpui::Font` 默认使用 `FontFeatures::default()`
  - Windows DirectWrite 负责把 `FontFeatures` 注入文本布局
  - 终端连接恢复对话框最终仍会新建 `TerminalView`

### 6. 技术选型理由
- **采用方案**: 在 Windows DirectWrite 层把空特性列表解析成显式默认 `liga/clig/calt = 1`，并统一 `disable_ligatures()` 的三标签关闭语义。
- **优势**: 能覆盖 SSH 页面里仍在使用默认 `FontFeatures` 的文本，不依赖恢复路径或业务时序。
- **风险**: 这会让 Windows 下所有支持连字的默认文本恢复平台默认连字；若个别区域不希望连字，应显式关闭。

### 7. 关键风险点
- **平台影响面**: 改动位于 `vendor/zed`，会影响 Windows 下所有文本渲染。
- **验证限制**: 当前环境缺少 `nasm` / `cmake`，无法完成完整编译级验证。
- **用户感知**: 若恢复 SSH 页面仍使用不支持连字的字体，例如 `Consolas`，修复后仍不会出现编程连字。
