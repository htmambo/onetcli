## 项目上下文摘要（tab-bar-tab-pointer-cursor）
生成时间：2026-03-29 13:40:48 +0800

### 1. 相似实现分析
- **实现1**: `crates/core/src/tab_container.rs:478-490`
  - 模式：可点击列表项在根节点直接声明 `.cursor_pointer()`
  - 可复用：`TabListActionItem::render(...)`
  - 需注意：光标样式应该挂在实际命中元素上，而不是只挂内部文本

- **实现2**: `crates/core/src/tab_container.rs:582-590`
  - 模式：tab 列表项本体使用 `.cursor_pointer()`，关闭按钮再单独声明自己的 pointer
  - 可复用：`TabListItem::render(...)`
  - 需注意：父级 pointer 不会阻止子级继续覆盖为其他 cursor

- **实现3**: `crates/core/src/tab_container.rs:1910-1928`
  - 模式：固定 tab 本体已经带 `.cursor_pointer()`
  - 可复用：`pinned-tab` 的渲染链
  - 需注意：滚动 tab 与固定 tab 应保持交互一致

### 2. 当前实现位置
- 目标文件：`crates/core/src/tab_container.rs`
- 目标节点：滚动区域内每个 tab 的根 `div()`，位于 `self.tabs.iter().enumerate().map(...)` 渲染链中

### 3. 设计约束
- 不能影响现有 Windows 下的 `occlude()` 命中策略
- 不能影响非 Windows 下的 `block_mouse_except_scroll()` 行为
- 不能覆盖已激活 tab 的 `.cursor_grab()`，否则会破坏“可拖拽重排”的反馈

### 4. 实现决策
- 在滚动 tab 的根节点上补 `.cursor_pointer()`
- 保留后续 `when(allow_tab_drag && is_active, |el| el.cursor_grab()...)` 分支，让可拖拽激活 tab 继续显示 `grab`
- 不改 `pinned-tab`，因为它已经带有 `.cursor_pointer()`

### 5. 验证方式
- `cargo fmt --all`
- `cargo check -p one-core`

### 6. 风险说明
- `cargo check -p one-core` 会同时编译 `gpui-component`，因此仍会带出已知的 `crates/ui/src/window_ext.rs` 历史 warning
- 这组 warning 与本次 tab 光标样式修改无关
