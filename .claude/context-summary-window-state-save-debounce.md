## 项目上下文摘要（window-state-save-debounce）
生成时间：2026-03-28 23:02:00 +08:00

### 1. 相似实现分析
- **实现1**: `D:\zhp\src\onetcli\main\src\onetcli_app.rs:418-423`
  - 模式：主窗口通过 `observe_window_bounds(...)` 监听窗口 bounds 变化。
  - 可复用：这里可以继续作为窗口状态捕获入口。
  - 需注意：如果回调里同步写盘，会在窗口拖动过程中高频阻塞主线程。

- **实现2**: `D:\zhp\src\onetcli\crates\core\src\tab_persistence.rs:68-97`
  - 模式：标签布局保存使用延迟定时任务，避免每次布局变化都立刻写文件。
  - 可复用：窗口状态保存也应采用同类“延迟/防抖写盘”策略。
  - 需注意：退出前仍需要一次兜底落盘，避免最后一次变化尚未触发延迟保存。

- **实现3**: `D:\zhp\src\onetcli\crates\core\src\utils\debouncer.rs:25-71`
  - 模式：项目已有通用 `Debouncer`，支持在一段静默时间后只执行最后一次操作。
  - 可复用：直接复用来抑制窗口移动/缩放过程中的高频保存。
  - 需注意：`debounce` 只是决定“这次是否应该执行”，真正的写盘仍需业务层显式调用。

### 2. 项目约定
- **命名约定**：异步保存辅助字段使用 `_task` 后缀，常量使用全大写下划线。
- **代码风格**：优先复用已有防抖工具，不自行再造一套节流器。
- **文件组织**：窗口监听逻辑留在 `main/src/onetcli_app.rs`，设置写盘逻辑留在 `main/src/setting_tab.rs`。

### 3. 可复用组件清单
- `D:\zhp\src\onetcli\crates\core\src\utils\debouncer.rs`：`Debouncer`
- `D:\zhp\src\onetcli\crates\core\src\tab_persistence.rs`：延迟保存模式
- `D:\zhp\src\onetcli\main\src\setting_tab.rs`：`AppSettings`

### 4. 测试策略
- `rustfmt --edition 2024` 检查修改文件格式
- 静态核对“捕获窗口状态 -> 防抖保存 -> 退出兜底保存”闭环
- Windows 实机手动验证窗口拖动是否恢复流畅，以及关闭后重启是否恢复尺寸/状态

### 5. 关键风险点
- 无法在当前环境完成 GUI 自动化验证，仍需用户实机确认。
- `cargo check -p main` 仍受 `cmake` / `nasm` 缺失影响，无法完成全量编译验证。
