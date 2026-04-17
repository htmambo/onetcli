## 项目上下文摘要（dialog-visual-structure）
生成时间：2026-03-26 02:57:42 +0800

### 1. 相似实现分析
- **实现1**: `crates/ui/src/dialog.rs`
  - 模式：通用 `Dialog` 承担所有简单弹窗的默认渲染与交互
  - 可复用：现有拖拽、焦点陷阱、遮罩、关闭逻辑都应保留
  - 需注意：不能修改 `Dialog` 对外 API，避免影响 `open_dialog / confirm / alert`

- **实现2**: `crates/db_view/src/connection_form_window.rs`
  - 模式：窗口级表单使用 `app_style::title_bar_style()` 处理标题栏，用 `border_t_1` 切分底部操作区
  - 可复用：标题栏背景、底部分隔线、按钮右对齐布局
  - 需注意：视觉层级清晰，但不能直接照搬窗口结构到弹窗

- **实现3**: `crates/redis_view/src/redis_form_window.rs`
  - 模式：内容区使用 `panel_bg + border` 的卡片式容器，整体与页面背景分离
  - 可复用：表单区与背景区分层、边框与圆角配合
  - 需注意：弹窗本身已经是浮层，外框只需增强，不应再叠过重容器

- **实现4**: `crates/terminal_view/src/ssh_form_window.rs`
  - 模式：与 Redis/连接窗口一致，标题与主体、底部操作区分离
  - 可复用：统一沿用 `app_style` 里的标题栏和边框语义
  - 需注意：保持主题切换兼容，不能写死颜色

### 2. 项目约定
- **命名约定**: 通用视觉语义集中在 `app_style::*`，局部变量采用 `has_* / *_padding / *_offset`
- **文件组织**: 通用弹窗逻辑放在 `crates/ui/src/dialog.rs`，业务窗口只消费组件
- **导入顺序**: 先标准库，再第三方，再 crate 内模块
- **代码风格**: 优先用 builder 链在同一层完成布局，不引入新的组件抽象

### 3. 可复用组件清单
- `crates/ui/src/app_style.rs`: `title_bar_style / footer_style / border / border_strong / panel_bg`
- `crates/ui/src/dialog.rs`: 既有拖拽、遮罩、动画、确认/取消逻辑
- `crates/db_view/src/connection_form_window.rs`: 底部操作区分隔线与右对齐按钮区

### 4. 测试策略
- **测试框架**: 当前任务以 Rust 编译校验为主
- **验证模式**: `cargo fmt --all` + 多包 `cargo check`
- **参考范围**: `gpui-component` 与主应用/数据库/终端相关包，确保通用弹窗修改不会破坏多处调用
- **覆盖要求**: 至少验证 builder 链、样式调用和多 crate 依赖编译通过

### 5. 依赖和集成点
- **外部依赖**: `gpui` 布局与样式 builder
- **内部依赖**: `Dialog` 依赖 `app_style`、`Button`、`Root`、`WindowExt`
- **集成方式**: 业务侧通过 `window.open_dialog(...)`、`.confirm()`、`.alert()` 自动复用默认渲染
- **配置来源**: 颜色与边框来自当前活动主题快照 `app_style`

### 6. 技术选型理由
- **为什么用这个方案**: 统一修改通用 `Dialog` 可以覆盖所有简单弹窗，避免逐页漏改
- **优势**: 改动面小、主题兼容性好、不会引入新的弹窗 API
- **劣势和风险**: 需要谨慎处理已有 padding/close button/拖拽区域，避免布局回归

### 7. 关键风险点
- **交互风险**: 标题栏改造后不能破坏拖拽、关闭按钮、键盘关闭行为
- **边界条件**: 无标题但有关闭按钮、无底部操作区、外部自定义 padding 的弹窗都要保持可用
- **性能瓶颈**: 仅是静态样式调整，无额外性能压力
- **主题风险**: 必须继续走 `app_style`，确保亮暗主题切换不被破坏
