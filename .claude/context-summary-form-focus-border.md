## 项目上下文摘要（form-focus-border）
生成时间：2026-03-26 02:22:37 +0800

### 1. 相似实现分析
- **实现1**: `crates/ui/src/input/input.rs:375`
  - 模式：输入框默认边框和焦点边框都在通用组件内部处理
  - 可复用：`focus_bordered`、`refine_style`、`focused_border`
  - 需注意：原实现先设置焦点边框，再 `refine_style`，导致自定义边框色把焦点边框覆盖掉

- **实现2**: `crates/ui/src/select.rs:804`
  - 模式：下拉框先应用普通样式，再在最后根据焦点状态叠加 `focused_border`
  - 可复用：焦点边框的正确叠加顺序
  - 需注意：这正是“新建连接下拉框能变色”的原因

- **实现3**: `crates/ui/src/button/button.rs:608`
  - 模式：按钮本身可追踪键盘焦点，并在最后追加焦点 ring
  - 可复用：`outline` 按钮在聚焦时额外改边框色
  - 需注意：设置页的下拉字段不是 `Select`，而是 `outline Button + dropdown_menu`

### 2. 项目约定
- **命名约定**：焦点样式统一用 `focused_border` 和主题 `ring`
- **文件组织**：输入/下拉公共行为应优先修在 `crates/ui` 通用组件层
- **代码风格**：不要在业务页面逐个加焦点边框，优先修复组件渲染顺序

### 3. 可复用组件清单
- `crates/ui/src/input/input.rs`：所有 `Input::new(...)` 的统一入口
- `crates/ui/src/select.rs`：所有 `Select::new(...)` 的统一入口
- `crates/ui/src/button/button.rs`：设置页“按钮式下拉”的统一入口
- `crates/ui/src/setting/fields/dropdown.rs`：设置页下拉字段基于 `Button::outline()`

### 4. 测试策略
- **验证方式**：优先执行整包 `cargo check`，覆盖主应用与各视图 crate
- **重点范围**：`main/db_view/terminal_view/redis_view/mongodb_view/sftp_view/gpui-component`
- **覆盖要求**：通用输入框、通用下拉框、设置页按钮式下拉都不应被本次修改破坏

### 5. 依赖和集成点
- **外部依赖**：`gpui`
- **内部依赖**：`StyledExt::focused_border`、`FocusableExt::focus_ring`
- **集成方式**：通过修改通用组件渲染顺序和焦点态边框逻辑，让业务页面自动受益

### 6. 技术选型理由
- **为什么用这个方案**：问题根因在组件层，不在单个页面；修组件能一次覆盖全部输入框和绝大多数下拉
- **优势**：影响面完整，避免继续在 `ssh/redis/mongo/设置/账号` 各处散补
- **风险**：`outline Button` 焦点边框会一并影响设置类按钮式下拉和其他 outline 按钮，需要人工观感确认

### 7. 关键风险点
- **边界条件**：显式 `focus_bordered(false)` 的输入框不应被强行加焦点边框
- **行为风险**：按钮式下拉获得键盘焦点时，边框与外层 ring 同时存在，需要人工确认视觉是否过强
- **验证限制**：当前没有自动 GUI 截图测试，需要你本地 tab/点击验证一轮
