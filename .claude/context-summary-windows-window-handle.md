## 项目上下文摘要（windows-window-handle）
生成时间：2026-03-28 21:50:10 +08:00

### 1. 相似实现分析
- **实现1**: `D:\zhp\src\onetcli\vendor\zed\crates\gpui\src\window.rs:1170-1372`
  - 模式：平台窗口在初始化时批量注册 `on_request_frame`、`on_resize`、`on_moved`、`on_input`、`on_hit_test_window_control` 等回调。
  - 可复用：所有平台事件最终都通过 `handle.update(...)` 回到 GPUI 窗口实体。
  - 需注意：窗口一旦在 `on_close` 中执行 `window.remove_window()`，这些晚到回调再继续 `handle.update(...).log_err()` 就会打出 `window not found`。

- **实现2**: `D:\zhp\src\onetcli\vendor\zed\crates\gpui\src\app.rs:1489-1530` 与 `D:\zhp\src\onetcli\vendor\zed\crates\gpui\src\app.rs:2371-2374`
  - 模式：`update_window_id` / `read_window` 在窗口实体不存在时统一返回 `context("window not found")`。
  - 可复用：说明“窗口已移除”在 GPUI 内部本来就是一个显式错误边界。
  - 需注意：这类错误不适合在全局语义层吞掉，更适合在平台销毁阶段收口晚到事件。

- **实现3**: `D:\zhp\src\onetcli\vendor\zed\crates\gpui\src\platform\windows\events.rs:265-295`
  - 模式：Windows `WM_DESTROY` 中先取出 `close` 回调，再执行上层关窗逻辑并回发 `WM_GPUI_CLOSE_ONE_WINDOW`。
  - 可复用：这里是“窗口已经进入销毁阶段”的最稳定切点，适合一次性清空其余平台回调。
  - 需注意：如果销毁后还保留 `request_frame` / `input` / `hit_test_window_control` 等回调，晚到消息仍会继续命中上层窗口。

- **实现4**: `D:\zhp\src\onetcli\vendor\zed\crates\gpui\src\platform\windows\events.rs:855-935` 与 `D:\zhp\src\onetcli\vendor\zed\crates\gpui\src\platform\windows\events.rs:1231-1249`
  - 模式：`WM_NCHITTEST`、`WM_NCMOUSEMOVE`、`TrackMouseEvent` 直接依赖有效 `HWND`，内部会调用 `ScreenToClient`、`GetWindowRect`。
  - 可复用：这些入口天然适合先做 `IsWindow` 守卫，再决定是否继续事件处理。
  - 需注意：用户当前日志里的 `0x80070578` 很符合这里在关窗尾声继续触发 Win32 API 的现象。

- **实现5**: `D:\zhp\src\onetcli\vendor\zed\crates\gpui\src\platform\windows\window.rs:343-369` 与 `D:\zhp\src\onetcli\vendor\zed\crates\gpui\src\platform\windows\window.rs:556-575`
  - 模式：Windows 平台层把所有回调存放在 `Callbacks` 结构里；`Drop for WindowsWindow` 会异步执行 `RevokeDragDrop` / `DestroyWindow`。
  - 可复用：`Callbacks` 适合新增“销毁时整体清空”的入口；`Drop` 适合只忽略已知的无效句柄错误码。
  - 需注意：`Drop` 发生在异步任务里，执行时 `HWND` 可能已经失效，不能继续无条件 `.log_err()`。

### 2. 项目约定
- **命名约定**：Rust 函数与局部变量使用 `snake_case`，辅助常量使用全大写下划线。
- **文件组织**：Windows 平台通用工具在 `platform/windows/util.rs`，生命周期事件在 `platform/windows/events.rs`，窗口对象与回调存储在 `platform/windows/window.rs`。
- **错误处理风格**：正常路径使用 `Result` + `log_err()`；平台局部例外应尽量缩小范围，不改全局错误语义。
- **平台分层**：GPUI 通用窗口语义在 `vendor/zed/crates/gpui/src/window.rs` 与 `app.rs`，Windows 专属修复优先收敛在 `platform/windows/*`。

### 3. 可复用组件清单
- `D:\zhp\src\onetcli\vendor\zed\crates\gpui\src\platform\windows\window.rs`：`Callbacks`
- `D:\zhp\src\onetcli\vendor\zed\crates\gpui\src\platform\windows\events.rs`：`handle_destroy_msg(...)`
- `D:\zhp\src\onetcli\vendor\zed\crates\gpui\src\app.rs`：`update_window_id(...)`
- `D:\zhp\src\onetcli\vendor\zed\crates\gpui\src\window.rs`：`WindowHandle::update(...)`

### 4. 测试策略
- **测试框架**：Rust 编译检查为主。
- **参考验证**：`cargo check -p one-core`，确认修改后的 `vendor/zed` 能被工作区下游 crate 正常编译。
- **额外验证**：
  - `rustfmt --edition 2024` 针对修改文件单独格式化。
  - `cargo check -p main` 作为整应用编译补充，但当前环境缺少 `cmake`/`nasm`。
  - `cargo check -p gpui` 可单独验证依赖库，但当前环境无法访问 `static.crates.io`。

### 5. 依赖和集成点
- **外部依赖**：Windows API `IsWindow`、`RevokeDragDrop`、`DestroyWindow`、`ScreenToClient`、`GetWindowRect`。
- **内部依赖**：GPUI 的 `on_close -> remove_window()` 生命周期；Windows 平台层 `Callbacks` 的持有与派发。
- **集成方式**：修复保持在 `vendor/zed/crates/gpui/src/platform/windows` 内，不改业务层 `TabContainer` 或应用入口。
- **配置来源**：无新增配置项。

### 6. 技术选型理由
- **为什么用这个方案**：用户日志明确指向 Windows 关窗尾声的生命周期噪音，最小风险方案就是在 Windows 平台层本地断开晚到回调，并只忽略“无效句柄”这种关窗后预期错误。
- **优势**：不修改 GPUI 全局 `window not found` 语义，不吞掉其他真实错误，不影响 macOS/Linux。
- **劣势和风险**：如果未来出现新的关窗错误码，当前只覆盖已确认的两类无效句柄错误，仍可能继续记录日志。

### 7. 关键风险点
- **生命周期边界**：如果 `WM_DESTROY` 之前已有事件回调取走了闭包，理论上仍可能存在极短时间窗。
- **平台差异**：本次只收敛 Windows 平台层，不覆盖其他平台的晚到事件。
- **验证缺口**：缺少 GUI 自动化与真实关闭场景日志对照，仍需 Windows 实机再确认一次。
- **环境阻塞**：`main` 的全量编译当前受 `cmake` / `nasm` 缺失影响，`gpui` 单包检查受网络沙箱影响。
