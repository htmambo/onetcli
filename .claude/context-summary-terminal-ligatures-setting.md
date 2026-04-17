## 项目上下文摘要（首页终端连字设置）
生成时间：2026-03-26 21:16:53 CST

### 1. 相似实现分析
- **实现1**: `main/src/setting_tab.rs`
  - 模式：首页设置统一写入 `AppSettings`，终端相关项通过 `sync_terminal_settings_to_all(...)` 广播到所有终端实例。
  - 可复用：终端字体、字号、行间距的设置项模式。
  - 需注意：本次用户明确要求只在首页设置里添加，不放终端右侧设置面板。

- **实现2**: `main/src/home/home_tabs.rs`
  - 模式：`setup_terminal_view(...)` 在终端创建时应用全局设置；`apply_terminal_settings_to_all(...)` 在设置变更后批量下发。
  - 可复用：终端排版参数统一通过 `TerminalView::apply_terminal_settings(...)` 下发。
  - 需注意：所有终端实例必须同步，不能只影响新开的终端。

- **实现3**: `crates/terminal_view/src/view.rs`
  - 模式：终端当前主题负责字体、字号、行高；`render(...)` 里会先构造 `Font`，再计算 `cell_width`。
  - 可复用：终端排版相关参数都在 `TerminalView` 内部统一生效。
  - 需注意：连字不仅影响文本绘制，也会影响字宽测量，二者必须同步切换。

- **实现4**: `crates/terminal_view/src/terminal_element.rs`
  - 模式：真实绘制阶段通过 `FontVariants` 创建字体变体。
  - 可复用：这里已经统一设置字体特性，适合抽成可复用辅助函数。
  - 需注意：当前代码硬编码关闭 `calt`，这是终端不显示连字的直接原因。

### 2. 项目约定
- **命名约定**: 终端全局设置字段统一使用 `terminal_*` 前缀。
- **文件组织**: 设置模型在 `main/src/setting_tab.rs`，终端同步在 `main/src/home/home_tabs.rs`，终端绘制与排版在 `crates/terminal_view/src/*`。
- **代码风格**: 复用既有设置同步链路，不新增新的状态容器或配置文件。

### 3. 可复用组件清单
- `main/src/setting_tab.rs::sync_terminal_settings_to_all`
- `main/src/home/home_tabs.rs::setup_terminal_view`
- `main/src/home/home_tabs.rs::apply_terminal_settings_to_all`
- `crates/terminal_view/src/view.rs::apply_terminal_settings`
- `crates/terminal_view/src/terminal_element.rs::FontVariants`

### 4. 测试策略
- **验证方式**:
  - `cargo fmt --all`
  - `cargo check -p terminal_view -p main`
- **覆盖重点**:
  - 首页设置新增连字开关后可正常编译。
  - 修改开关后所有终端实例都能收到新值。
  - 字体特性与字宽测量使用同一布尔值，避免渲染与列宽不一致。

### 5. 依赖和集成点
- **外部依赖**: 无新增依赖。
- **内部依赖**:
  - `AppSettings` 负责持久化
  - `GlobalHomePage` 负责广播到所有终端
  - `TerminalView` 负责渲染与字宽测量
  - `TerminalElement` 负责实际文本绘制

### 6. 技术选型理由
- **采用方案**: 增加布尔型终端全局设置 `terminal_font_ligatures`，首页设置切换后统一同步到所有终端实例。
- **优势**: 改动面小，完全复用现有终端设置同步链路，同时把渲染和测量统一到同一开关上。
- **风险**: 若只改绘制不改测量，光标、选区和点击定位会错位；因此必须双路径同时切换。

### 7. 关键风险点
- **一致性风险**: 连字开启后，某些字体的符号组合可能改变视觉宽度，因此 `cell_width` 也要使用相同字体特性重新计算。
- **兼容性风险**: 默认值必须保持现状关闭，避免未主动开启的用户体验突变。
- **范围控制**: 用户明确要求不在终端右侧设置面板加入该开关，本次只修改首页设置与终端渲染链路。
