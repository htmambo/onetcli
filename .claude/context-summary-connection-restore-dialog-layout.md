## 项目上下文摘要（connection-restore-dialog-layout）
生成时间：2026-03-29 04:44:42 +08:00

### 0. 检索说明
- 当前会话未提供 `desktop-commander`、`context7`、`github.search_code`、`sequential-thinking` 工具。
- 本次改用仓库内 `rg`、源码阅读和已有 `vendor/zed` / `crates/ui` 实现做等效上下文分析。
- 用户二次反馈后已确认：问题对象不是主窗口恢复，而是启动时弹出的“恢复连接”对话框。

### 1. 相似实现分析
- **实现1**: `D:\usr\htdocs\onetcli\main\src\connection_restore.rs:147-187`
  - 模式：启动恢复提示通过 `window.open_dialog(...)` 打开，当前宽度固定 `620px`，内部列表最大高度固定 `360px`。
  - 可复用：只需改此对话框的布局计算即可，不必继续动主窗口恢复链路。
  - 需注意：当前 builder 没有读取主窗口 viewport，固定尺寸在小窗口下会失配。

- **实现2**: `D:\usr\htdocs\onetcli\crates\ui\src\dialog.rs:381-401`
  - 模式：通用 `Dialog` 的默认垂直定位按固定 `360px` 高度估算中心点，再加上拖动偏移。
  - 可复用：了解默认行为后，可以在业务层通过 `margin_top(...)` 主动覆盖定位。
  - 需注意：当对话框真实高度明显大于 360px 时，默认定位会把底部推到视口外。

- **实现3**: `D:\usr\htdocs\onetcli\crates\ui\src\dialog.rs:569-605`
  - 模式：对话框只允许通过标题栏拖动，标题栏本身是整行 drag hit area。
  - 可复用：如果要改善“难拖动”的感受，可以增大标题区的有效视觉高度，而不必改底层拖动状态机。
  - 需注意：恢复对话框当前标题只有单行文本，可拖动区域视觉上不够明显。

- **实现4**: `D:\usr\htdocs\onetcli\main\src\update.rs:217-256`
  - 模式：普通更新提示也是 `window.open_dialog(...)`，宽度固定但内容相对较矮，因此不触发超界问题。
  - 可复用：继续沿用 `DialogButtonProps`、`confirm()` 和回调模式。
  - 需注意：不能拿矮对话框的固定高度经验直接套到恢复连接对话框。

- **实现5**: `D:\usr\htdocs\onetcli\main\src\home_tab.rs:1188-1215`
  - 模式：冲突处理对话框通过 `max_h(...).overflow_y_scroll()` 限制主体高度，避免列表把整个 Dialog 撑爆。
  - 可复用：恢复连接列表也应按当前视口动态限制最大高度。
  - 需注意：固定 `400px` 在不同窗口高度下仍可能不稳，最好用 viewport 自适应。

### 2. 项目约定
- **命名约定**：Rust 类型用 `PascalCase`，函数和局部变量用 `snake_case`。
- **文件组织**：恢复提示行为收敛在 `main/src/connection_restore.rs`；通用对话框能力保留在 `crates/ui/src/dialog.rs`。
- **代码风格**：优先在业务层计算布局参数，避免直接改通用 `Dialog` 影响全局弹窗。
- **交互风格**：继续使用 `Dialog + Checkbox + confirm()` 组合，不切换到另一套弹窗框架。

### 3. 可复用组件清单
- `D:\usr\htdocs\onetcli\main\src\connection_restore.rs`：恢复连接对话框 builder 与勾选状态管理
- `D:\usr\htdocs\onetcli\crates\ui\src\dialog.rs`：`margin_top(...)`、标题栏拖动机制
- `D:\usr\htdocs\onetcli\main\src\home_tab.rs`：滚动列表对话框的 `max_h(...).overflow_y_scroll()` 模式
- `D:\usr\htdocs\onetcli\main\src\update.rs`：标准 `window.open_dialog(...)` 构造方式

### 4. 测试策略
- **测试框架**：Rust 内置单元测试。
- **本次策略**：
  - 给恢复对话框新增纯逻辑布局 helper，测试“小窗口收敛”和“大窗口居中”两种场景。
  - 再运行主窗口相关已有测试，确保本轮修复未破坏前一轮 `setting_tab` 逻辑。
- **覆盖重点**：宽度收敛、列表高度收敛、顶部偏移不再把对话框放到视口外。

### 5. 依赖和集成点
- **外部依赖**：GPUI `Window::viewport_size()`、`Pixels`、`Size`
- **内部依赖**：`ConnectionRestoreDialogView` 的滚动列表、`Dialog::margin_top(...)` 和恢复确认回调
- **集成方式**：在打开恢复对话框前计算布局参数，传入 builder 和视图实体
- **配置来源**：无新增配置，完全运行时按当前主窗口 viewport 计算

### 6. 技术选型理由
- **为什么不再改主窗口恢复链路**：用户复现表明真正越界的是启动恢复提示对话框，不是主窗口本身。
- **为什么在业务层算布局**：通用 `Dialog` 被大量模块复用，直接改默认定位会扩大回归面。
- **为什么把列表高度做成动态值**：恢复连接项数量不确定，只有收紧列表最大高度，才能稳定控制整个对话框高度。
- **为什么增加更明显的顶部标题区**：底层拖动逻辑已经存在，改善可视拖动区比重写拖动机制更稳妥。

### 7. 关键风险点
- **估算误差**：对话框总高度仍是基于“非列表区域保留高度”的估算值，不是运行时实测布局。
- **极小窗口**：如果将来主窗口最小尺寸再缩小，当前 `dialog_width` 下限策略还需复核。
- **交互预期**：本次改善的是“更容易拖”和“不越界”，但它仍然是应用内 `Dialog`，不会变成系统原生弹窗窗口。
