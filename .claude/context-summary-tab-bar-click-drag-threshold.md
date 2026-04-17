## 项目上下文摘要（tab-bar-click-drag-threshold）
生成时间：2026-03-29 11:57:22 +08:00

### 0. 检索说明
- 当前会话未提供 `desktop-commander`、`context7`、`github.search_code`、`sequential-thinking`。
- 本次改用仓库内 `rg`、源码阅读、`git diff` 和既有 `.claude` 留痕做等效上下文分析。
- 用户问题聚焦于“tab-bar 中鼠标单击 tab 被误判为拖动”，因此检索重点放在 tab 激活、tab 拖拽和 GPUI 拖拽阈值上。

### 1. 相似实现分析
- **实现1**: `D:\usr\htdocs\onetcli\crates\core\src\tab_container.rs:2027-2068`
  - 模式：主业务 tab-bar 的每个 tab 由自定义 `div()` 渲染，同时绑定 `on_drag(...)` 和 `on_click(...)`。
  - 可复用：继续保留现有 `DragTab`、`move_tab(...)`、`set_active_index(...)` 逻辑，不改排序流程。
  - 需注意：拖拽和点击命中区完全重叠，只要底层拖拽阈值过低，就会把正常点击升级成拖拽。

- **实现2**: `D:\usr\htdocs\onetcli\crates\ui\src\dock\tab_panel.rs:728-778`
  - 模式：通用停靠面板 tab 使用 `Tab::on_click(...)` 激活、`Tab::on_drag(...)` 拖拽。
  - 可复用：同样是“点击激活 + 拖动重排”的组合交互，适合一起收敛到相同阈值策略。
  - 需注意：如果只修主窗口 tab-bar，不修通用 dock tab，项目内同类交互会出现体验不一致。

- **实现3**: `D:\usr\htdocs\onetcli\crates\ui\src\tab\tab.rs:611-690`
  - 模式：通用 `Tab` 组件负责视觉状态、点击回调和基础鼠标事件隔离。
  - 可复用：继续沿用 `Tab` 组件现有 builder 风格，不额外引入新的 tab 专用交互组件。
  - 需注意：`Tab` 组件本身不控制拖拽阈值，阈值问题属于更底层的 `gpui` 交互系统。

- **实现4**: `D:\usr\htdocs\onetcli\vendor\zed\crates\gpui\src\elements\div.rs:2345-2364`
  - 模式：`gpui` 在元素绑定了 `on_drag(...)` 后，鼠标移动距离超过 `DRAG_THRESHOLD` 即进入拖拽态。
  - 可复用：框架已有统一的拖拽判定链路，只需补充“元素级阈值覆盖”能力，不必重写拖拽状态机。
  - 需注意：默认阈值只有 `2px`，对 tab 这类“优先点击、次要拖拽”的控件过于敏感。

### 2. 项目约定
- **命名约定**：Rust 常量与函数继续使用 `SCREAMING_SNAKE_CASE`/`snake_case`，类型使用 `PascalCase`。
- **文件组织**：主窗口 tab-bar 在 `crates/core/src/tab_container.rs`，通用 dock tab 在 `crates/ui/src/dock/tab_panel.rs`，底层交互在 `vendor/zed/crates/gpui/src/elements/div.rs`。
- **代码风格**：继续使用 GPUI builder 链、`.when(...)` 条件分支和小范围局部常量，不做大规模结构重写。

### 3. 可复用组件清单
- `D:\usr\htdocs\onetcli\crates\core\src\tab_container.rs`：`DragTab`
- `D:\usr\htdocs\onetcli\crates\core\src\tab_container.rs`：`move_tab(...)`
- `D:\usr\htdocs\onetcli\crates\core\src\tab_container.rs`：`set_active_index(...)`
- `D:\usr\htdocs\onetcli\crates\ui\src\tab\tab.rs`：`Tab`
- `D:\usr\htdocs\onetcli\vendor\zed\crates\gpui\src\elements\div.rs`：`Interactivity::on_drag(...)`

### 4. 测试策略
- **测试框架**：Rust 单元测试与编译检查。
- **参考用例**：`D:\usr\htdocs\onetcli\crates\core\src\tab_container.rs:2404-2419`
- **本次策略**：
  - 运行 `cargo test -p one-core tab_container::tests --lib -- --nocapture`，确认主 tab-bar 相关逻辑仍通过。
  - 通过上述测试过程实际编译 `gpui` 和 `gpui-component`，覆盖新的元素级拖拽阈值 API 以及 `dock/tab_panel.rs` 调用点。
  - 尝试补跑 `gpui` 新增单测；若受环境权限或网络限制阻塞，则在验证记录中明确说明。

### 5. 依赖和集成点
- **外部依赖**：`gpui` 的 `Interactivity` 拖拽判定链路。
- **内部依赖**：
  - `D:\usr\htdocs\onetcli\crates\core\src\tab_container.rs`
  - `D:\usr\htdocs\onetcli\crates\ui\src\dock\tab_panel.rs`
  - `D:\usr\htdocs\onetcli\vendor\zed\crates\gpui\src\elements\div.rs`
- **集成方式**：在底层保留默认 `2px` 阈值，只给 tab 相关元素设置 `6px` 自定义阈值。

### 6. 技术选型理由
- **为什么不直接调大全局阈值**：会影响滑块、分栏拖拽、表格列宽调整等所有 `on_drag(...)` 交互，回归面太大。
- **为什么扩展 `gpui` 能力**：项目内已有多处“点击激活 + 拖拽排序”的 tab 交互，补一个元素级阈值 API 可以在多个 tab 场景复用。
- **为什么阈值取 `6px`**：相比默认 `2px` 更能覆盖正常点击时的轻微手抖，又不会把拖拽重排变成明显吃力的操作。
- **为什么还要把激活前移到 `mouse_down`**：仅靠提高阈值仍然依赖“鼠标抬起前没有被升级成拖拽”这个条件；对可拖拽 tab，更稳妥的交互是“按下即激活，继续移动再拖拽”。

### 7. 关键风险点
- **真实交互风险**：当前本地验证以编译和单测为主，仍建议用户实机确认“单击激活”和“拖动重排”手感是否符合预期。
- **验证环境限制**：直接运行 `gpui` crate 单测时受到 Cargo git 目录权限和网络限制影响，无法完成独立框架测试。
- **一致性风险**：这次已同步调整主 tab-bar 和 dock tab；若仓库后续新增新的 tab 拖拽入口，也应沿用相同阈值策略。
