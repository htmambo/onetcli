## 项目上下文摘要（deepin-window-control-corner）
生成时间：2026-03-28 02:00:23 +0800

### 1. 相似实现分析
- **实现1**: [crates/core/src/tab_container.rs](/usr/htdocs/onetcli/crates/core/src/tab_container.rs)
  - 模式：主窗口标签栏在右上角自绘窗口按钮
  - 可复用：`render_window_controls(...)` 与 `render_control_button(...)`
  - 需注意：最右侧关闭按钮当前是标准矩形容器，容易与外层圆角壳层发生视觉冲突

- **实现2**: [crates/ui/src/title_bar.rs](/usr/htdocs/onetcli/crates/ui/src/title_bar.rs)
  - 模式：弹窗和普通标题栏统一复用 `WindowControls`
  - 可复用：`linux_prefers_system_window_controls()` 与窗口按钮渲染入口
  - 需注意：这里也会受 Deepin 右上角圆角问题影响，不能只修主窗口

- **实现3**: [crates/ui/src/window_border.rs](/usr/htdocs/onetcli/crates/ui/src/window_border.rs)
  - 模式：窗口根层负责边框、阴影、圆角和内容裁剪
  - 可复用：`window.set_client_inset(...)` 与系统装饰路径判断
  - 需注意：当前圆角属于内容层表现，不等于 X11 物理 shape

- **实现4**: [vendor/zed/crates/gpui/src/platform/linux/x11/window.rs](/usr/htdocs/onetcli/vendor/zed/crates/gpui/src/platform/linux/x11/window.rs)
  - 模式：平台层为 Deepin 写 `_DEEPIN_NO_TITLEBAR` / `_DEEPIN_FORCE_DECORATE`
  - 可复用：已有 Deepin 专有原子写入逻辑
  - 需注意：本次现场检查已确认这些属性值正确，问题不在属性缺失

### 2. 现场证据
- 主窗口 `0x8000002` 属性：
  - `_DEEPIN_NO_TITLEBAR(CARDINAL) = 1`
  - `_DEEPIN_FORCE_DECORATE(CARDINAL) = 0`
  - `_MOTIF_WM_HINTS(_MOTIF_WM_HINTS) = 0x2, 0x0, 0x0, 0x0, 0x0`
- 主窗口树结构：
  - `root -> 0x20f45fc -> 0x20f45fd -> 0x8000002`
  - 说明 Deepin 仍在应用窗口外层包裹了无名父窗口
- `xwininfo -shape` 结果：
  - `0x8000002` 与 `0x20f45fd` 都是 `No window shape defined`
  - 说明右上角圆角更像是外层壳层/视觉层效果，不是应用窗口真实 shape 裁剪

### 3. 项目约定
- **命名约定**：布尔判断使用 `should_*` / `is_*`；Linux 兼容判断已有 `linux_prefers_system_window_controls`
- **文件组织**：主窗口标签栏在 `crates/core`，通用标题栏在 `crates/ui`，平台 X11 逻辑在 `vendor/zed`
- **代码风格**：优先最小改动，沿用链式 UI 构造，通过局部布尔值控制 `.when(...)`

### 4. 可复用组件清单
- [crates/ui/src/title_bar.rs](/usr/htdocs/onetcli/crates/ui/src/title_bar.rs): `linux_prefers_system_window_controls`
- [crates/core/src/tab_container.rs](/usr/htdocs/onetcli/crates/core/src/tab_container.rs): 主窗口右上角按钮实现
- [crates/ui/src/window_border.rs](/usr/htdocs/onetcli/crates/ui/src/window_border.rs): 圆角与内容裁剪路径

### 5. 测试策略
- **测试框架**：`cargo test` / `cargo check`
- **测试模式**：标题栏单元测试 + 相关 crate 编译检查 + Deepin 实机回归
- **参考文件**：`crates/ui/src/title_bar.rs` 中已有桌面环境识别测试
- **覆盖要求**：不破坏非 Deepin 平台；Deepin 下主窗口与通用标题栏都需要验证右上角按钮位置

### 6. 技术选型理由
- **为什么用“关闭按钮自身圆角裁剪”**：
  - 既然 Deepin 外层壳层与应用内容不共用 shape/mask，整组按钮右移会破坏贴边布局
  - 只让最右侧关闭按钮自身带圆角裁剪，可以在不改变按钮组对齐关系的前提下避开切角区域
- **优势**：
  - 改动集中在关闭按钮包装层，不动平台层和窗口创建流程
  - 同时覆盖主窗口标签栏和通用标题栏
- **风险**：
  - 这是 Deepin 兼容绘制修正，不是消除外层壳层的根治方案
  - 需要用户实机确认当前主题下关闭按钮圆角半径是否足够

### 7. 关键风险点
- **环境差异**：不同 Deepin/KWin 主题的圆角半径可能不同
- **边界条件**：最大化或右侧贴边时不应继续保留额外圆角裁剪，因此必须受 `tiling.top/right` 约束
- **回归面**：弹窗标题栏和主窗口标签栏都要同步处理，避免行为不一致
