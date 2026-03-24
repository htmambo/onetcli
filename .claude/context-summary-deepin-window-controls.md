## 项目上下文摘要（deepin-window-controls）
生成时间：2026-03-24 23:15:51 +0800

### 1. 相似实现分析
- **实现1**: `main/src/main.rs:53-66`
  - 模式：Linux 主窗口直接请求 `WindowDecorations::Client`
  - 可复用：主窗口平台分支和 `WindowOptions` 构造
  - 需注意：Linux 下没有使用 `titlebar` 选项，而是完全依赖窗口装饰和应用内部布局

- **实现2**: `main/src/onetcli_app.rs:281-307`
  - 模式：主标签容器在非 macOS 平台启用 `with_window_controls(true)`
  - 可复用：现有标签栏窗口按钮开关
  - 需注意：这里只区分平台，不区分当前窗口是否真的处于客户端装饰

- **实现3**: `crates/core/src/tab_container.rs:1522-1526`、`1935-2032`
  - 模式：标签栏内部根据 `show_window_controls` 渲染窗口三键，并在 Linux 上手动处理点击
  - 可复用：`render_window_controls` 和 `window.window_decorations()` 运行时信息
  - 需注意：当前只检查 `show_window_controls`，没有兼容桌面环境差异

- **实现4**: `crates/ui/src/title_bar.rs:255-314`
  - 模式：通用 `TitleBar` 组件总是追加 `WindowControls`
  - 可复用：统一标题栏按钮实现和 Linux 关闭回调
  - 需注意：弹窗、表单窗口同样会复用这里，问题影响面不只主窗口

### 2. 项目约定
- **命名约定**: 运行时布尔值使用 `show_*` / `is_*` / `should_*`
- **文件组织**: 通用窗口 UI 在 `crates/ui`，业务窗口组合逻辑在 `crates/core` 与 `main`
- **导入顺序**: 先外部 crate，再按模块分组
- **代码风格**: 链式 UI 构造 + 少量局部布尔变量控制 `.when(...)`

### 3. 可复用组件清单
- `crates/ui/src/title_bar.rs`: 通用标题栏和窗口控制按钮实现
- `crates/core/src/tab_container.rs`: 主工作区标签栏窗口按钮实现
- `main/src/main.rs`: Linux 主窗口装饰配置入口

### 4. 测试策略
- **测试框架**: Rust 内联单元测试
- **测试模式**: 对纯函数做单测，对 UI 变更做本地编译验证
- **参考文件**: `crates/ui/src/description_list.rs`、`crates/core/src/config.rs`
- **覆盖要求**: Deepin/DDE 命中、非 Deepin 不命中、主工程编译通过

### 5. 依赖和集成点
- **外部依赖**: `gpui`（窗口装饰和窗口操作），`gpui-component`
- **内部依赖**: `one-core` 依赖 `gpui-component`，因此适合把兼容判断放在 `crates/ui`
- **集成方式**: `Window::window_decorations()` + Linux 环境变量辅助判断
- **配置来源**: `XDG_CURRENT_DESKTOP`、`DESKTOP_SESSION`

### 6. 技术选型理由
- **为什么用这个方案**: 不继续硬删系统标题栏，而是只在适合的环境下显示应用自绘按钮，风险最小
- **优势**: 改动集中、影响面明确、对 Deepin 25 的重复按钮问题直接有效
- **劣势和风险**: 这是桌面环境兼容分支，不是对上游 X11 装饰行为的根治

### 7. 关键风险点
- **运行时判定偏差**: 若其它桌面环境也存在同样问题，后续可能要扩展兼容列表
- **行为差异**: 隐藏自绘按钮后，Linux 某些窗口只保留系统标题栏按钮
- **验证边界**: 当前无法在命令行内直接截图核验，只能靠编译和纯函数测试兜底
