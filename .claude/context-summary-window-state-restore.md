## 项目上下文摘要（window-state-restore）
生成时间：2026-03-28 22:24:00 +08:00

### 1. 相似实现分析
- **实现1**: `D:\zhp\src\onetcli\main\src\main.rs:51-68`
  - 模式：主窗口启动时统一构造 `WindowOptions`，当前默认使用固定居中的 `Bounds`。
  - 可复用：窗口恢复逻辑最适合接在这里，直接复用 `WindowOptions.window_bounds`。
  - 需注意：如果这里只写死 `WindowBounds::Windowed(...)`，即使设置里已有状态也不会被使用。

- **实现2**: `D:\zhp\src\onetcli\main\src\setting_tab.rs:437-493`
  - 模式：`AppSettings` 统一负责 `settings.json` 的读取、序列化和写盘。
  - 可复用：窗口状态应并入 `AppSettings`，避免新增第二套配置文件。
  - 需注意：新字段必须带 `#[serde(default)]`，否则旧配置升级会解析失败。

- **实现3**: `D:\zhp\src\onetcli\main\src\home_tab.rs:3215-3249`
  - 模式：界面偏好修改后通过 `cx.update_global::<AppSettings, _>(...)` 立即写盘。
  - 可复用：窗口状态也应采用“运行时增量保存”而不是只在退出时保存。
  - 需注意：这样能避开关窗阶段句柄已经失效的生命周期边界。

- **实现4**: `D:\zhp\src\onetcli\vendor\zed\crates\gpui\src\app\context.rs:428-444`
  - 模式：GPUI 提供 `observe_window_bounds(...)` 监听窗口 bounds 变化。
  - 可复用：窗口移动、缩放、最大化切换都可以通过这条链路捕获。
  - 需注意：注册时会立即激活一次回调，因此保存逻辑必须可重复、幂等。

- **实现5**: `D:\zhp\src\onetcli\vendor\zed\crates\gpui\src\window.rs:1118-1167`
  - 模式：窗口创建时读取 `WindowOptions.window_bounds`，并在 `Maximized/Fullscreen` 情况下主动恢复对应状态。
  - 可复用：只要业务层传入正确的 `WindowBounds`，GPUI 会负责窗口态恢复。
  - 需注意：`WindowBounds::Maximized(bounds)` 中的 `bounds` 是恢复尺寸，不是当前铺满屏幕尺寸。

- **实现6**: `D:\zhp\src\onetcli\vendor\zed\crates\gpui\src\platform\windows\window.rs:177-208`
  - 模式：Windows 平台层的 `window.window_bounds()` 会返回当前窗口状态以及可恢复的 bounds。
  - 可复用：业务层无需自己拼最大化恢复尺寸，直接持久化这个返回值即可。
  - 需注意：关闭阶段再读这类状态风险较高，因此更适合在运行时提前保存。

### 2. 项目约定
- **命名约定**：Rust 类型使用 `PascalCase`，函数和局部变量使用 `snake_case`。
- **文件组织**：应用启动在 `main/src/main.rs`，应用生命周期在 `main/src/onetcli_app.rs`，持久化配置在 `main/src/setting_tab.rs`。
- **代码风格**：优先复用现有 `AppSettings`、`observe_*` 和 GPUI 原生 `WindowBounds` 机制，不新增平行框架。
- **兼容方式**：配置新增字段一律走 `serde default`，确保旧 `settings.json` 平滑升级。

### 3. 可复用组件清单
- `D:\zhp\src\onetcli\main\src\setting_tab.rs`：`AppSettings::load/save`
- `D:\zhp\src\onetcli\main\src\home_tab.rs`：偏好项增量保存模式
- `D:\zhp\src\onetcli\vendor\zed\crates\gpui\src\app\context.rs`：`observe_window_bounds(...)`
- `D:\zhp\src\onetcli\vendor\zed\crates\gpui\src\platform.rs`：`WindowBounds`
- `D:\zhp\src\onetcli\vendor\zed\crates\gpui\src\platform\windows\window.rs`：`window_bounds()`

### 4. 测试策略
- **测试框架**：Rust 内置单元测试 + `cargo check`
- **本次可做验证**：
  - `rustfmt --edition 2024` 校验修改文件格式
  - `cargo check -p main` 尝试验证主应用编译链
  - 静态核对 `main.rs -> AppSettings -> observe_window_bounds` 闭环
- **本次受限项**：
  - `cargo check -p main` 仍会因环境缺少 `cmake` / `nasm` 停在 `aws-lc-sys`
  - 无 GUI 自动化，最终仍需 Windows 实机手动关闭/重启应用确认

### 5. 依赖和集成点
- **外部依赖**：GPUI 的 `WindowBounds`、`observe_window_bounds(...)`、`window.window_bounds()`
- **内部依赖**：`AppSettings` 全局设置、主窗口初始化路径、现有标签布局保存逻辑
- **集成方式**：在运行时保存窗口状态，在启动时恢复；不改现有 tab 布局存档机制
- **配置来源**：`settings.json`

### 6. 技术选型理由
- **为什么复用 `AppSettings`**：当前仓库已有成熟的设置持久化链路，复用成本最低，也最符合现有结构。
- **为什么用 `observe_window_bounds`**：窗口关闭时再读取状态容易撞上 Windows 句柄销毁边界，运行时增量保存更稳。
- **为什么保存 `WindowBounds` 而不是单独存一个 `maximized` 布尔值**：GPUI 已经把“窗口态 + 恢复尺寸”封装好，直接复用语义更完整。

### 7. 关键风险点
- **实机风险**：缺少 GUI 自动化，仍需手工验证“调整尺寸/最大化/关闭/重启”全链路。
- **显示器风险**：如果用户跨显示器切换环境，保存的位置可能越界；本次先保证状态链路恢复，不额外重写多显示器校正逻辑。
- **环境风险**：当前会话无法完成主应用全量编译，需依赖静态审查与后续实机验证补齐。
