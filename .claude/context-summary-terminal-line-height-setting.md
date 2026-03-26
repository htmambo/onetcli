## 项目上下文摘要（终端行间距设置）
生成时间：2026-03-26 19:49:30 +0800

### 1. 相似实现分析
- **实现1**: `main/src/home/home_tabs.rs:32`
  - 模式：终端全局设置通过 `setup_terminal_view(...)` 在创建时下发，并通过 `TerminalViewEvent` 持久化回 `AppSettings`。
  - 可复用：`FontSizeChanged`、`ThemeChanged`、`AutoCopyChanged` 的持久化与批量同步链路。
  - 需注意：`apply_terminal_settings_to_all(...)` 只同步部分终端设置，新增字段要同步扩展这里。

- **实现2**: `crates/terminal_view/src/sidebar/mod.rs:76`
  - 模式：右侧设置面板事件先转成 `TerminalSidebarEvent`，再由 `TerminalView::handle_sidebar_event(...)` 落到具体 setter。
  - 可复用：`FontSizeChanged`、`FontFamilyChanged` 的事件穿透模式。
  - 需注意：如果新增右侧设置项，需要同时补 `SettingsPanelEvent` 和 `TerminalSidebarEvent` 两层。

- **实现3**: `crates/terminal_view/src/sidebar/settings_panel.rs:454`
  - 模式：设置面板内部直接维护输入框状态，并在 `set_current_theme(...)` 时把主题中的字体信息回写到 UI。
  - 可复用：字体大小输入框的手动输入、步进按钮、抑制回流机制。
  - 需注意：当前 `set_current_theme(...)` 只更新字号和字体，若新增行间距控件，需要一并同步。

- **实现4**: `crates/terminal_view/src/theme.rs:56`
  - 模式：终端主题本身承载 `font_size`、`font_family`、`line_height_scale` 等排版参数。
  - 可复用：`with_line_height_scale(...)`、`line_height()`、默认常量 `DEFAULT_LINE_HEIGHT_SCALE`。
  - 需注意：当前 `line_height_scale` 已是实际生效字段，但未暴露到任何用户设置。

### 2. 项目约定
- **命名约定**: 终端设置字段使用 `terminal_*`，事件使用 `*Changed`。
- **文件组织**: 产品设置在 `main/src/setting_tab.rs` 与 `main/locales/main.yml`；终端侧边栏设置在 `crates/terminal_view/src/sidebar/settings_panel.rs` 与 `crates/terminal_view/locales/terminal_view.yml`。
- **代码风格**: 复用现有同步链路与 setter，不新增第二套配置来源。

### 3. 可复用组件清单
- `TerminalView::set_line_height_scale(...)`
- `TerminalView::line_height_scale()`
- `TerminalTheme::with_line_height_scale(...)`
- `HomePage::apply_terminal_settings_to_all(...)`
- `sync_terminal_settings_to_all(...)`

### 4. 测试策略
- **验证方式**: 编译验证 + 定向代码搜索验证
- **参考命令**: `cargo check -p terminal_view -p main`、`rg 'terminal_line_height' ...`
- **覆盖要求**: 新字段能从首页设置下发、能从右侧设置面板触发、能回写到 `AppSettings`

### 5. 依赖和集成点
- **外部依赖**: 无新增依赖
- **内部依赖**: `AppSettings`、`HomePage` 终端同步、`SettingsPanel`、`TerminalSidebar`、`TerminalView`
- **集成方式**: 复用“终端字号”的全局同步链路，并复用“主题当前值回写侧边栏”的 UI 同步机制

### 6. 技术选型理由
- **为什么用这个方案**: 行间距本来就在 `TerminalTheme` 里，最小修复就是把它接入现有设置链，而不是另造独立状态。
- **优势**: 和终端字号一致，行为可预期，跨 tab 同步与持久化都已有成熟模式。
- **风险**: 需要同时改首页设置和右侧设置，否则用户仍会遇到“有一处能改、另一处不同步”的分裂体验。

### 7. 关键风险点
- **同步一致性**: `apply_terminal_settings(...)`、`apply_theme(...)`、`set_current_theme(...)` 都要包含行间距，否则 UI 会显示旧值。
- **边界条件**: 行间距范围需要统一限制，避免不同入口使用不同 clamp。
- **工具缺失**: 当前环境无法使用仓库要求的 `desktop-commander`、`sequential-thinking`，本次以本地代码证据和编译验证替代。
