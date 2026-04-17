## 项目上下文摘要（window-restore-center）
生成时间：2026-03-29 04:24:08 +08:00

### 0. 检索说明
- 当前会话未提供 `desktop-commander`、`context7`、`github.search_code`、`sequential-thinking` 工具。
- 本次改用仓库内 `rg` 检索、已有测试代码和 `vendor/zed` 中的 GPUI 源码进行等效上下文分析，并在操作日志中留痕。

### 1. 相似实现分析
- **实现1**: `D:\usr\htdocs\onetcli\main\src\main.rs:53-60`
  - 模式：启动时先按主屏尺寸裁剪默认窗口大小，再调用 `AppSettings::restored_main_window_bounds(...)` 决定最终启动位置。
  - 可复用：恢复窗口越界校验应继续放在 `AppSettings`，避免把显示器逻辑散落回 `main.rs`。
  - 需注意：`main.rs` 只负责启动参数拼装，不适合新增复杂坐标修正逻辑。

- **实现2**: `D:\usr\htdocs\onetcli\main\src\setting_tab.rs:210-261`
  - 模式：`SavedWindowBounds` 统一负责窗口状态的序列化、反序列化与 `WindowBounds` 转换。
  - 可复用：越界恢复逻辑最适合挂在这里，直接复用已有 `SavedWindowDisplayState` 和 `WindowBounds` 语义。
  - 需注意：`Maximized/Fullscreen` 保存的是恢复尺寸，不是当前铺满屏幕尺寸。

- **实现3**: `D:\usr\htdocs\onetcli\main\src\setting_tab.rs:605-612`
  - 模式：`restored_main_window_bounds(...)` 当前只做“有值就恢复，否则居中”的简单分支。
  - 可复用：只需把“有值就恢复”替换成“有值且仍在可见区域内才恢复，否则居中”。
  - 需注意：这里已拿到 `&App`，可以直接访问显示器列表，无需改调用链。

- **实现4**: `D:\usr\htdocs\onetcli\crates\core\src\popup_window.rs:153-160`
  - 模式：弹窗先按主屏大小裁剪尺寸，再使用 `Bounds::centered(...)` 居中。
  - 可复用：越界回退时应沿用“先限制尺寸，再居中”的交互预期。
  - 需注意：如果只改坐标不改尺寸，窗口仍可能继续超出显示区域。

- **实现5**: `D:\usr\htdocs\onetcli\vendor\zed\crates\gpui\src\platform.rs:307-325`
  - 模式：GPUI 的 `PlatformDisplay` 同时提供 `bounds()` 和 `visible_bounds()`，后者会排除任务栏/停靠栏。
  - 可复用：恢复校验应基于 `visible_bounds()` 而不是整块屏幕 bounds。
  - 需注意：这决定了窗口是否真正落在用户可见可操作区域。

- **实现6**: `D:\usr\htdocs\onetcli\vendor\zed\crates\gpui\src\geometry.rs:771-782` 与 `1438-1463`
  - 模式：`Bounds::centered_at(...)` 提供指定中心点居中，`is_contained_within(...)` 提供完整包含判断。
  - 可复用：可直接用“是否被任一显示器可见区域完整包含”判断越界，再用可见区域中心回退。
  - 需注意：完整包含比简单相交更符合“不要出界”的需求。

### 2. 项目约定
- **命名约定**：Rust 类型使用 `PascalCase`，函数与局部变量使用 `snake_case`。
- **文件组织**：窗口恢复逻辑归 `main/src/setting_tab.rs`，启动入口保留在 `main/src/main.rs`。
- **代码风格**：优先复用 GPUI 原生 `Bounds`、`WindowBounds`、`visible_bounds()`，不新增平台特判分支。
- **测试风格**：`main/src/setting_tab.rs` 以同文件 `#[test]` 为主，测试名使用中文场景描述。

### 3. 可复用组件清单
- `D:\usr\htdocs\onetcli\main\src\setting_tab.rs`：`SavedWindowBounds::from_window_bounds`、`SavedWindowBounds::to_window_bounds`
- `D:\usr\htdocs\onetcli\main\src\setting_tab.rs`：`AppSettings::restored_main_window_bounds`
- `D:\usr\htdocs\onetcli\vendor\zed\crates\gpui\src\platform.rs`：`PlatformDisplay::visible_bounds`
- `D:\usr\htdocs\onetcli\vendor\zed\crates\gpui\src\geometry.rs`：`Bounds::centered_at`、`Bounds::is_contained_within`
- `D:\usr\htdocs\onetcli\vendor\zed\crates\gpui\src\app\test_context.rs`：`TestAppContext::single`、`update(...)`

### 4. 测试策略
- **测试框架**：Rust 内置单元测试。
- **参考文件**：`D:\usr\htdocs\onetcli\main\src\setting_tab.rs:1259-1355`
- **本次新增测试目标**：
  - 保存位置完全越界时，恢复结果改为主屏居中。
  - 保存尺寸大于当前可见区域时，恢复结果会先裁剪再居中。
  - 原有合法/非法状态转换测试继续保留。

### 5. 依赖和集成点
- **外部依赖**：GPUI 的 `App::displays()`、`primary_display()`、`PlatformDisplay::visible_bounds()`、`Bounds`、`WindowBounds`
- **内部依赖**：`AppSettings` 的启动恢复链路，主窗口默认尺寸计算
- **集成方式**：仅在配置恢复阶段做校正，不改保存阶段、不改窗口监听阶段
- **配置来源**：`settings.json` 中的 `main_window_bounds`

### 6. 技术选型理由
- **为什么在 `AppSettings` 修复**：当前唯一的恢复入口就在这里，改动最聚焦，回归面最小。
- **为什么用 `visible_bounds()`**：任务栏和 Dock 不应被视为可恢复区域，否则“可见但点不到”的问题仍会保留。
- **为什么选择“越界即重新居中”**：用户诉求是启动时不要落到窗口外，应回到应用中间，而不是继续保留一部分越界的位置。
- **为什么保留原窗口状态**：对 `Maximized/Fullscreen` 仅修正恢复尺寸和锚点，不强行降级窗口态，兼容现有 GPUI 语义。

### 7. 关键风险点
- **多显示器切换**：用户原先保存在副屏的位置，在只剩主屏时会被回收到主屏中间。
- **窗口跨屏摆放**：新逻辑会把“部分越界/跨屏”统一视为不合法并回中，这与旧行为有差异，但更符合本次需求。
- **验证限制**：当前以单元测试和静态审查为主，仍缺少真实 Windows GUI 启动后的肉眼验证。
