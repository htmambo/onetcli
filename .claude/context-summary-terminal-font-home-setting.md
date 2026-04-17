## 项目上下文摘要（首页终端字体设置）
生成时间：2026-03-26 20:55:57 CST

### 1. 相似实现分析
- **实现1**: `main/src/setting_tab.rs`
  - 模式：首页设置统一写入 `AppSettings`，随后调用 `sync_terminal_settings_to_all(...)` 把变更广播到所有终端实例。
  - 可复用：终端字号、行间距、自动复制、中键粘贴的设置项与持久化写法。
  - 需注意：首页普通 UI 字体与终端字体是两条不同链路，不能混用。

- **实现2**: `main/src/home/home_tabs.rs`
  - 模式：`setup_terminal_view(...)` 在终端创建时应用全局设置；`apply_terminal_settings_to_all(...)` 负责后续批量同步。
  - 可复用：`TerminalViewEvent` 到 `AppSettings` 的持久化逻辑，以及终端批量更新入口。
  - 需注意：主题切换与排版参数分开处理，终端主题变更不能覆盖字号、字体、行间距。

- **实现3**: `crates/terminal_view/src/sidebar/settings_panel.rs`
  - 模式：终端右侧设置面板已经有字体下拉框，候选值来自 `TerminalTheme::available_monospace_fonts()`。
  - 可复用：固定等宽字体列表、`SettingsPanelEvent::FontFamilyChanged`、当前主题同步到设置面板的逻辑。
  - 需注意：首页设置必须复用同一套字体候选，不应另造来源。

- **实现4**: `crates/terminal_view/src/view.rs`
  - 模式：终端排版参数都挂在 `current_theme` 上，通过 `apply_terminal_settings(...)` 或单项 setter 应用。
  - 可复用：`apply_theme(...)` 的 `preserve_theme_typography(...)` 策略。
  - 需注意：此前缺少字体变更向上发射 `TerminalViewEvent`，这是首页设置与右侧设置不同步的缺口。

### 2. 项目约定
- **命名约定**: 全局设置字段统一挂在 `AppSettings`，终端事件统一使用 `TerminalViewEvent::*Changed`。
- **文件组织**: 设置数据模型在 `main/src/setting_tab.rs`，终端同步逻辑在 `main/src/home/home_tabs.rs`，终端局部设置与主题在 `crates/terminal_view/src/*`。
- **代码风格**: 保持 Rust 链式 UI 构建风格，不新增额外状态容器。

### 3. 可复用组件清单
- `main/src/setting_tab.rs::sync_terminal_settings_to_all`
- `main/src/home/home_tabs.rs::setup_terminal_view`
- `main/src/home/home_tabs.rs::apply_terminal_settings_to_all`
- `crates/terminal_view/src/theme.rs::default_monospace_font`
- `crates/terminal_view/src/theme.rs::TerminalTheme::available_monospace_fonts`
- `crates/terminal_view/src/view.rs::preserve_theme_typography`

### 4. 测试策略
- **测试框架**: 当前改动以 Rust 编译验证为主。
- **参考模式**:
  - `cargo fmt --all`
  - `cargo check -p terminal_view -p main`
- **覆盖重点**:
  - 首页设置新增终端字体项后可编译。
  - 右侧设置改终端字体时，事件能写回 `AppSettings`。
  - 首页设置改终端字体时，所有终端实例走统一同步入口。

### 5. 依赖和集成点
- **外部依赖**: 无新增依赖。
- **内部依赖**:
  - `AppSettings` 持久化到 `settings.json`
  - `GlobalHomePage` 用于跨终端实例同步
  - `TerminalTheme` 提供字体候选与默认值
- **集成方式**: 首页设置更新 `AppSettings` 后，通过 `HomePage::apply_terminal_settings_to_all(...)` 下发到每个 `TerminalView`。

### 6. 技术选型理由
- **采用方案**: 继续复用现有 `AppSettings + TerminalViewEvent + HomePage` 的同步链路。
- **优势**: 改动面小，首页设置和右侧设置自然共享同一套终端状态，且不会破坏已有主题保留排版逻辑。
- **风险**: 终端字体事件原本缺口在 `TerminalView` 层，需要补齐后再验证批量同步是否编译通过。

### 7. 关键风险点
- **边界条件**: 旧配置文件没有 `terminal_font_family` 字段时，必须自动回退到平台默认等宽字体。
- **一致性风险**: 主题切换仍需保留终端字体、字号、行间距，不得回退到主题内置排版参数。
- **验证限制**: 当前没有 GUI 自动化，本次以本地编译验证为主，手工交互仍建议后续点验。
