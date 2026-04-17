## 项目上下文摘要（windows-mouse-drag）
生成时间：2026-03-28 20:30:23 +08:00

### 1. 相似实现分析
- **实现1**: `D:\zhp\src\onetcli\crates\core\src\tab_container.rs:1541-1969`
  - 模式：主窗口 tab bar 同时承担窗口拖动、tab 交互、窗口控件渲染三类职责。
  - 可复用：`TabBarDragState`、`render_window_controls(...)`、`WindowControlArea::Drag`。
  - 需注意：`#tabs` 是 `overflow_x_scroll()` 容器，若整体声明为 `Drag`，会和 tab 自身的鼠标拖拽排序竞争命中。

- **实现2**: `D:\zhp\src\onetcli\crates\ui\src\title_bar.rs:308-388`
  - 模式：标题栏内部使用稳定容器 `#bar` 声明 `window_control_area(WindowControlArea::Drag)`，同时保留 `TitleBarState.should_move` 手动拖窗状态机。
  - 可复用：拖窗热区与交互子元素分离的布局方式；`show_custom_window_controls` 条件渲染。
  - 需注意：交互控件区域必须独立，不能让整个可交互容器都落入系统拖窗命中测试。

- **实现3**: `D:\zhp\src\onetcli\main\src\main.rs:66-82` 与 `D:\zhp\src\onetcli\main\src\onetcli_app.rs:336-359`
  - 模式：Windows 主窗口启用 `TitleBar::title_bar_options()`，并在主 `TabContainer` 上显式开启 `.with_window_controls(true)`。
  - 可复用：问题修复应集中在 `TabContainer`，不应改动应用入口窗口配置。
  - 需注意：主窗口与弹窗共用标题栏能力，修复需要避免影响 macOS/Linux 和弹窗窗口。

- **实现4**: `D:\zhp\src\onetcli\crates\core\src\popup_window.rs:163-175`
  - 模式：弹窗窗口也沿用 `TitleBar::title_bar_options()`。
  - 可复用：说明仓库已有统一标题栏方案，主窗口 tab bar 需要与该方案保持行为一致。
  - 需注意：不能把主窗口问题扩散成全局标题栏回归。

- **实现5（依赖行为）**: `D:\zhp\src\onetcli\vendor\zed\crates\gpui\src\window.rs:1308-1318` 与 `D:\zhp\src\onetcli\vendor\zed\crates\gpui\src\window.rs:1968-1973`
  - 模式：`window_control_hitboxes` 命中后会直接返回对应 `WindowControlArea`；`start_window_move()` 注释明确标注主要处理 Linux 和 macOS。
  - 可复用：Windows 更依赖稳定的 `WindowControlArea` 命中，而不是把拖窗完全压在 `start_window_move()` 事件链上。
  - 需注意：命中优先级受插入顺序影响，把整个 tab 滚动容器标成 `Drag` 很容易吞掉 tab 拖动排序。

### 2. 项目约定
- **命名约定**：Rust 函数/变量使用 `snake_case`，类型使用 `PascalCase`。
- **文件组织**：主窗口 tab 交互集中在 `crates/core/src/tab_container.rs`；通用标题栏能力集中在 `crates/ui/src/title_bar.rs`。
- **代码风格**：GPUI builder 链式组合，大量使用 `.when(...)`、`.when_some(...)` 条件拼装 UI。
- **平台分支**：macOS、Linux、Windows 通过 `cfg!(target_os = "...")` 与条件渲染分流。

### 3. 可复用组件清单
- `D:\zhp\src\onetcli\crates\core\src\tab_container.rs`：`TabBarDragState`
- `D:\zhp\src\onetcli\crates\core\src\tab_container.rs`：`render_window_controls(...)`
- `D:\zhp\src\onetcli\crates\ui\src\title_bar.rs`：`TitleBarState`
- `D:\zhp\src\onetcli\vendor\zed\crates\gpui\src\window.rs`：`WindowControlArea` 命中机制

### 4. 测试策略
- **测试框架**：Rust 单元测试 + `cargo check` 编译检查。
- **参考文件**：`D:\zhp\src\onetcli\crates\ui\src\title_bar.rs:393-417`
- **本次测试重点**：
  - Windows 只保留独立拖窗热区，不再让 `#tabs` 卷入系统拖窗命中。
  - 非 Windows 平台继续保留既有手动拖窗链路。
  - tab 点击、关闭、拖动排序交互不被 `WindowControlArea::Drag` 抢占。

### 5. 依赖和集成点
- **外部依赖**：`gpui` 的 `WindowControlArea`、`start_window_move()`、窗口 hit-test。
- **内部依赖**：`OnetCliApp::new(...)` 对 `TabContainer::with_window_controls(true)` 的启用。
- **集成方式**：修复点局限在 `TabContainer::render_tab_bar(...)` 的 Windows 分支。
- **配置来源**：主窗口入口 `main/src/main.rs` 的 `WindowOptions`。

### 6. 技术选型理由
- **为什么用这个方案**：最小化改动现有结构，只在 Windows 上把拖窗热区从 tab 滚动容器拆出来，避免影响 macOS/Linux 的既有拖窗逻辑。
- **优势**：不改入口配置，不改 tab 排序实现，不改通用标题栏组件；回归面小。
- **劣势和风险**：Windows 可拖窗区域会从“整个 tab 滚动区”缩成“独立热区”，需要手工确认热区宽度体验是否足够。

### 7. 关键风险点
- **边界条件**：tab 很多导致滚动时，独立拖窗热区仍需稳定存在。
- **交互优先级**：tab 自身拖动排序、点击激活、关闭按钮不能再被系统 hit-test 抢占。
- **平台差异**：macOS/Linux 仍依赖手动拖窗事件链，不能被 Windows 分支误伤。
- **验证缺口**：当前只能做本地编译与单测，最终 GUI 拖窗体验仍需桌面手工回归。
