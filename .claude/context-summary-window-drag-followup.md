## 项目上下文摘要（window-drag-followup）
生成时间：2026-03-28 23:59:00 +08:00

### 1. 相似实现分析
- **实现1**: `D:\zhp\src\onetcli\crates\ui\src\title_bar.rs:308-388`
  - 模式：通用标题栏把主体容器 `#bar` 声明为 `WindowControlArea::Drag`，并保留 `TitleBarState.should_move` 供非 Windows 平台手动拖窗。
  - 可复用：拖窗热区与交互控件分离；双击标题栏走窗口缩放逻辑。
  - 需注意：Windows 依赖命中区，不能只靠 `start_window_move()`。

- **实现2**: `D:\zhp\src\onetcli\crates\core\src\tab_container.rs:1541-1915`
  - 模式：主窗口使用 `TabContainer` 取代普通标题栏，同时承载 tab 交互、窗口控件和拖窗热区。
  - 可复用：`uses_manual_window_move(...)`、`should_render_windows_drag_spacer(...)`、`tab-bar-inline-drag-spacer`、`tab-bar-drag-spacer`。
  - 需注意：若把整个 `#tabs` 容器标成 `Drag`，会抢走 tab 点击与拖拽排序事件。

- **实现3**: `D:\zhp\src\onetcli\main\src\onetcli_app.rs:419-503`
  - 模式：窗口 bounds 变化只写本地缓存 `pending_window_bounds`，通过 `Debouncer` 延迟写回全局设置并落盘。
  - 可复用：`stage_window_bounds(...)`、`flush_pending_window_bounds(...)`、`schedule_window_bounds_save(...)`。
  - 需注意：拖动窗口时不能在高频回调中调用 `AppSettings::global_mut(...)`，否则全局观察者通知会导致拖动卡顿。

- **实现4**: `D:\zhp\src\onetcli\main\src\main.rs:46-83` 与 `D:\zhp\src\onetcli\main\src\setting_tab.rs:588-620`
  - 模式：应用入口从 `AppSettings` 恢复主窗口位置与显示状态；设置模块负责 bounds 快照与延迟保存入口。
  - 可复用：`restored_main_window_bounds(...)`、`snapshot_main_window_bounds(...)`、`save_global(...)`。
  - 需注意：窗口恢复与窗口拖动属于同一条数据链，不能只修热区不修状态保存性能。

### 2. 项目约定
- **命名约定**：Rust 函数与变量使用 `snake_case`，类型使用 `PascalCase`。
- **文件组织**：窗口拖动热区放在 `crates/core/src/tab_container.rs`；窗口状态保存放在 `main/src/onetcli_app.rs` 与 `main/src/setting_tab.rs`。
- **代码风格**：GPUI builder 链 + `.when(...)` 平台分支；行为判断优先提取为小函数。
- **平台策略**：Windows 走 `WindowControlArea` 命中；Linux/macOS 保留手动 `start_window_move()` 链路。

### 3. 可复用组件清单
- `D:\zhp\src\onetcli\crates\core\src\tab_container.rs`：`uses_manual_window_move(...)`
- `D:\zhp\src\onetcli\crates\core\src\tab_container.rs`：`should_render_windows_drag_spacer(...)`
- `D:\zhp\src\onetcli\main\src\onetcli_app.rs`：`pending_window_bounds`
- `D:\zhp\src\onetcli\main\src\onetcli_app.rs`：`window_bounds_save_debouncer`
- `D:\zhp\src\onetcli\main\src\setting_tab.rs`：`SavedWindowBounds`

### 4. 测试策略
- **测试框架**：Rust 单元测试 + `cargo check` 编译验证。
- **参考文件**：`D:\zhp\src\onetcli\crates\core\src\tab_container.rs:2130-2146`
- **本次实际执行**：
  - `C:\Users\hoping\.cargo\bin\cargo.exe test -p one-core tab_container::tests --lib -- --nocapture`
  - `C:\Users\hoping\.cargo\bin\cargo.exe check -p main`
- **覆盖重点**：
  - Windows 仅渲染独立拖窗热区
  - 非 Windows 继续保留手动拖窗链路
  - 主窗口编译链是否还能通过

### 5. 依赖和集成点
- **外部依赖**：`gpui` 的 `WindowControlArea` 命中机制与窗口 bounds 观察接口。
- **内部依赖**：`OnetCliApp::new(...)` 的主窗口生命周期；`AppSettings` 的全局设置持久化。
- **集成方式**：主窗口拖动由 `TabContainer` 热区负责，主窗口状态保存由 `OnetCliApp` 防抖写回负责。
- **配置来源**：`main/src/main.rs` 的 `WindowOptions` 与 `main/src/setting_tab.rs` 的 `main_window_bounds`。

### 6. 技术选型理由
- **为什么用当前方案**：用户表面的“不能拖动”同时可能来自命中区缺失和拖动时高频状态更新卡顿，必须两条链路同时闭环。
- **优势**：修复点局部、符合现有架构，不需要重写标题栏或平台层。
- **劣势和风险**：GUI 体感仍需 Windows 实机确认；当前环境缺少 `cmake` 和 `nasm`，无法完成 `main` 全量编译。

### 7. 关键风险点
- **交互冲突**：tab 拖拽排序与窗口拖拽命中区仍然存在天然竞争，Windows 只能使用独立热区。
- **性能回归**：任何重新把窗口移动回调接回 `global_mut(...)` 或同步写盘的改动，都会再次造成拖动卡顿。
- **验证缺口**：当前只能验证 `one-core` 单测和 `main` 的构建阻塞原因，无法在本环境里直接做 GUI 实机拖动回归。
