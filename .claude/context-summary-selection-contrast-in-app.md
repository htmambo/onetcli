## 项目上下文摘要（selection-contrast-in-app）
生成时间：2026-03-25 17:31:06 +0800

### 1. 相似实现分析
- **实现1**: `crates/ui/src/text/inline.rs`
  - 模式：`TextView` 选择态由 `Inline::paint_selection(...)` 统一绘制
  - 结论：AI 消息和可选富文本的选区问题由这里决定

- **实现2**: `crates/ui/src/input/element.rs`
  - 模式：输入框多行/单行文本先布局 `TextRun`，再在绘制阶段画选区背景
  - 结论：输入框已具备按背景区间自动切黑/白字的能力，只是没把 selection 区间接进来

- **实现3**: `crates/ui/src/theme/schema.rs` 与 `crates/ui/src/theme/default-theme.json`
  - 模式：`selection`、`list_active`、`table_active` 都由主题 token 控制，且主题应用阶段之前会强行压低 alpha
  - 结论：这是列表、表格和文本选区对比度整体偏低的公共来源

### 2. 项目约定
- **命名约定**: UI 状态色统一由 `ThemeColor` token 提供，不在业务页面直接硬编码
- **文件组织**: 文字选区逻辑收敛在 `crates/ui/src/text` 和 `crates/ui/src/input`，默认配色收敛在 `crates/ui/src/theme`
- **代码风格**: 尽量复用现有渲染辅助函数，不为单个页面追加特判

### 3. 可复用组件清单
- `crates/ui/src/input/element.rs::split_runs_by_bg_segments`
- `crates/ui/src/text/inline.rs::paint_selection`
- `crates/ui/src/theme/default-theme.json`
- `crates/ui/src/theme/schema.rs::apply_config`

### 4. 测试策略
- **验证方式**:
  - `cargo fmt --all`
  - `cargo check -p main`
  - `cargo test -p gpui-component input::element::tests --lib`
- **覆盖重点**:
  - 选区背景与文本的绘制顺序
  - 输入框选区是否接入自动前景色切换
  - 默认主题 token 是否不再被强制压低透明度

### 5. 技术结论
- AI 消息选中发灰的直接原因：`TextView` 先画文字、后画选区背景，半透明遮罩盖在文字上
- 输入框选区对比不足的直接原因：只画 `selection` 背景，不切换选中文字前景色
- 列表/表格选中不明显的公共原因：默认主题值偏淡，且主题应用阶段又额外压低 alpha
