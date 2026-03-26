## 项目上下文摘要（终端侧边栏设置翻译补齐）
生成时间：2026-03-26 19:43:38 +0800

### 1. 相似实现分析
- **实现1**: `crates/terminal_view/src/sidebar/file_manager_panel.rs:507`
  - 模式：侧边栏输入框占位符直接使用 `t!("FileManager.search_placeholder")`。
  - 可复用：`InputState::new(...).placeholder(t!(...))` 的本地化接入方式。
  - 需注意：同一 crate 内的面板文案都集中在 `crates/terminal_view/locales/terminal_view.yml`。

- **实现2**: `main/src/setting_tab.rs:637`
  - 模式：设置页标题、字段名、描述均通过 `t!("Settings.General.*")` 读取词条。
  - 可复用：设置类页面应优先用明确的词条键，而不是直接写死英文。
  - 需注意：用户当前反馈的 `Font Size`、`Font Family` 已在主设置页存在成熟翻译，可沿用命名语义。

- **实现3**: `crates/terminal_view/src/sidebar/quick_command_panel.rs:273`
  - 模式：终端侧边栏其他面板的 tooltip、对话框标题、按钮说明都已走 `QuickCommand.*` 词条。
  - 可复用：在 `terminal_view.yml` 中为面板补齐专属键，再由面板直接 `t!()` 读取。
  - 需注意：当前 `settings_panel.rs` 是该侧边栏里少数仍残留硬编码英文的文件。

### 2. 项目约定
- **命名约定**: 面板文案键以功能域分组，如 `Settings.*`、`QuickCommand.*`、`FileManager.*`。
- **文件组织**: 终端侧边栏 UI 在 `crates/terminal_view/src/sidebar/`，对应翻译在 `crates/terminal_view/locales/terminal_view.yml`。
- **代码风格**: 可见文案优先用 `t!()`，避免在组件树里直接写死英文字符串。

### 3. 可复用组件清单
- `rust_i18n::t`
- `InputState::placeholder(...)`
- `Select::placeholder(...)`
- `Common.settings` 与 `Common.cancel` 等公共文案键

### 4. 测试策略
- **验证方式**: 定向搜索面板里的英文硬编码 + `cargo check -p terminal_view`
- **参考命令**: `rg` 扫描 `settings_panel.rs` 中的可见英文标签、占位符、说明文案
- **覆盖要求**: 至少覆盖标题、搜索区、字体区、主题区、占位符和说明提示

### 5. 依赖和集成点
- **外部依赖**: 无新增依赖
- **内部依赖**: `settings_panel.rs`、`terminal_view.yml`、全局 `Common.*` 词条
- **集成方式**: 仅替换可见字符串来源，不改动事件流和设置状态

### 6. 技术选型理由
- **为什么用这个方案**: 问题本质是面板还在直接渲染英文字符串，最小修复就是补词条并统一改成 `t!()`。
- **优势**: 改动范围小，不影响终端设置逻辑，也便于后续继续审计同类面板。
- **风险**: 只做静态翻译接入，未自动化验证每个语言在 GUI 上的最终排版。

### 7. 关键风险点
- **遗漏风险**: 需要确认设置面板里所有可见英文都被覆盖，而不是只改用户点名的三处。
- **文案准确性**: 搜索提示文案不能继续保留旧的 macOS 快捷键描述，需与当前交互一致。
- **工具缺失**: 当前环境无法使用仓库要求的 `desktop-commander`、`sequential-thinking`，本次以本地搜索、代码证据和编译验证替代。
