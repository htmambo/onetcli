## 项目上下文摘要（终端主题切换字号保持）
生成时间：2026-03-26 19:31:07 +0800

### 1. 相似实现分析
- **实现1**: `crates/terminal_view/src/view.rs:763`
  - 模式：`set_theme` 直接用传入主题整体覆盖 `current_theme`。
  - 可复用：现有主题切换入口与行高刷新逻辑。
  - 需注意：传入主题来自主题列表默认值，包含默认字号 13。

- **实现2**: `crates/terminal_view/src/view.rs:809`
  - 模式：`apply_terminal_settings` 单独负责把全局终端字号写回当前主题，并同步侧边栏状态。
  - 可复用：现有字号 clamp、侧边栏同步、行为开关同步逻辑。
  - 需注意：它与 `apply_theme` 分开执行，后者会再次覆盖字号。

- **实现3**: `main/src/home/home_tabs.rs:41` 与 `main/src/home/home_tabs.rs:161`
  - 模式：创建终端和批量同步时，先取 `AppSettings::terminal_font_size`，再单独应用主题。
  - 可复用：终端设置由主应用统一分发的架构。
  - 需注意：主题同步使用 `TerminalTheme::find_by_name(...)`，得到的主题对象仍然带默认字号。

### 2. 项目约定
- **命名约定**: 终端行为函数使用 `set_*`、`apply_*` 区分是否对外 emit 事件。
- **文件组织**: 终端展示逻辑集中在 `crates/terminal_view/src/view.rs`，针对性测试也放在同文件尾部。
- **代码风格**: 优先抽出纯函数帮助逻辑复用，并在现有测试模块就近补回归测试。

### 3. 可复用组件清单
- `TerminalTheme::with_font_size(...)`
- `TerminalTheme::with_font_family(...)`
- `TerminalTheme::with_font_fallbacks(...)`
- `TerminalTheme::with_line_height_scale(...)`

### 4. 测试策略
- **测试框架**: Rust 内置单元测试
- **参考位置**: `crates/terminal_view/src/view.rs` 现有 `#[cfg(test)]` 模块
- **覆盖要求**: 验证切换主题后主题名/颜色会变化，但字号、字体族、备用字体和行高比例保持不变

### 5. 依赖和集成点
- **外部依赖**: 无新增依赖
- **内部依赖**: `TerminalView::set_theme`、`TerminalView::apply_theme`、`main/src/home/home_tabs.rs` 的现有调用链
- **集成方式**: 保持现有主题切换入口不变，只修正主题对象进入 `current_theme` 前的合并逻辑

### 6. 技术选型理由
- **为什么用这个方案**: 根因是“主题对象覆盖了排版参数”，所以应该在主题切换入口保留当前排版参数，而不是额外修补设置面板显示。
- **优势**: 改动最小，既修当前终端，也修跨 tab 的主题同步分支。
- **风险**: 需要确保保留的是排版参数而不是颜色参数，避免主题切换失效。

### 7. 关键风险点
- **行为一致性**: `set_theme` 和 `apply_theme` 必须共用同一保留逻辑，否则本地切换和跨 tab 同步会继续分叉。
- **回归风险**: 如果只按主题名判断是否需要更新，会掩盖字号/字体等排版差异。
- **工具缺失**: 仓库规范要求的 `desktop-commander`、`sequential-thinking` 等工具在当前环境不可用，本次以本地代码证据与单测替代。
