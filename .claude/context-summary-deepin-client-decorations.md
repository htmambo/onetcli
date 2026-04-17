## 项目上下文摘要（deepin-client-decorations）
生成时间：2026-03-25 02:05:00 +0800

### 1. 相似实现分析
- **实现1**: `/home/hoping/.cargo/git/checkouts/zed-a70e2ad075855582/8b5328c/crates/gpui/src/platform/linux/x11/client.rs`
  - 模式：X11 客户端启动时统一探测窗口管理器能力，再把结果存入 `X11ClientState`
  - 可复用：`check_gtk_frame_extents_supported(...)`
  - 需注意：当前客户端装饰能力只依赖 `compositor_present && _GTK_FRAME_EXTENTS`

- **实现2**: `/home/hoping/.cargo/git/checkouts/zed-a70e2ad075855582/8b5328c/crates/gpui/src/platform/linux/x11/window.rs`
  - 模式：具体窗口在 `request_decorations(...)` 里根据 `WindowDecorations` 写 X11 属性
  - 可复用：`_MOTIF_WM_HINTS` 的已有客户端/服务端切换路径
  - 需注意：目前没有写 Deepin 专有 `_DEEPIN_NO_TITLEBAR`

- **实现3**: [crates/ui/src/title_bar.rs](/usr/htdocs/onetcli/crates/ui/src/title_bar.rs)
  - 模式：标题栏是否显示应用自绘按钮由 `should_render_custom_window_controls(...)` 决定
  - 可复用：`window.window_decorations()` 的真实装饰状态
  - 需注意：当前 Deepin 被桌面环境名直接拦截，即使窗口已经是真客户端装饰也不显示应用按钮

### 2. 项目约定
- **命名约定**：平台能力探测函数使用 `check_*_supported`，布尔字段使用 `*_supported`
- **文件组织**：X11 能力探测在 `client.rs`，具体窗口属性写入在 `window.rs`，应用标题栏逻辑在 `crates/ui/src/title_bar.rs`
- **代码风格**：优先最小改动，不新增无关抽象；平台差异尽量封装在平台层

### 3. 可复用组件清单
- `/home/hoping/.cargo/git/checkouts/zed-a70e2ad075855582/8b5328c/crates/gpui/src/platform/linux/x11/client.rs`: `_NET_SUPPORTED` 能力探测路径
- `/home/hoping/.cargo/git/checkouts/zed-a70e2ad075855582/8b5328c/crates/gpui/src/platform/linux/x11/window.rs`: `_MOTIF_WM_HINTS` 切换逻辑
- [crates/ui/src/title_bar.rs](/usr/htdocs/onetcli/crates/ui/src/title_bar.rs): 应用自绘按钮渲染入口

### 4. 测试策略
- **测试框架**：`cargo test` / `cargo check` / `cargo build`
- **测试模式**：平台层最小单测 + 主项目编译验证
- **参考文件**：
  - `/home/hoping/.cargo/git/checkouts/zed-a70e2ad075855582/8b5328c/crates/gpui/src/platform/linux/x11/window.rs`
  - [crates/ui/src/title_bar.rs](/usr/htdocs/onetcli/crates/ui/src/title_bar.rs)
- **覆盖要求**：本地自动验证确认编译和已有单测不回退；Deepin GUI 行为仍需你实机验证

### 5. 依赖和集成点
- **外部依赖**：`gpui` X11 平台实现
- **内部依赖**：主程序 `WindowOptions.window_decorations = Client`，标题栏通过 `window.window_decorations()` 读取实际状态
- **集成方式**：平台层成功返回 `Decorations::Client` 后，应用层标题栏应自动显示自绘按钮

### 6. 技术选型理由
- **为什么用这个方案**：根因已定位到 Deepin 专有窗口装饰路径，不应再继续在应用层兜底系统标题栏按钮行为
- **优势**：改动集中，直接绕开系统标题栏“最大化/还原”主按钮问题
- **风险**：`_DEEPIN_NO_TITLEBAR` 的恢复语义仍需实机确认，因此服务端装饰分支也要显式写回 `0`

### 7. 关键风险点
- **窗口管理器兼容**：Deepin/KWin 只对支持该原子的会话生效，其它桌面不能误触
- **边界条件**：应用层如果仍按桌面环境名隐藏自绘按钮，会导致隐藏系统标题栏后没有窗口按钮
- **性能瓶颈**：无新增性能风险
