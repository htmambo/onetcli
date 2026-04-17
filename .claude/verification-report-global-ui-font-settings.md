## 审查报告 - 全局 UI 字体设置接通
时间：2026-03-26 20:39:21 +0800

### 需求核对
- **目标**: 把首页设置里那组“字体 / 字号”真正接通到全局 UI，而不是只存配置不生效。
- **范围**: `main/src/setting_tab.rs`，以及与之关联的全局 Theme 应用路径。
- **交付物**: 代码修复、本地格式化与编译验证、`.claude` 留痕。
- **审查要点**: 只影响普通 UI，不影响终端专属字体设置；主题切换后仍保留用户自定义字体和字号。

### 技术维度评分
- **代码质量**: 95/100
  - 用单一辅助函数统一写回 `Theme::global_mut(cx)` 并刷新窗口，避免重复逻辑。
  - 启动加载、设置即时修改、明暗主题切换三条路径都已接通。
- **测试覆盖**: 86/100
  - 已完成格式化和 `cargo check -p main` 编译验证。
  - 尚未做 GUI 手工验证。
- **规范遵循**: 95/100
  - 改动集中、命名清晰，没有新增第二套设置体系。

### 战略维度评分
- **需求匹配**: 98/100
  - 直接解决“这组设置是做什么的、为什么看不出效果”的问题。
- **架构一致**: 96/100
  - 普通 UI 继续走全局 Theme，终端继续走 `terminal_*` 专属配置。
- **风险评估**: 90/100
  - 剩余风险主要在于 GUI 体验确认，而非代码链路缺失。

### 综合评分
- **综合评分**: 95/100
- **建议**: 通过

### 本地验证记录
- `cargo fmt --all`：通过
- `cargo check -p main`：通过

### 结论
- [`setting_tab.rs`](/usr/htdocs/onetcli/main/src/setting_tab.rs#L393) 到 [`setting_tab.rs`](/usr/htdocs/onetcli/main/src/setting_tab.rs#L404) 新增 `apply_ui_font_preferences(...)`，统一把设置写回 `Theme` 并刷新所有窗口。
- [`setting_tab.rs`](/usr/htdocs/onetcli/main/src/setting_tab.rs#L406) 到 [`setting_tab.rs`](/usr/htdocs/onetcli/main/src/setting_tab.rs#L416) 现在会在启动加载设置时应用全局 UI 字体和字号。
- [`setting_tab.rs`](/usr/htdocs/onetcli/main/src/setting_tab.rs#L601) 到 [`setting_tab.rs`](/usr/htdocs/onetcli/main/src/setting_tab.rs#L617) 会在切换深浅主题后重新覆盖用户自定义字体和字号，避免被主题默认值冲掉。
- [`setting_tab.rs`](/usr/htdocs/onetcli/main/src/setting_tab.rs#L688) 到 [`setting_tab.rs`](/usr/htdocs/onetcli/main/src/setting_tab.rs#L699) 与 [`setting_tab.rs`](/usr/htdocs/onetcli/main/src/setting_tab.rs#L708) 到 [`setting_tab.rs`](/usr/htdocs/onetcli/main/src/setting_tab.rs#L721) 会在设置页即时修改普通 UI 的字体和字号。
- [`root.rs`](/usr/htdocs/onetcli/crates/ui/src/root.rs#L449) 和 [`root.rs`](/usr/htdocs/onetcli/crates/ui/src/root.rs#L459) 已经消费全局 Theme 的 `font_size` / `font_family`，所以这次接通后会影响首页、设置页、弹窗等普通界面。

### 残余风险
- 尚未手工确认不同页面在更换字体后是否存在局部排版挤压。
- 终端和其他显式使用等宽字体的区域不会跟随这组设置变化，这是预期行为。
