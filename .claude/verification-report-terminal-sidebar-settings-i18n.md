## 审查报告 - 终端侧边栏设置翻译补齐
时间：2026-03-26 19:43:38 +0800

### 需求核对
- **目标**: 补齐终端右侧设置面板中未翻译的 `Search`、`Font Size`、`Font Family` 等文案，并清理相关说明提示。
- **范围**: `crates/terminal_view/src/sidebar/settings_panel.rs`、`crates/terminal_view/locales/terminal_view.yml`、`.claude` 留痕。
- **交付物**: 文案修复、词条补齐、本地扫描验证、编译验证、上下文摘要、操作日志、审查报告。
- **审查要点**: 设置面板内不再直接渲染英文可见文案；提示文案与当前交互一致；不影响设置事件流。

### 技术维度评分
- **代码质量**: 94/100
  - 修复集中在文案层，未改动设置面板的状态和事件处理。
  - 使用 `t!()` 与现有 `Settings.*` 键体系，符合仓库既有模式。
- **测试覆盖**: 87/100
  - 已做定向硬编码扫描和 `cargo check -p terminal_view`。
  - 尚未对多语言 GUI 显示做自动化或手工回归。
- **规范遵循**: 95/100
  - 新增词条与留痕文件均使用简体中文。
  - 当前环境缺少仓库要求的专用工具，已在日志中记录替代方式。

### 战略维度评分
- **需求匹配**: 96/100
  - 直接覆盖用户点名的 `Search`、`Font Size`、`Font Family`，并顺手清理设置面板内其他同类英文。
- **架构一致**: 95/100
  - 与终端侧边栏其他面板一样，统一走 `terminal_view.yml + t!()` 的本地化架构。
- **风险评估**: 89/100
  - 主要剩余风险是未验证不同语言在 GUI 中的排版与截断情况。

### 综合评分
- **综合评分**: 93/100
- **建议**: 通过

### 本地验证记录
- `rg -n '"(Settings|SEARCH|FONT SIZE|FONT FAMILY|THEME|Search\\.\\.\\.|Select font\\.\\.\\.|Press [^"]+)"' crates/terminal_view/src/sidebar/settings_panel.rs`：未命中
- `cargo fmt --all -- /usr/htdocs/onetcli/crates/terminal_view/src/sidebar/settings_panel.rs`：通过
- `cargo check -p terminal_view`：通过

### 结论
- 设置面板标题、搜索区、字体区、主题区、搜索占位符和字体选择占位符都已切到 i18n 词条。
- 搜索提示已从硬编码英文改为本地化文案，并改成与当前交互一致的“回车/Shift+回车”提示。
- 当前结论建立在静态扫描与编译验证之上，尚未覆盖 GUI 语言切换的最终显示效果。
