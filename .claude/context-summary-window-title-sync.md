## 项目上下文摘要（window-title-sync）
生成时间：2026-03-24 23:43:00 +0800

### 1. 相似实现分析
- **实现1**: `main/src/onetcli_app.rs:384-397`
  - 模式：主窗口只挂载 `TabContainer`，没有独立标题栏视图
  - 可复用：主窗口 `render` 生命周期是同步系统窗口标题的最佳入口
  - 需注意：任何标题同步都必须围绕 `tab_container` 当前状态完成

- **实现2**: `crates/core/src/tab_container.rs:1322-1343`
  - 模式：容器内部已有 `active_tab()`，但缺少“当前激活标题”统一读取接口
  - 可复用：`pinned_tab_active`、`active_tab()`、`pinned_tab` 已经足够推导当前标题
  - 需注意：首页固定标签和普通标签共存，不能只读 `active_tab()`

- **实现3**: `crates/core/src/popup_window.rs:120`
  - 模式：窗口创建后直接调用 `window.set_window_title(&title)`
  - 可复用：沿用同一个窗口标题 API，不需要接触底层平台实现
  - 需注意：主窗口标题不是一次性固定值，而要跟随标签切换动态更新

### 2. 项目约定
- **命名约定**: 布尔值和状态缓存使用 `current_*`、`window_title`、`build_*`
- **文件组织**: 窗口状态同步逻辑放在 `main/src/onetcli_app.rs`，通用标签状态读取放在 `crates/core/src/tab_container.rs`
- **导入顺序**: 常量和纯函数靠近文件顶部
- **代码风格**: 小范围辅助函数 + `render` 中基于缓存的最小更新

### 3. 可复用组件清单
- `crates/core/src/tab_container.rs::active_tab`
- `crates/core/src/tab_container.rs::pinned_tab_active`
- `gpui::Window::set_window_title`

### 4. 测试策略
- **测试框架**: Rust 内联单元测试
- **测试模式**: 对标题格式化纯函数做单测，对主工程做编译验证
- **参考文件**: `crates/core/src/config.rs`、`crates/ui/src/title_bar.rs`
- **覆盖要求**: 正常标题、空标题回退、主工程编译通过

### 5. 依赖和集成点
- **外部依赖**: `gpui::Window`
- **内部依赖**: `OnetCliApp -> TabContainer`
- **集成方式**: 主窗口 `render` 读取当前标签标题，变化时调用 `window.set_window_title`
- **配置来源**: 无新增配置

### 6. 技术选型理由
- **为什么用这个方案**: 这是 A 方案里唯一真正能利用系统标题栏空白区的低风险实现
- **优势**: 改动小、验证快、不触碰主窗口结构
- **劣势和风险**: 只能让系统标题栏显示当前标签名，不能把真实标签控件移入系统标题栏

### 7. 关键风险点
- **首页固定标签**: 若只看普通活动标签，会在切回首页时显示错误标题
- **渲染时同步**: 需要避免每次渲染都重复设置相同标题
- **验收边界**: 成功与否主要靠你实际观察视觉效果，而不是单纯编译结果
