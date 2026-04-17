## 审查报告 - 终端行间距设置
时间：2026-03-26 20:16:19 +0800

### 需求核对
- **目标**: 把终端现有但未暴露的行间距能力补成真实可配置设置，并保证首页设置、终端右侧设置和终端实例之间保持同步。
- **范围**: `main/src/setting_tab.rs`、`main/src/home/home_tabs.rs`、`crates/terminal_view/src/sidebar/*`、`crates/terminal_view/src/view.rs`、终端主题常量导出与相关本地化词条。
- **交付物**: 行间距设置项、事件同步链路、主题切换排版保持、本地验证、`.claude` 留痕文件。
- **审查要点**: 行间距不能再是隐藏代码常量；切换主题不能覆盖字号或行间距；不引入新的配置状态源。

### 技术维度评分
- **代码质量**: 95/100
  - 直接复用现有 `AppSettings -> HomePage -> TerminalView -> Sidebar` 链路，没有新造状态层。
  - 行间距范围统一收敛到 `theme.rs` 的常量和 `with_line_height_scale(...)` / `set_line_height_scale(...)` 中，避免不同入口各写一套边界。
- **测试覆盖**: 89/100
  - 已补做 `preserve_theme_typography_keeps_current_font_configuration` 定向测试与 `cargo check -p terminal_view -p main` 编译验证。
  - 仍缺少 GUI 级联动回归，例如首页设置、右侧设置、切换主题后的真实渲染检查。
- **规范遵循**: 95/100
  - 文档、日志与本地化词条均使用简体中文。
  - 当前环境缺少仓库要求的专用工具，已明确使用本地代码证据和本地编译/测试替代。

### 战略维度评分
- **需求匹配**: 98/100
  - 直接覆盖用户指出的“代码里有行间距，但设置里找不到”问题，并补齐两个入口。
- **架构一致**: 96/100
  - 行间距继续作为 `TerminalTheme` 的排版参数存在，只是接入已有设置体系，没有破坏当前主题架构。
- **风险评估**: 90/100
  - 主要剩余风险集中在 GUI 体验验证不足，而非代码链路缺失。

### 综合评分
- **综合评分**: 95/100
- **建议**: 通过

### 本地验证记录
- `cargo test -p terminal_view preserve_theme_typography_keeps_current_font_configuration -- --nocapture`：通过
- `cargo check -p terminal_view -p main`：通过

### 结论
- [`setting_tab.rs`](/usr/htdocs/onetcli/main/src/setting_tab.rs#L717) 已新增首页“终端行间距”设置项，并在值变化后立即同步到所有已打开终端。
- [`home_tabs.rs`](/usr/htdocs/onetcli/main/src/home/home_tabs.rs#L44) 与 [`home_tabs.rs`](/usr/htdocs/onetcli/main/src/home/home_tabs.rs#L90) 已把行间距纳入“创建时下发 + 事件回写 + 全量广播”的闭环。
- [`settings_panel.rs`](/usr/htdocs/onetcli/crates/terminal_view/src/sidebar/settings_panel.rs#L184) 与 [`settings_panel.rs`](/usr/htdocs/onetcli/crates/terminal_view/src/sidebar/settings_panel.rs#L560) 已在终端右侧设置面板接入“行间距”输入框，并支持手动输入与步进调节。
- [`view.rs`](/usr/htdocs/onetcli/crates/terminal_view/src/view.rs#L837) 与 [`view.rs`](/usr/htdocs/onetcli/crates/terminal_view/src/view.rs#L1003) 已确保终端实例实际应用并广播行间距变化。
- [`view.rs`](/usr/htdocs/onetcli/crates/terminal_view/src/view.rs#L96) 的 `preserve_theme_typography(...)` 仍然生效，因此切换主题时不会覆盖当前字号、字体和行间距。
- [`theme.rs`](/usr/htdocs/onetcli/crates/terminal_view/src/theme.rs#L25) 与 [`terminal_view.yml`](/usr/htdocs/onetcli/crates/terminal_view/locales/terminal_view.yml#L334) / [`main.yml`](/usr/htdocs/onetcli/main/locales/main.yml#L771) 已把范围常量和文案一起补齐。

### 残余风险
- 尚未在 GUI 中手工跑完整路径：首页设置行间距、打开终端、右侧切换主题、再次增减字号/行间距。
- 编译过程中出现的 `gpui-component` 与 `ssh` 既有 warning 与本次修复无关，但仍值得后续单独清理。
