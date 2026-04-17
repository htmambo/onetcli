## 项目上下文摘要（主应用窗口 spotlight 效果）
生成时间：2026-03-26 00:32:13 +0800

### 1. 相似实现分析
- **实现1**: `crates/ui/src/window_border.rs:94`
  - 模式：`canvas(...)` + `window.mouse_position()` + `on_mouse_move(...)`
  - 可复用：主应用窗口内可按鼠标位置做自绘叠层
  - 需注意：自绘区域要避免破坏现有交互命中

- **实现2**: `crates/ui/src/color_picker.rs:395`
  - 模式：普通组件上直接接 `hover(...)` 和 `on_mouse_move(...)`
  - 可复用：卡片 hover 态和鼠标移动驱动状态更新
  - 需注意：高频鼠标事件只能做轻量状态更新

- **实现3**: `main/src/home_tab.rs:2920`
  - 模式：卡片容器使用边框、阴影、左侧强调线和 group hover
  - 可复用：左侧强调线、hover 边框提亮、卡片级视觉反馈
  - 需注意：要沿用现有卡片尺寸、圆角和边框表达

- **实现4**: `main/src/sync_server_theme.rs:1`
  - 模式：`sync_server` 已有独立深色绿主题 token
  - 可复用：背景、边框、强调色、危险态色板
  - 需注意：危险态不能复用绿色 spotlight

### 2. 项目约定
- **命名约定**: 主应用新增 UI 元素使用 PascalCase 组件名，主题辅助函数使用 snake_case
- **文件组织**: 通用 GPUI 组件放 `crates/ui/src`，业务主题包装放 `main/src/sync_server_theme.rs`
- **代码风格**: 保持链式 GPUI builder 写法，不额外引入状态管理框架

### 3. 可复用组件清单
- `crates/ui/src/window_border.rs`：鼠标位置与自绘能力参考
- `crates/ui/src/color_picker.rs`：hover + 鼠标移动状态更新模式
- `main/src/home_tab.rs`：主应用卡片 hover 和强调线模式
- `main/src/sync_server_theme.rs`：主应用 sync_server 深色主题 token

### 4. 测试策略
- **验证方式**: `cargo check -p gpui-component` 与 `cargo check -p main`
- **手工验证点**:
  - 设置页账户区域卡片 hover 是否出现左侧强调线和局部高光
  - 登录/注册对话框头部卡片和错误块 hover 是否符合预期

### 5. 依赖和集成点
- **内部依赖**: `main` 依赖 `gpui-component`
- **集成方式**: 在 `crates/ui` 提供通用 `SpotlightCard`，由 `main/src/sync_server_theme.rs` 提供预设包装

### 6. 技术选型理由
- **选择**: 使用 `canvas + PaintQuad + on_mouse_move` 近似实现 spotlight
- **原因**: GPUI 当前直接暴露线性渐变和自绘能力，但未见现成径向渐变 API
- **取舍**: 用多层圆形半透明叠层近似 Web 聚光，不追求 1:1 DOM 效果

### 7. 关键风险点
- 高频鼠标移动会触发重绘，需避免复杂路径绘制
- 自绘叠层必须放在卡片内部并配合 `overflow_hidden`
- danger 卡片必须使用红色强调，不能混入绿色 hover 语义
