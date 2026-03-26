## 审查报告 - 应用窗口边框预览
时间：2026-03-26 20:26:43 +0800

### 需求核对
- **目标**: 先给当前应用窗口补一个可见边框，便于观察实际视觉效果。
- **范围**: `crates/ui/src/window_border.rs` 公共窗口层，影响主窗口和弹窗。
- **交付物**: 可见边框预览、本地格式化与编译验证、`.claude` 留痕。
- **审查要点**: 不重写系统窗口装饰，不破坏 Linux 既有客户端边框与拖拽缩放逻辑。

### 技术维度评分
- **代码质量**: 94/100
  - 改动集中在公共窗口装饰层，范围小。
  - 通过条件渲染区分“系统装饰路径”和“自绘客户端边框路径”，避免明显双框。
- **测试覆盖**: 84/100
  - 已完成格式化与编译验证。
  - 当前没有 GUI 自动化，也没有截图级视觉回归。
- **规范遵循**: 95/100
  - 文档与日志均使用简体中文。
  - 没有新增无关 API、设置项或页面级重复逻辑。

### 战略维度评分
- **需求匹配**: 96/100
  - 直接满足“先加一下我看看”的预览需求。
- **架构一致**: 96/100
  - 继续复用 `WindowBorder` 与 `Root` 统一窗口基础设施。
- **风险评估**: 88/100
  - 主要风险在于视觉强度是否符合预期，需要 GUI 实看。

### 综合评分
- **综合评分**: 93/100
- **建议**: 通过

### 本地验证记录
- `cargo fmt --all`：通过
- `cargo check -p gpui-component -p main`：通过

### 结论
- [`window_border.rs`](/usr/htdocs/onetcli/crates/ui/src/window_border.rs#L107) 新增 `show_content_border`，用于识别系统装饰路径。
- [`window_border.rs`](/usr/htdocs/onetcli/crates/ui/src/window_border.rs#L227) 到 [`window_border.rs`](/usr/htdocs/onetcli/crates/ui/src/window_border.rs#L230) 现在会在系统装饰路径下补一圈 1px 内边框。
- [`root.rs`](/usr/htdocs/onetcli/crates/ui/src/root.rs#L451) 已经统一使用 `window_border()`，所以主窗口和弹窗都会自动继承这次预览改动。
- Linux 原有客户端外框、阴影和缩放热区逻辑仍保留，见 [`window_border.rs`](/usr/htdocs/onetcli/crates/ui/src/window_border.rs#L198) 到 [`window_border.rs`](/usr/htdocs/onetcli/crates/ui/src/window_border.rs#L224)。

### 残余风险
- 还没做 GUI 手工确认，无法保证这圈边框在你的主题和桌面环境下强度刚好合适。
- 如果你想要的是“更像系统外框”的效果，这个预览版还不够，需要继续调色、宽度，甚至区分平台策略。
