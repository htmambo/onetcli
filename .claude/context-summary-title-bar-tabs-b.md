## 项目上下文摘要（title-bar-tabs-b）
生成时间：2026-03-24 23:59:30 +0800

### 1. 相似实现分析
- **实现1**: `main/src/onetcli_app.rs:399-427`
  - 模式：主窗口当前只渲染 `TabContainer`，系统窗口标题同步放在 `render` 中完成
  - 可复用：主窗口 `render` 是插入 `TitleBar` 的唯一稳定入口
  - 需注意：不能破坏现有 `Root` 弹层、通知层和窗口标题同步逻辑

- **实现2**: `crates/story/src/lib.rs:649-673`
  - 模式：窗口主体采用“`TitleBar` + 内容区”的纵向布局
  - 可复用：`v_flex().child(title_bar).child(content)` 的主窗口组织方式
  - 需注意：标题栏和内容区必须拆分为两个独立渲染层，而不是把整页实体重复挂载两次

- **实现3**: `crates/story/src/title_bar.rs:53-86`
  - 模式：`TitleBar::new()` 内部承载可交互子元素，右侧控件由组件自行管理
  - 可复用：标题栏中放复杂交互内容是既有模式
  - 需注意：交互区域需要阻断标题栏拖动事件，否则点击内部控件时会误触发窗口拖动

- **实现4**: `crates/core/src/tab_container.rs:1491-2074`
  - 模式：`TabContainer` 当前同时负责标签条和内容区渲染
  - 可复用：既有标签状态机、拖拽、关闭、固定首页标签、活动标签切换逻辑
  - 需注意：B 方案应只拆渲染层，不重写标签状态或持久化协议

- **实现5**: `main/src/main.rs:46-66` 与 `crates/core/src/popup_window.rs:95-111`
  - 模式：弹窗已统一启用 `TitleBar::title_bar_options()`，主窗口此前在 Linux 下未启用
  - 可复用：窗口选项层的标题栏能力
  - 需注意：Linux/Deepin 下仍要兼容系统控件保留的情况，不能重新引入双按钮

### 2. 项目约定
- **命名约定**: UI 布尔配置使用 `with_*` builder 命名，渲染拆分使用 `render_*`
- **文件组织**: 容器级渲染逻辑保留在 `crates/core/src/tab_container.rs`，主窗口组装保留在 `main/src/onetcli_app.rs`
- **导入顺序**: 标准库 / `gpui` / 组件库 / 项目模块
- **代码风格**: 通过 builder 链式拼装 UI，局部模式拆分成小方法，避免引入额外全局状态

### 3. 可复用组件清单
- `gpui_component::TitleBar`
- `gpui_component::TitleBar::title_bar_options`
- `crates/core/src/tab_container.rs::render_tab_content`
- `crates/core/src/tab_container.rs::render_window_controls`
- `crates/ui/src/title_bar.rs::should_render_custom_window_controls`

### 4. 测试策略
- **测试框架**: Rust 内联单元测试 + `cargo check`/`cargo test --no-run`
- **测试模式**: 主工程编译、主窗口标题测试、标题栏按钮兼容测试、核心包测试目标编译
- **参考文件**: `main/src/onetcli_app.rs`、`crates/ui/src/title_bar.rs`
- **覆盖要求**: 结构改造后主工程可编译、原有窗口标题测试保持通过、Deepin 按钮兼容测试保持通过

### 5. 依赖和集成点
- **外部依赖**: `gpui::WindowOptions`、`gpui_component::TitleBar`
- **内部依赖**: `OnetCliApp -> TabContainer -> TabContent`
- **集成方式**: 主窗口创建时启用标题栏选项，运行时用 `TitleBar` 承载标签条，`TabContainer` 只渲染内容区
- **配置来源**: 无新增配置，沿用现有窗口选项和 `TabContainer` builder

### 6. 技术选型理由
- **为什么用这个方案**: 在不重写标签模型的前提下，这是最接近“把标签移到标题栏”的可落地方案
- **优势**: 状态逻辑不动、回退简单、与现有 `TitleBar` 生态一致
- **劣势和风险**: 视觉成败仍取决于 Linux/Deepin 对标题栏区域的实际呈现，必须依赖实机观察

### 7. 关键风险点
- **拖动冲突**: 标题栏承载标签后，点击标签或关闭按钮不能误触发窗口拖动
- **双控件回归**: 主窗口切到 `TitleBar` 后不能重新出现应用自绘和系统控件并存
- **布局收缩**: 标题栏可用宽度比独立标签栏更紧，长标签在窗口窄时更容易压缩
- **平台差异**: Linux 主窗口启用 `titlebar` 选项后，Deepin/X11 的真实呈现效果需要实机确认
