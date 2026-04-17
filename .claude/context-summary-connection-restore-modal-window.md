## 项目上下文摘要（connection-restore-modal-window）
生成时间：2026-03-29 06:18:00 +08:00

### 0. 检索说明
- 当前会话未提供 `desktop-commander`、`context7`、`github.search_code`、`sequential-thinking` 工具。
- 本次改用仓库内 `rg`、源码阅读和 `vendor/zed` 平台实现做等效上下文分析。
- 用户反馈“关闭了主窗口，恢复窗口还没关闭；恢复窗口应该排它，未确认前其它功能不能响应”后，问题从“拖动/位置”收敛为“窗口类型与生命周期绑定错误”。

### 1. 相似实现分析
- **实现1**: `D:\usr\htdocs\onetcli\main\src\connection_restore.rs:156-188`
  - 模式：恢复连接提示当前通过 `open_popup_window_with_should_close(...)` 打开独立窗口。
  - 可复用：保留现有恢复列表、跳过和恢复逻辑，只改底层窗口类型。
  - 需注意：当前窗口类型默认是普通 `Normal`，因此主窗口和恢复窗口彼此独立，不具备排它性。

- **实现2**: `D:\usr\htdocs\onetcli\crates\core\src\popup_window.rs:87-224`
  - 模式：popup helper 当前只暴露尺寸和标题，窗口类型固定写死为 `WindowKind::Normal`。
  - 可复用：扩展 `PopupWindowOptions` 即可把不同 popup 场景映射到不同 `WindowKind`。
  - 需注意：不能为了恢复窗口改坏其它 popup；默认值必须维持 `Normal`。

- **实现3**: `D:\usr\htdocs\onetcli\vendor\zed\crates\gpui\src\platform\windows\window.rs:412-454`
  - 模式：Windows 下 `WindowKind::Dialog` 会取当前活动窗口作为父窗口，并先 `EnableWindow(parent, false)` 禁用父窗口。
  - 可复用：这是仓库依赖中现成的系统级模态语义，不需要业务层自己去屏蔽主窗口输入。
  - 需注意：对话框销毁时需要恢复父窗口可交互状态。

- **实现4**: `D:\usr\htdocs\onetcli\vendor\zed\crates\gpui\src\platform\windows\events.rs:272-286`
  - 模式：Windows 下 `Dialog` 销毁时会自动重新启用父窗口并拉回前台。
  - 可复用：恢复窗口关闭后自动把焦点和交互权还给主窗口。
  - 需注意：这要求恢复窗口本身必须被创建成 `Dialog`，而不是普通 `Normal`。

- **实现5**: `D:\usr\htdocs\onetcli\vendor\zed\crates\gpui\src\platform\linux\wayland\window.rs:185-198` 与 `...linux/x11/window.rs:658-668`
  - 模式：Linux 下 `WindowKind::Dialog` 也会设置父窗口关系，并将窗口标记为 modal/dialog。
  - 可复用：恢复窗口的排它性语义可以沿用跨平台底层实现，而不需要单独写 Windows 特判。
  - 需注意：不同桌面环境观感可能不同，但父子关系和模态标记是现成的。

### 2. 项目约定
- **命名约定**：Rust 类型使用 `PascalCase`，函数和局部变量使用 `snake_case`。
- **文件组织**：通用 popup 行为在 `crates/core/src/popup_window.rs`，恢复业务在 `main/src/connection_restore.rs`。
- **代码风格**：优先复用底层窗口系统已有的 `WindowKind` 语义，不在业务层自造“假模态”状态机。

### 3. 可复用组件清单
- `D:\usr\htdocs\onetcli\crates\core\src\popup_window.rs`：统一 popup 打开链路
- `D:\usr\htdocs\onetcli\vendor\zed\crates\gpui\src\platform\windows\window.rs`：Windows `Dialog` 父窗口禁用逻辑
- `D:\usr\htdocs\onetcli\vendor\zed\crates\gpui\src\platform\windows\events.rs`：Windows `Dialog` 销毁时恢复父窗口
- `D:\usr\htdocs\onetcli\main\src\connection_restore.rs`：恢复连接视图与关闭逻辑

### 4. 测试策略
- **测试框架**：Rust 单元测试 + 针对性模块测试。
- **本次策略**：
  - 运行 `cargo test -p main connection_restore -- --nocapture`
  - 运行 `cargo test -p main 主窗口 -- --nocapture`
- **覆盖重点**：改动后恢复弹窗链路仍可编译，主窗口与恢复相关逻辑不回归。

### 5. 依赖和集成点
- **外部依赖**：GPUI `WindowKind::Dialog`
- **内部依赖**：`PopupWindowOptions`、`open_popup_window_with_should_close(...)`
- **集成方式**：恢复窗口通过 popup helper 打开，但窗口类型从默认 `Normal` 切到 `Dialog`

### 6. 技术选型理由
- **为什么不退回主窗口内 `Dialog`**：此前恢复提示之所以迁移到 popup，是为了解决长列表弹窗拖动不跟手和卡顿问题；这次问题是“缺少模态语义”，不必回退到旧链路。
- **为什么用 `WindowKind::Dialog`**：底层已提供父窗口禁用、销毁恢复、平台模态标记等完整能力，最符合“排它性”的需求。
- **为什么扩展 `PopupWindowOptions`**：只让恢复窗口覆写 kind，其他 popup 仍保持普通独立窗口语义。

### 7. 关键风险点
- **实机验证**：当前测试覆盖编译和纯逻辑，仍需在 Windows 桌面确认“主窗口不可响应”和“关闭主窗口不会留下孤儿恢复窗口”。
- **平台差异**：Linux/macOS 的模态体验由底层窗口系统决定，交互细节可能和 Windows 不完全一致，但父子关系会更正确。
