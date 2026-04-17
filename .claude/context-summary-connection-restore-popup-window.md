## 项目上下文摘要（connection-restore-popup-window）
生成时间：2026-03-29 05:04:03 +08:00

### 0. 检索说明
- 当前会话未提供 `desktop-commander`、`context7`、`github.search_code`、`sequential-thinking` 工具。
- 本次改用仓库内 `rg`、源码阅读和已有 `main` / `crates/core` 实现做等效上下文分析。
- 用户继续反馈“难拖动、拖动不跟手、十分卡顿”后，确认问题根因已经从“布局”升级为“弹窗类型选错”，需要从应用内 `Dialog` 切到独立 `popup window`。

### 1. 相似实现分析
- **实现1**: `D:\usr\htdocs\onetcli\main\src\connection_restore.rs:163-186`
  - 模式：恢复连接提示的入口已经开始迁移到 `open_popup_window_with_should_close(...)`，但恢复按钮仍停留在错误的上下文调用上。
  - 可复用：保留当前恢复项列表和勾选逻辑，只修正打开与关闭链路。
  - 需注意：恢复动作最终要调用 `HomePage::restore_saved_connection_sessions(...)`，它明确需要主窗口 `&mut Window`。

- **实现2**: `D:\usr\htdocs\onetcli\crates\core\src\popup_window.rs:163-222`
  - 模式：仓库内其它可正常拖动的表单弹窗统一走 `open_popup_window(...)` 创建独立窗口。
  - 可复用：新增 `open_popup_window_with_should_close(...)` 后，可以对右上角关闭按钮自定义“先清理恢复快照，再关窗”的逻辑。
  - 需注意：窗口位置应基于 `primary_display().visible_bounds()` 居中，而不是整块 `bounds()`。

- **实现3**: `D:\usr\htdocs\onetcli\main\src\onetcli_app.rs:39-50`
  - 模式：主窗口实体和首页实体通过 `GlobalMainWindowHandle`、`GlobalHomePage` 挂在全局。
  - 可复用：popup 内部若要驱动主窗口行为，应通过 `cx.try_global::<GlobalMainWindowHandle>()` 拿句柄后再 `cx.update_window(...)`。
  - 需注意：不能在 popup 视图的 `Context<Self>` 上直接调用 `Entity::update_in(...)`，因为这要求 `VisualContext`。

- **实现4**: `D:\usr\htdocs\onetcli\crates\core\src\certificate_manager.rs:780-907`
  - 模式：独立 popup 内容区统一使用按钮触发 `request_popup_window_close(...)`，保存逻辑在对应视图上下文中完成。
  - 可复用：恢复连接 popup 也应沿用“底部按钮栏 + 关闭窗口辅助函数”的交互风格。
  - 需注意：关闭路径应保持幂等，避免重复清理引发状态紊乱。

- **实现5**: `D:\usr\htdocs\onetcli\main\src\home_tab.rs:505-560`
  - 模式：恢复提示的打开、跳过和真正恢复逻辑全部收敛在 `HomePage` 中。
  - 可复用：`skip_pending_connection_restore(...)` 和 `restore_saved_connection_sessions(...)` 已具备完整状态清理与标签恢复逻辑。
  - 需注意：恢复前会先清空待恢复快照，所以 popup 关闭、Esc、右上角关闭都必须落到“跳过恢复”语义。

### 2. 项目约定
- **命名约定**：Rust 类型使用 `PascalCase`，函数和局部变量使用 `snake_case`。
- **文件组织**：popup 通用能力留在 `crates/core/src/popup_window.rs`，恢复业务留在 `main/src/connection_restore.rs`。
- **代码风格**：优先复用既有 `popup_window` 能力，不为单一弹窗再造第二套拖动或定位机制。
- **交互风格**：复杂表单或长列表优先使用独立 popup window，页内模态 `Dialog` 适用于轻量确认类交互。

### 3. 可复用组件清单
- `D:\usr\htdocs\onetcli\crates\core\src\popup_window.rs`：统一 popup 创建、聚焦和关闭能力
- `D:\usr\htdocs\onetcli\main\src\onetcli_app.rs`：`GlobalMainWindowHandle`
- `D:\usr\htdocs\onetcli\main\src\home_tab.rs`：恢复连接的跳过与恢复主逻辑
- `D:\usr\htdocs\onetcli\crates\core\src\certificate_manager.rs`：popup 底部操作栏与关闭模式

### 4. 测试策略
- **测试框架**：Rust 内置单元测试。
- **本次策略**：
  - 继续保留恢复 popup 尺寸收敛测试，确保独立窗口尺寸不会失控。
  - 运行 `cargo test -p main connection_restore -- --nocapture`，确认迁移后模块可编译且布局测试通过。
  - 运行 `cargo test -p main 主窗口 -- --nocapture`，确认主窗口恢复链路与 popup 相关测试一起通过。
- **覆盖重点**：popup 尺寸上限、主窗口恢复调用链、关闭时恢复快照清理语义。

### 5. 依赖和集成点
- **外部依赖**：GPUI `WindowOptions`、`WindowBounds`、`visible_bounds()`、`update_window(...)`
- **内部依赖**：`HomePage::restore_saved_connection_sessions(...)`、`HomePage::skip_pending_connection_restore(...)`
- **集成方式**：恢复提示由主窗口 defer 打开独立 popup；popup 内部通过全局主窗口句柄回到主窗口上下文执行恢复
- **配置来源**：无新增配置，尺寸与位置均按当前窗口和显示器可见区域运行时计算

### 6. 技术选型理由
- **为什么不继续修 `Dialog` 拖动**：用户连续反馈“能拖但卡顿、不跟手”，这更像是应用内模态重绘模型的问题，而不是标题栏命中区不足。
- **为什么切到 `popup_window`**：仓库里其它拖动正常的复杂弹窗都走独立窗口链路，复用已有成熟方案回归面更小。
- **为什么新增 `open_popup_window_with_should_close(...)`**：恢复提示需要把右上角关闭按钮映射为“跳过恢复并清理快照”，默认关闭逻辑不够用。
- **为什么恢复动作必须回主窗口执行**：恢复标签页需要主窗口 `Window` 作为目标上下文，popup 自己的窗口不能替代主窗口。

### 7. 关键风险点
- **真实交互验证**：当前验证以编译和单元测试为主，尚未附带 Windows 实机拖动录屏。
- **关闭动作幂等性**：Esc、右上角关闭、底部“跳过”最终都会走清理快照逻辑，虽然当前实现是幂等的，但后续若扩展副作用需继续留意。
- **多窗口前提**：当前实现依赖全局主窗口句柄；若未来支持多个主窗口，需要重新定义恢复目标窗口选择策略。
