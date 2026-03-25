## 项目上下文摘要（table-data-printable-key-edit）
生成时间：2026-03-25 20:26:57 +0800

### 1. 相似实现分析
- **实现1**: `crates/one_ui/src/edit_table/state.rs`
  - 模式：表格单元格编辑入口统一收敛在 `EditTableState`，`on_cell_click(...)` 负责双击进入编辑，`start_editing(...)` 负责创建编辑器实体并切换状态。
  - 可复用：`current_cell_for_navigation()`、`start_editing(...)`、`commit_cell_edit(...)`
  - 需注意：当前没有表格级 `on_key_down` 监听，因此键盘字符输入无法触发编辑。

- **实现2**: `crates/one_ui/src/edit_table/delegate.rs`
  - 模式：编辑策略通过 `EditTableDelegate` 抽象，`single_click_to_edit()` 默认关闭，文档明确默认保持双击编辑。
  - 可复用：`cell_edit_enabled(...)`、`row_number_enabled(...)`
  - 需注意：这层已经定义了交互约束，不能为数据库视图单独改成单击编辑。

- **实现3**: `crates/db_view/src/table_data/results_delegate.rs`
  - 模式：数据库结果页通过 `build_input(...)` 为不同字段类型创建输入控件，并在创建时立即 `focus(window, cx)`。
  - 可复用：已有 `CellEditor::Input / DatePickerInput / DateTimePickerInput / TimePickerInput`，无需再为首字符手工拼值。
  - 需注意：只要表格层先进入编辑并把按键重放给当前焦点输入框，数据库结果页就能自动继承行为。

- **实现4**: `crates/redis_view/src/redis_cli_view.rs`、`crates/ui/src/input/otp_input.rs`
  - 模式：已有组件通过 `.on_key_down(cx.listener(...))` 接入键盘事件，并在需要时调用 `window.prevent_default()`、`cx.stop_propagation()`。
  - 可复用：表格层可沿用同样的监听方式接入 `KeyDownEvent`。
  - 需注意：如果不阻止传播，字符键可能继续冒泡到表格默认处理链。

### 2. 项目约定
- **命名约定**: 事件处理函数使用 `on_*` / `handle_*` 风格，内部辅助判断函数倾向小而直接。
- **文件组织**: 通用表格行为放在 `crates/one_ui/src/edit_table/state.rs`，业务层通过 delegate 注入差异。
- **导入顺序**: 维持现有 `std`、`super`、`crate`、外部 crate 的分组方式。
- **代码风格**: 优先复用既有状态流转方法，不在业务层做重复逻辑，不引入无关重构。

### 3. 可复用组件清单
- `crates/one_ui/src/edit_table/state.rs::current_cell_for_navigation`：获取当前活动单元格。
- `crates/one_ui/src/edit_table/state.rs::start_editing`：统一切换编辑态。
- `crates/one_ui/src/edit_table/delegate.rs::cell_edit_enabled`：判断当前表格是否允许编辑。
- `crates/db_view/src/table_data/results_delegate.rs::build_input`：构建并聚焦对应输入控件。
- `vendor/zed/crates/gpui/src/window.rs::dispatch_keystroke`：把首个按键重新派发到新焦点输入控件。

### 4. 测试策略
- **测试框架**: Rust `cargo test` / `cargo check`
- **测试模式**: 本次优先补纯逻辑单元测试或编译验证，避免引入重型 UI 交互测试。
- **参考实现**: `crates/one_ui/src/edit_table/selection.rs` 已有纯逻辑测试，可参考同文件内 helper 测试风格。
- **覆盖要求**: 正常字符输入、带控制修饰键的非编辑场景、控制字符过滤。

### 5. 依赖和集成点
- **外部依赖**: `gpui::KeyDownEvent`、`gpui::Keystroke`、`Window::dispatch_keystroke`
- **内部依赖**: `EditTableState` 与 `EditTableDelegate`、`CellEditor`
- **集成方式**: 在表格根容器 `inner_table` 上新增 `on_key_down`，命中后调用 `start_editing` + `dispatch_keystroke`
- **配置来源**: 是否允许编辑由 delegate 的 `cell_edit_enabled(...)` 决定

### 6. 技术选型理由
- **为什么用这个方案**: 直接在通用表格层接管字符键，数据库数据表和其他基于 `EditTableState` 的视图都能复用。
- **优势**: 不需要为不同编辑器类型手工注入首字符；可复用现有输入组件的 IME、日期解析和光标行为。
- **劣势和风险**: 需要谨慎过滤修饰键，避免误伤复制粘贴、快捷键和行号列。

### 7. 关键风险点
- **边界条件**: 行号列、未选中单元格、已经处于编辑态时不能重复进入编辑。
- **交互风险**: `Ctrl/Cmd` 类快捷键不能触发编辑；`Shift` 打出的可打印字符应保留。
- **性能瓶颈**: 仅新增轻量按键判断，无显著性能风险。
- **工具说明**: 仓库说明优先使用 `desktop-commander`、`context7`、`github.search_code`；当前会话未提供这些工具，因此改用本地代码检索和编译验证留痕。
