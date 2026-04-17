## 项目上下文摘要（popup-window-style）
生成时间：2026-03-26 03:04:38 +0800

### 1. 相似实现分析
- **实现1**: `crates/core/src/popup_window.rs`
  - 模式：独立弹出窗口统一入口，负责窗口参数、装饰和根视图包装
  - 可复用：所有连接表单和凭证管理都走这里，适合统一补窗口壳体样式
  - 需注意：不能破坏独立窗口关闭逻辑和焦点陷阱

- **实现2**: `crates/db_view/src/connection_form_window.rs`
  - 模式：窗口内容内部自己绘制标题栏、主体卡片和底部操作区
  - 可复用：标题栏和底部操作区已有清晰分层，无需逐窗重做
  - 需注意：这类表单已经有自定义头尾，只需要通用窗口边框增强

- **实现3**: `crates/core/src/certificate_manager.rs`
  - 模式：凭证管理与编辑窗口走 `PopupWindow`，但内部缺少明确的页头/页脚结构
  - 可复用：现有表单和按钮逻辑可保留，只补视觉分层
  - 需注意：删除确认弹窗仍是 `Dialog`，不要混淆两条路径

### 2. 项目约定
- **命名约定**: 弹出窗口统一走 `open_popup_window`，页内模态走 `open_dialog`
- **文件组织**: 通用窗口壳体在 `crates/core/src/popup_window.rs`，业务内容在各自 view 中
- **导入顺序**: 先 `gpui`，再 `gpui_component`，最后 crate 内模块
- **代码风格**: 优先在通用层收口，再对明显缺头尾结构的业务视图做最小补强

### 3. 可复用组件清单
- `crates/core/src/popup_window.rs`: 独立窗口统一入口
- `crates/ui/src/app_style.rs`: `page_header_style / footer_style / border_strong / panel_bg`
- `crates/db_view/src/connection_form_window.rs`: 弹出窗口内容层的结构参考
- `crates/ui/src/dialog.rs`: 已完成的页内模态分层策略，可借鉴但不能直接照搬

### 4. 测试策略
- **测试框架**: 本轮以 Rust 编译验证为主
- **验证模式**: `cargo fmt --all` + 多包 `cargo check`
- **参考范围**: `one-core`、`gpui-component`、`main`、各连接窗口相关 crate
- **覆盖要求**: 确保 `PopupWindow` 通用层和凭证管理窗口渲染都能通过编译

### 5. 依赖和集成点
- **外部依赖**: `gpui` 窗口系统和组件 builder
- **内部依赖**: `PopupWindowView` 依赖 `Root`；凭证管理依赖 `CertificateRepository`
- **集成方式**: 连接表单与凭证管理通过 `open_popup_window(...)` 打开独立窗口
- **配置来源**: 颜色和边框都来自 `app_style` 当前主题快照

### 6. 技术选型理由
- **为什么用这个方案**: 连接表单和凭证管理共用 `PopupWindow`，先改通用壳体最划算
- **优势**: 不需要把大表单强行迁回 `Dialog`，保持现有窗口行为
- **劣势和风险**: 凭证管理窗口内部本身缺页头/页脚，需要额外补一层业务视图结构

### 7. 关键风险点
- **交互风险**: 不能破坏弹出窗口 Esc 关闭和焦点陷阱
- **边界条件**: 已有自定义标题栏的连接窗口不能出现双重页头
- **性能瓶颈**: 仅布局和样式变更，无明显性能风险
- **主题风险**: 必须继续沿用 `app_style`，避免再次破坏亮暗主题切换
