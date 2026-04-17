## 项目上下文摘要（deepin-window-restore）
生成时间：2026-03-25 00:31:30 +0800

### 1. 相似实现分析
- **实现1**: [main/src/main.rs](/usr/htdocs/onetcli/main/src/main.rs)
  - 模式：主窗口统一在 `WindowOptions` 里定义装饰、背景和尺寸
  - 可复用：`window_background`、`window_decorations`
  - 需注意：Linux 下此前一直强制 `Transparent + Client`

- **实现2**: [crates/ui/src/window_border.rs](/usr/htdocs/onetcli/crates/ui/src/window_border.rs)
  - 模式：窗口根层统一包装自绘边框和阴影
  - 可复用：`window.set_client_inset(...)`
  - 需注意：此前无论系统装饰还是客户端装饰都会写入 `client inset`

- **实现3**: [crates/ui/src/title_bar.rs](/usr/htdocs/onetcli/crates/ui/src/title_bar.rs)
  - 模式：通过桌面环境判断是否渲染应用自绘窗口按钮
  - 可复用：`linux_prefers_system_window_controls()`
  - 需注意：Deepin/DDE 已走系统控件兼容分支

- **实现4**: `/home/hoping/.cargo/git/checkouts/zed-a70e2ad075855582/8b5328c/crates/gpui/src/platform/linux/x11/window.rs`
  - 模式：X11 平台窗口状态由 `gpui` 维护
  - 可复用：日志与状态判断逻辑
  - 需注意：本机启动日志明确出现 `x11: no compositor present, falling back to server-side window decorations`

- **实现5**: `/home/hoping/.cargo/git/checkouts/zed-a70e2ad075855582/8b5328c/crates/gpui/src/platform/linux/wayland/window.rs`
  - 模式：Wayland `zoom()` 按当前最大化状态显式执行 `set_maximized` / `unset_maximized`
  - 可复用：显式状态切换思路
  - 需注意：Wayland 没有使用“统一 Toggle”这一层抽象

- **实现6**: `/home/hoping/.cargo/git/checkouts/zed-a70e2ad075855582/8b5328c/crates/gpui/src/window.rs`
  - 模式：上层窗口抽象把“最大化/还原”统一映射到平台层 `zoom()`
  - 可复用：说明只需修平台层，不应继续在应用 UI 层兜底
  - 需注意：如果 X11 `zoom()` 语义错误，标签栏双击、自绘按钮和其他上层入口都会一起受影响

### 2. 项目约定
- **命名约定**：平台判断函数以 `linux_*` 命名，窗口辅助逻辑集中在 `ui` 层
- **文件组织**：窗口创建在 `main/src/main.rs`，窗口边框与标题栏兼容逻辑在 `crates/ui/src`
- **导入顺序**：先标准库/外部依赖，再本地模块；平台专用导入用 `#[cfg]`
- **代码风格**：优先最小改动，不拆分无关结构，不引入新的全局状态

### 3. 可复用组件清单
- [crates/ui/src/title_bar.rs](/usr/htdocs/onetcli/crates/ui/src/title_bar.rs): `linux_prefers_system_window_controls`
- [crates/ui/src/window_border.rs](/usr/htdocs/onetcli/crates/ui/src/window_border.rs): `WindowBorder`
- [main/src/main.rs](/usr/htdocs/onetcli/main/src/main.rs): 主窗口 `WindowOptions`

### 4. 测试策略
- **测试框架**：`cargo test`
- **测试模式**：编译检查 + 标题栏兼容单测 + 主窗口标题单测
- **参考文件**：
  - [main/src/onetcli_app.rs](/usr/htdocs/onetcli/main/src/onetcli_app.rs)
  - [crates/ui/src/title_bar.rs](/usr/htdocs/onetcli/crates/ui/src/title_bar.rs)
- **覆盖要求**：窗口行为仍需 Deepin 25 实机验证，本地自动化主要兜底编译与兼容分支

### 5. 依赖和集成点
- **外部依赖**：`gpui` X11 窗口实现
- **内部依赖**：`main` 的窗口创建依赖 `gpui-component` 的窗口兼容判断
- **集成方式**：窗口创建参数影响 `gpui` 平台窗口；`WindowBorder` 影响 X11 `_GTK_FRAME_EXTENTS`
- **配置来源**：`XDG_CURRENT_DESKTOP=Deepin`、`DESKTOP_SESSION=deepin`、`XDG_SESSION_TYPE=x11`

### 6. 技术选型理由
- **为什么用这个方案**：既然本机已经自动降级到系统装饰，就不应继续向窗口管理器声明客户端边框范围，也不应继续使用透明背景扩大歧义
- **优势**：改动集中，和现有“单组按钮 + A 方案标题同步”不冲突
- **劣势和风险**：无法在本地自动化复现 Deepin 的还原按钮行为，仍需你做 GUI 实测

### 6.1 新的根因判断
- **已确认的代码事实**：X11 `zoom()` 仍固定发送 `_NET_WM_STATE` 的 `Toggle`
- **对照实现证据**：Wayland `zoom()` 已按当前状态显式区分“最大化”和“还原”
- **当前假设**：Deepin 25 的主“还原”路径更依赖显式移除最大化状态；统一 `Toggle` 在其标题栏主按钮语义下兼容性不足
- **最小修复方向**：将 X11 `zoom()` 从固定 `Toggle` 改为“当前已最大化则 `Remove`，否则 `Add`”

### 6.2 新增现场结论
- **OnetCli 当前实测窗口**：`0x8c00002`
- **普通态属性**：
  - `_NET_WM_STATE` 为空
  - `WM_NORMAL_HINTS` 为 `user specified location: 320, 162`、`user specified size: 3200 by 1836`、`gravity: NorthWest`
  - `WM_HINTS` / `WM_CLIENT_LEADER` / `_MOTIF_WM_HINTS` 已存在
- **标准 X11 切换实测**：
  - 手工发送 `_NET_WM_STATE Add(MAXIMIZED_VERT, MAXIMIZED_HORZ)` 后，窗口成功变为 `3840x2080 +0+80`
  - 再手工发送 `_NET_WM_STATE Toggle(MAXIMIZED_VERT, MAXIMIZED_HORZ)` 后，窗口成功恢复为 `3200x1836 +322+242`
- **直接推论**：OnetCli 当前窗口的标准 EWMH 最大化/还原链路本身是通的；用户仍看到“系统主按钮和双击标题栏不能还原”，剩余问题更接近 Deepin/KWin `com.deepin.chameleon` 装饰插件路径，而不是应用窗口属性仍然缺失

### 7. 关键风险点
- **窗口管理器兼容**：Deepin 的最大化/还原主按钮行为依旧是核心不确定点
- **边界条件**：若某些 Deepin 环境实际启用了合成器，系统装饰与透明背景组合可能表现不同
- **性能瓶颈**：无新增性能风险
- **安全考虑**：本次不涉及安全逻辑
