## 项目上下文摘要（应用窗口边框预览）
生成时间：2026-03-26 20:26:43 +0800

### 1. 相似实现分析
- **实现1**: `crates/ui/src/window_border.rs:20`
  - 模式：窗口边框与阴影统一封装在 `WindowBorder`，同时处理 Linux 客户端边框、阴影和拖拽缩放热区。
  - 可复用：`window_border()`、`window.set_client_inset(...)`、`cx.theme().window_border`。
  - 需注意：Deepin / DDE 的系统控件优先路径会弱化自绘边框。

- **实现2**: `crates/ui/src/root.rs:447`
  - 模式：所有主窗口与弹窗内容都会经过 `Root::render(...)`，并统一包裹 `window_border()`。
  - 可复用：在窗口根层统一加样式，而不是逐页加边框。
  - 需注意：改这里会影响所有使用 `Root` 的窗口。

- **实现3**: `main/src/main.rs:65`
  - 模式：主窗口在 Linux 下启用 `WindowDecorations::Client`，并根据桌面环境选择透明或不透明背景。
  - 可复用：主窗口本来就支持客户端装饰路径。
  - 需注意：是否真的显示自绘外框，取决于窗口装饰策略和桌面环境。

- **实现4**: `crates/core/src/popup_window.rs:163`
  - 模式：弹窗与主窗口一样复用 `Root` 和 `WindowDecorations::Client`。
  - 可复用：只改 `window_border.rs` 就能同时影响弹窗。
  - 需注意：需要避免 Linux 自绘外框路径和新增预览边框叠加得过重。

- **实现5**: `crates/ui/src/title_bar.rs:21`
  - 模式：Deepin / DDE 会被识别为“优先系统窗口控件”的桌面环境。
  - 可复用：用该判断区分“系统装饰路径”和“纯客户端边框路径”。
  - 需注意：在这条路径下，直接改外框不稳定，根内容内边框更稳。

### 2. 项目约定
- **命名约定**: 窗口层公共能力放在 `crates/ui/src/*`。
- **文件组织**: 窗口装饰在 `window_border.rs`，窗口根容器在 `root.rs`，平台判断在 `title_bar.rs`。
- **代码风格**: 优先在公共基础层做最小改动，避免在主窗口和弹窗各写一份。

### 3. 可复用组件清单
- `window_border()`
- `WindowBorder::render(...)`
- `Root::render(...)`
- `linux_prefers_system_window_controls()`
- `cx.theme().window_border`

### 4. 测试策略
- **验证方式**: `cargo fmt --all` + `cargo check -p gpui-component -p main`
- **人工验证建议**: 启动主窗口和任意弹窗，观察窗口内容边缘是否出现一圈可见边框

### 5. 依赖和集成点
- **内部依赖**: `window_border.rs`、`root.rs`、`title_bar.rs`
- **集成方式**: 在 `window_border.rs` 的系统装饰路径上补一层内容内边框，不改页面层

### 6. 技术选型理由
- **为什么用这个方案**: 用户只是先看效果，最稳的是补“视觉内边框”，而不是重写系统外框行为。
- **优势**: 跨主窗口和弹窗统一生效，对 Deepin 更稳。
- **风险**: 这是视觉边框，不是操作系统级真实外框；如果用户想要更强烈效果，后续还要继续调颜色或宽度。
