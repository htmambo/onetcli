## 项目上下文摘要（统一子窗口 Dialog 化与尺寸兜底）
生成时间：2026-03-29 06:42:47

### 1. 相似实现分析
- **实现1**: [popup_window.rs](/D:/usr/htdocs/onetcli/crates/core/src/popup_window.rs)
  - 模式：统一封装所有独立 popup 系统窗口
  - 可复用：`PopupWindowOptions`、`open_popup_window(...)`、`open_popup_window_with_should_close(...)`
  - 需注意：此前默认 `kind` 是普通窗口，且初始定位只按屏幕可见区域居中

- **实现2**: [connection_restore.rs](/D:/usr/htdocs/onetcli/main/src/connection_restore.rs)
  - 模式：启动阶段恢复连接列表使用统一 popup 入口打开独立窗口
  - 可复用：恢复窗口尺寸计算 `compute_connection_restore_popup_layout(...)`
  - 需注意：这是最先暴露“普通窗口不排它”的业务路径

- **实现3**: [setting_tab.rs](/D:/usr/htdocs/onetcli/main/src/setting_tab.rs)
  - 模式：主窗口恢复时先检查是否越界，再裁剪尺寸并居中
  - 可复用：`centered_bounds_in_visible_area(...)` 的“先裁剪后居中”思路
  - 需注意：这里处理的是主窗口恢复，目标区域是显示器可见范围

- **实现4**: [window.rs](/D:/usr/htdocs/onetcli/vendor/zed/crates/gpui/src/platform/windows/window.rs)
  - 模式：底层 `WindowKind::Dialog` 在 Windows 上会抓取当前活跃父窗口并禁用它
  - 可复用：无需业务层额外实现排它逻辑
  - 需注意：父子关系建立依赖打开时的父窗口上下文

### 2. 项目约定
- **命名约定**: 工具函数使用 `snake_case`，配置对象使用 `Options`/`Config` 后缀
- **文件组织**: 共用窗口能力放在 `crates/core`，业务调用点分散在 `main`、`db_view`、`terminal_view`、`redis_view`、`mongodb_view`
- **代码风格**: Rustfmt 默认格式；链式 UI 构建保持逐层缩进
- **交互约定**: 业务内嵌确认弹层优先用 `open_dialog(...)`，独立系统窗统一走 `open_popup_window(...)`

### 3. 可复用组件清单
- `crates/core/src/popup_window.rs`: 独立子窗口统一入口
- `main/src/setting_tab.rs`: 越界窗口裁剪与居中思路
- `main/src/onetcli_app.rs`: 主窗口句柄初始化与应用级窗口上下文

### 4. 测试策略
- **测试框架**: Rust 内置单元测试 + `cargo test`
- **参考文件**: [connection_restore.rs](/D:/usr/htdocs/onetcli/main/src/connection_restore.rs:487)、[setting_tab.rs](/D:/usr/htdocs/onetcli/main/src/setting_tab.rs:1322)
- **本次测试重点**:
  - popup 默认窗口类型是否改为 `Dialog`
  - 子窗口尺寸是否会被裁剪到父窗口内容区以内
  - 恢复窗口既有尺寸策略是否仍然成立

### 5. 依赖和集成点
- **内部依赖**: `PopupWindowOptions` 被 `main`、`db_view`、`core`、`terminal_view`、`redis_view`、`mongodb_view` 多处直接调用
- **平台依赖**: `vendor/zed` 的 `gpui` 提供 `WindowKind::Dialog` 语义与窗口尺寸 API
- **关键集成点**:
  - `window.window_bounds()`：读取父窗口几何边界
  - `window.viewport_size()`：读取父窗口内容区尺寸
  - `window.resize(...)`：在子窗口越界时压回允许范围

### 6. 技术选型理由
- **为什么统一改成 Dialog**: 用户要求优先降低排查成本，直接复用底层模态语义比逐窗检查业务逻辑更稳
- **为什么在统一入口做尺寸兜底**: 所有独立子窗口都走同一封装，集中修复能覆盖现有全部调用点
- **为什么要显式传父窗口**: 比依赖“当前活跃窗口”更稳定，能确保尺寸裁剪和初始居中都基于明确父窗

### 7. 关键风险点
- **嵌套对话框**: 证书管理等路径会从子窗口继续打开二级子窗口，需要确保仍按父子链工作
- **尺寸语义差异**: `viewport_size` 是内容区尺寸，`window_bounds` 是整窗定位边界，使用时必须分开处理
- **回归风险**: 统一入口签名改变后，需要补全所有调用点，否则会出现编译失败
