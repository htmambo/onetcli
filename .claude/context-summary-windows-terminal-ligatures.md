## 项目上下文摘要（Windows 终端连字）
生成时间：2026-03-28 23:44:07 +08:00

### 1. 相似实现分析
- **实现1**: `crates/terminal_view/src/terminal_element.rs`
  - 模式：终端真实绘制统一通过 `FontVariants` 构造字体，并复用 `terminal_font_features(...)` 控制 OpenType 特性。
  - 可复用：终端字体特性应在单一 helper 中集中定义，避免绘制路径分叉。
  - 需注意：此前开启连字时返回空特性列表，Windows 下会形成“空 Typography”。

- **实现2**: `crates/terminal_view/src/view.rs`
  - 模式：字宽测量与渲染共用 `terminal_font_features(...)`，保证列宽、光标和选区与真实绘制一致。
  - 可复用：本次修复只需调整 helper，本身就能同时覆盖测量和绘制。
  - 需注意：不能只改实际绘制，否则终端栅格会错位。

- **实现3**: `vendor/zed/crates/gpui/src/platform/windows/direct_write.rs`
  - 模式：Windows 使用 `IDWriteTypography` 承载字体特性，并在 `apply_font_features(...)` 中处理 `liga`、`clig`、`calt`。
  - 可复用：DirectWrite 已支持显式启停三类连字特性，不需要新增平台分支。
  - 需注意：当 `FontFeatures` 为空时，`apply_font_features(...)` 会直接返回，最终仍会把空 Typography 绑定到文本布局。

- **实现4**: `crates/terminal_view/src/theme.rs`
  - 模式：Windows 默认终端字体为 `Consolas`，备用字体才包含 `Cascadia Code`、`Fira Code`、`JetBrains Mono` 等支持连字的字体。
  - 可复用：默认字体策略暂不变，本次先修复字体特性表达。
  - 需注意：即便代码修复完成，若用户继续使用 `Consolas`，仍不会看到编程连字。

### 2. 项目约定
- **命名约定**: 终端排版 helper 使用 `terminal_*` 前缀，测试命名采用行为描述风格。
- **文件组织**: 终端字体特性定义在 `terminal_element.rs`，测量路径在 `view.rs`，平台实现保留在 `vendor/zed`。
- **代码风格**: 优先修正现有 helper，不引入新的终端设置对象或平台分支开关。

### 3. 可复用组件清单
- `crates/terminal_view/src/terminal_element.rs::terminal_font_features`
- `crates/terminal_view/src/terminal_element.rs::FontVariants`
- `crates/terminal_view/src/view.rs::render`
- `vendor/zed/crates/gpui/src/platform/windows/direct_write.rs::apply_font_features`

### 4. 测试策略
- **测试框架**: Rust 内联单元测试
- **参考模式**:
  - `crates/terminal_view/src/keys.rs`
  - `crates/terminal_view/src/view.rs`
- **验证重点**:
  - 开启连字时显式输出 `liga/clig/calt = 1`
  - 关闭连字时显式输出 `liga/clig/calt = 0`
  - `cargo test -p terminal_view --lib`
  - `cargo check -p terminal_view`

### 5. 依赖和集成点
- **外部依赖**: 无新增依赖
- **内部依赖**:
  - `TerminalView` 依赖 `terminal_font_features(...)` 做字宽测量
  - `TerminalElement` 依赖同一 helper 做真实绘制
  - `gpui` Windows 文本系统负责把 `FontFeatures` 转成 DirectWrite typography

### 6. 技术选型理由
- **采用方案**: 让终端 helper 始终显式声明 `liga`、`clig`、`calt` 三个特性，而不是在开启时返回空列表。
- **优势**: 修复面最小，同时统一 Windows 和非 Windows 的开关语义。
- **风险**: 若用户字体本身不支持编程连字，开启后仍看不到效果；这是字体能力限制，不是终端开关链路问题。

### 7. 关键风险点
- **平台差异**: Windows DirectWrite 对空 Typography 的行为与 macOS/Linux 现状不一致，是本次首要风险点。
- **用户感知**: Windows 默认 `Consolas` 不支持编程连字，修复后仍可能需要用户切换到支持连字的字体。
- **一致性风险**: 必须保持测量与绘制继续共用同一 helper，避免列宽错位。
