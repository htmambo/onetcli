## 项目上下文摘要（chatdb-completion-menu-height）
生成时间：2026-03-26 02:42:16 +0800

### 1. 相似实现分析
- **实现1**: `crates/ui/src/input/popovers/completion_menu.rs`
  - 模式：输入补全面板，当前使用固定 `MAX_MENU_HEIGHT`
  - 可复用：当前位置计算、文档侧栏、补全列表状态管理
  - 需注意：只计算了宽度，没有计算窗口剩余高度，也不会在底部空间不足时向上展开

- **实现2**: `crates/ui/src/input/popovers/hover_popover.rs`
  - 模式：按窗口剩余空间动态计算 `max_h`
  - 可复用：`window.bounds().size.height - edge padding` 的高度约束思路
  - 需注意：它已经实现了“空间不够时换方向”的基本逻辑

- **实现3**: `crates/ui/src/menu/popup_menu.rs`
  - 模式：弹出菜单按窗口高度的一半限制最大高度
  - 可复用：面板超高时使用 `overflow_y_scroll`
  - 需注意：说明项目里其它弹层早就有高度约束，`completion_menu` 是遗漏点

- **实现4**: `crates/ui/src/input/popovers/code_action_menu.rs`
  - 模式：与 `completion_menu` 同类的输入弹层
  - 可复用：同步修复，避免留下同类问题
  - 需注意：同样是固定高度、固定向下展开

### 2. 项目约定
- **命名约定**：弹层布局统一使用 `max_h / snap_to_window / edge padding` 一类约束语义
- **文件组织**：输入相关浮层集中在 `crates/ui/src/input/popovers`
- **代码风格**：优先在通用弹层层修复，不在 ChatDb 业务层特殊判断窗口高度

### 3. 可复用组件清单
- `crates/ui/src/input/popovers/completion_menu.rs`
- `crates/ui/src/input/popovers/code_action_menu.rs`
- `crates/ui/src/input/popovers/hover_popover.rs`
- `crates/ui/src/menu/popup_menu.rs`

### 4. 测试策略
- **验证方式**：优先 `cargo check` 覆盖 UI、db_view 与主应用
- **覆盖要求**：ChatDb `@` 提示列表、通用补全面板、代码动作菜单都不应因布局变更失效
- **人工检查点**：窗口高度较小时，补全列表应限制高度并避免跑出窗口底部

### 5. 依赖和集成点
- **外部依赖**：`gpui`
- **内部依赖**：`InputState.last_layout`、`input_bounds`、`editor_popover`、`List::max_h`
- **集成方式**：在输入弹层自身根据窗口大小和光标位置计算布局

### 6. 技术选型理由
- **为什么用这个方案**：问题在通用补全面板，不在 ChatDb 业务组件；修通用层能同时覆盖其它补全场景
- **优势**：`@` 表名提示、LSP completion、代码动作菜单一起得到边界约束
- **风险**：向上展开的阈值是经验值，需要人工看一眼观感

### 7. 关键风险点
- **边界条件**：窗口极小或光标非常靠近顶部/底部时，面板高度可能只剩很小空间
- **行为风险**：垂直布局下文档面板与列表面板会共享同一高度上限，文档可能更早进入滚动
- **验证限制**：当前没有 GUI 截图测试，只能通过编译和人工交互确认
