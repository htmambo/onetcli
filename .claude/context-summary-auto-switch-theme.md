## 项目上下文摘要（auto-switch-theme）
生成时间：2026-03-28 02:35:25 +0800

### 1. 相似实现分析
- **实现1**: [main/src/setting_tab.rs](/usr/htdocs/onetcli/main/src/setting_tab.rs)
  - 模式：设置项通过 `SettingField::*` 直接读写 `AppSettings`
  - 可复用：`AppSettings::apply(...)`、设置页里主题相关开关的保存逻辑
  - 需注意：`auto_switch_theme` 当前只写盘，不参与主题计算

- **实现2**: [crates/ui/src/theme/mod.rs](/usr/htdocs/onetcli/crates/ui/src/theme/mod.rs)
  - 模式：主题切换统一走 `Theme::change(...)`，系统外观同步走 `Theme::sync_system_appearance(...)`
  - 可复用：`WindowAppearance -> ThemeMode` 的转换规则
  - 需注意：`Theme::change(..., None, ...)` 不会主动刷新所有窗口

- **实现3**: [vendor/zed/crates/gpui/src/window.rs](/usr/htdocs/onetcli/vendor/zed/crates/gpui/src/window.rs)
  - 模式：窗口支持 `observe_window_appearance(...)` 监听系统外观变化
  - 可复用：主窗口上注册外观变化回调
  - 需注意：现有业务代码里没有任何地方使用这个监听

- **实现4**: [main/src/onetcli_app.rs](/usr/htdocs/onetcli/main/src/onetcli_app.rs)
  - 模式：主窗口初始化集中在 `OnetCliApp::new(...)`
  - 可复用：在主窗口生命周期里挂接外观观察者
  - 需注意：这里最适合作为“系统主题变化 -> 重新应用 AppSettings”入口

### 2. 项目约定
- **命名约定**：主题相关布尔判断使用 `auto_*` / `effective_*` / `resolve_*`
- **文件组织**：设置持久化在 `main/src/setting_tab.rs`，主题实现集中在 `crates/ui/src/theme/mod.rs`
- **代码风格**：优先提取纯函数做判定，方便补单元测试；副作用逻辑集中在少量 helper

### 3. 可复用组件清单
- [main/src/setting_tab.rs](/usr/htdocs/onetcli/main/src/setting_tab.rs): `AppSettings`、设置页读写入口
- [crates/ui/src/theme/mod.rs](/usr/htdocs/onetcli/crates/ui/src/theme/mod.rs): `Theme::change`、`ThemeMode`
- [vendor/zed/crates/gpui/src/window.rs](/usr/htdocs/onetcli/vendor/zed/crates/gpui/src/window.rs): `observe_window_appearance`

### 4. 测试策略
- **测试框架**：Rust 单元测试 + `cargo check`
- **测试模式**：为“自动切换主题的有效模式计算”补纯函数测试；编译验证主流程
- **覆盖要求**：
  - 自动切换关闭时沿用手动主题
  - 自动切换开启时跟随系统外观
  - 系统亮暗切换时有实际监听入口

### 5. 根因判断
- `auto_switch_theme` 当前只在设置页中保存到 `AppSettings`，并未参与 `AppSettings::apply(...)` 的主题模式选择。
- 设置页勾选/取消 `auto_switch_theme` 后，没有任何立即调用 `Theme::change(...)` 的逻辑。
- 主窗口也没有注册 `observe_window_appearance(...)`，所以系统主题变化不会驱动应用重新计算主题。
- 进一步现场核实发现：
  - `gdbus call ... org.freedesktop.portal.Settings.Read org.freedesktop.appearance color-scheme`
    返回 `org.freedesktop.portal.Error.NotFound`
  - 说明当前 Deepin/X11 会话没有通过 `xdg-desktop-portal` 暴露标准亮暗模式键
  - 但 `gsettings` 可读到 `com.deepin.xsettings theme-name='deepin'`，且系统主题目录中同时存在 `deepin` / `deepin-dark`
  - 因此 Deepin 需要走 `gsettings theme-name` 回退，而不能只依赖 `cx.window_appearance()`

### 6. 技术选型理由
- **为什么修在 `AppSettings` + 主窗口监听**：
  - 主题偏好和自动切换本质是“设置驱动的全局状态”，应由 `AppSettings` 决定有效主题
  - 系统外观变化是窗口事件，最合适的接入点是主窗口初始化
- **为什么增加 Deepin gsettings 回退**：
  - 当前会话的标准 portal 接口缺失，`gpui` 默认系统外观值会退回到 `Light`
  - Deepin 自己的主题名已经能区分 `deepin` / `deepin-dark`，足够支撑亮暗判断
- **优势**：
  - 代码集中，能同时修复“切换无效”和“系统变化不跟随”
  - 可以补纯函数单测，降低回归风险
- **风险**：
  - 如果未来有多主窗口场景，可能需要为新窗口重复注册外观监听
  - Deepin 主题变化目前通过“窗口外观事件 + 窗口重新激活时回读”同步，若系统在应用持续前台时静默改主题，可能不会瞬时刷新
