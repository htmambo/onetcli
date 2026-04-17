## 项目上下文摘要（tab-bar-windows-visible-width）
生成时间：2026-03-29 05:55:00 +08:00

### 0. 检索说明
- 当前会话未提供 `desktop-commander`、`context7`、`github.search_code`、`sequential-thinking` 工具。
- 本次改用仓库内 `rg`、源码阅读和既有 `.claude` 留痕做等效上下文分析。
- 用户补充“目测大概是窗口控制的那三个按钮的总宽度”后，问题范围从“tab 宽度算法”收敛到“Windows 右侧固定功能区的布局占位”。

### 1. 相似实现分析
- **实现1**: `D:\usr\htdocs\onetcli\crates\core\src\tab_container.rs:1850-2276`
  - 模式：主窗口 tab bar 由滚动区、Windows 拖拽热区、下拉按钮和窗口控件共同组成。
  - 可复用：只调整右侧子元素顺序即可修正可视宽度，不必改 tab 自身宽度和滚动逻辑。
  - 需注意：Windows 仍要保留独立拖拽热区，不能回退到整块 `#tabs` 容器抢占 `WindowControlArea::Drag`。

- **实现2**: `D:\usr\htdocs\onetcli\crates\ui\src\title_bar.rs:308-388`
  - 模式：通用标题栏把拖拽区、窗口控件和标题内容拆开布局。
  - 可复用：功能区应尽量贴近窗口控件，避免在主要内容区中间插入大块固定空白。
  - 需注意：Windows 依赖显式命中区，不能仅凭视觉空白推断可拖动。

- **实现3**: `D:\usr\htdocs\onetcli\.claude\operations-log.md:3311-3376`
  - 模式：前两轮 Windows 拖拽修复引入了 `tab-bar-inline-drag-spacer` 和 `tab-bar-drag-spacer` 两段热区。
  - 可复用：保留“滚动区内联热区 + 右侧兜底热区”的总体思路。
  - 需注意：此前右侧兜底热区放在“滚动区后、下拉前”，会直接压缩 tab 区域的可视宽度。

### 2. 项目约定
- **命名约定**：Rust 变量和函数使用 `snake_case`，类型使用 `PascalCase`。
- **文件组织**：主窗口标签条和窗口控件布局集中在 `crates/core/src/tab_container.rs`。
- **代码风格**：继续使用 GPUI builder 链和 `.when(...)` 条件分支，小改动优先通过调整子元素顺序解决。

### 3. 可复用组件清单
- `D:\usr\htdocs\onetcli\crates\core\src\tab_container.rs`：`WINDOWS_TAB_BAR_DRAG_SPACER_WIDTH`
- `D:\usr\htdocs\onetcli\crates\core\src\tab_container.rs`：`tab-bar-inline-drag-spacer`
- `D:\usr\htdocs\onetcli\crates\core\src\tab_container.rs`：`tab-bar-drag-spacer`
- `D:\usr\htdocs\onetcli\crates\core\src\tab_container.rs`：`render_window_controls(...)`

### 4. 测试策略
- **测试框架**：Rust 单元测试 + 编译检查。
- **本次策略**：
  - 运行 `cargo test -p one-core tab_container::tests --lib -- --nocapture`，确认 Windows/非 Windows 平台判定逻辑未回归。
  - 运行 `cargo check -p main` 做集成编译检查。
- **覆盖重点**：Windows 独立拖拽热区仍保留；tab bar 布局调整后核心 crate 继续可编译。

### 5. 依赖和集成点
- **外部依赖**：GPUI 的 `WindowControlArea`
- **内部依赖**：`TabContainer::render_tab_bar(...)`、`render_window_controls(...)`
- **集成方式**：通过 `OnetCliApp::new(...)` 的 `.with_window_controls(true)` 启用 Windows 主窗口控件区

### 6. 技术选型理由
- **为什么不改 tab 宽度计算**：用户描述的偏差量更像右侧固定区域的布局占位，而不是 tab 项本身的 `w(tab_width)` 算法问题。
- **为什么改子元素顺序**：右侧固定拖拽热区本身仍有价值，但放在下拉按钮前会直接减少 tab 区域的可视宽度；移到下拉按钮后，能在不删热区的前提下回收这段宽度。
- **为什么不回退拖拽修复**：此前已经验证过 Windows 需要独立拖拽热区，不能为了宽度把拖拽能力重新做坏。

### 7. 关键风险点
- **交互平衡**：拖拽热区虽然还在，但位置更靠右，仍需 Windows 实机确认拖窗手感是否足够。
- **验证环境**：`cargo check -p main` 当前会被 `aws-lc-sys` 的环境依赖和权限问题阻塞，不是这次 tab bar 布局改动本身的编译错误。
