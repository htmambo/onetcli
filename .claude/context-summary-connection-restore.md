## 项目上下文摘要（连接恢复提示）
生成时间：2026-03-27 23:55:53 +0800

### 1. 相似实现分析
- **实现1**: [main/src/onetcli_app.rs](/usr/htdocs/onetcli/main/src/onetcli_app.rs)
  - 模式：应用初始化时创建 `TabContainer`、注册全局实体，并在 `on_app_quit` 中做持久化收尾。
  - 可复用：`GlobalTabContainer`、`GlobalHomePage`、`cx.on_app_quit(...)`、`cx.defer(...)`。
  - 需注意：当前启动阶段会直接调用 `load_tabs(...)`，退出阶段会异步调用 `save_tab_state(...)`。

- **实现2**: [crates/core/src/tab_container.rs](/usr/htdocs/onetcli/crates/core/src/tab_container.rs)、[crates/core/src/tab_persistence.rs](/usr/htdocs/onetcli/crates/core/src/tab_persistence.rs)
  - 模式：`TabContent::dump()` 输出最小可序列化状态，`TabContainer::dump()` 汇总所有普通标签页，再由持久化模块写入配置目录。
  - 可复用：`TabContainerState`、`TabItemState`、`save_tab_state` / `load_tab_state` 的配置目录获取逻辑。
  - 需注意：现有 builder 注册链路未落地，不能直接依赖 `load_tabs(...)` 恢复连接页。

- **实现3**: [main/src/home/home_tabs.rs](/usr/htdocs/onetcli/main/src/home/home_tabs.rs)、[main/src/home/home_strategy.rs](/usr/htdocs/onetcli/main/src/home/home_strategy.rs)
  - 模式：首页统一负责打开各类连接页，数据库/Redis/Mongo 还会根据 `DatabaseOpenMode` 决定按单连接还是工作区页打开。
  - 可复用：`open_ssh_terminal`、`open_serial_terminal`、`open_sftp_view`、`add_item_to_tab`、`open_redis_tab`、`open_mongodb_tab`。
  - 需注意：恢复时如果继续走“读当前设置”的逻辑，会导致工作区页和单连接页类型漂移。

- **实现4**: [main/src/home_tab.rs](/usr/htdocs/onetcli/main/src/home_tab.rs)、[crates/ui/src/dialog.rs](/usr/htdocs/onetcli/crates/ui/src/dialog.rs)
  - 模式：首页已在 `render()` 中用 `window.defer(...)` 触发启动后弹窗，项目已有 `Dialog + Checkbox` 组合模式。
  - 可复用：认证过期/错误弹窗的时序处理、工作区筛选里的 `Checkbox` 列表交互。
  - 需注意：恢复提示应等连接和工作区异步加载完成后再弹出。

### 2. 项目约定
- **命名约定**: 模块名使用蛇形，类型名使用大驼峰，恢复类型/快照结构建议沿用 `*Restore*` / `*Snapshot*`。
- **文件组织**: 生命周期入口在 `main/src/onetcli_app.rs`，首页交互在 `main/src/home_tab.rs` 与 `main/src/home/`，通用状态结构放 `crates/core/src/`。
- **导入顺序**: 同一文件内先标准库，再第三方/框架，再项目内模块。
- **代码风格**: Rust 风格化交给 `cargo fmt`，闭包和 `window.defer(...)` 用法应与现有页面一致。

### 3. 可复用组件清单
- [crates/core/src/tab_persistence.rs](/usr/htdocs/onetcli/crates/core/src/tab_persistence.rs): 配置目录定位与 JSON 状态读写模式。
- [main/src/home/home_tabs.rs](/usr/htdocs/onetcli/main/src/home/home_tabs.rs): 各类连接页打开入口。
- [main/src/home/home_strategy.rs](/usr/htdocs/onetcli/main/src/home/home_strategy.rs): 按连接类型选择打开策略。
- [crates/ui/src/dialog.rs](/usr/htdocs/onetcli/crates/ui/src/dialog.rs): 恢复提示弹窗基础能力。
- [main/src/home_tab.rs](/usr/htdocs/onetcli/main/src/home_tab.rs): 首页启动后延迟弹窗模式、`Checkbox` 交互模式。

### 4. 测试策略
- **测试框架**: 以 Rust 单元测试和 crate 级 `cargo test` 为主。
- **测试模式**: 本次优先补快照模型与持久化逻辑的单元测试，再用 `cargo check` 覆盖 UI 编译链路。
- **参考文件**: [main/src/onetcli_app.rs](/usr/htdocs/onetcli/main/src/onetcli_app.rs) 现有窗口标题测试；[crates/core/src/storage/models.rs](/usr/htdocs/onetcli/crates/core/src/storage/models.rs) 现有存储模型测试。
- **覆盖要求**: 正常序列化/反序列化、无效快照过滤、恢复项去重/选择、编译通过。

### 5. 依赖和集成点
- **外部依赖**: `serde` / `serde_json` 持久化，`gpui` / `gpui_component` 做窗口与弹窗交互。
- **内部依赖**: `StoredConnection`、`Workspace`、`TabContainer`、`TabContent::dump()`、首页各类打开入口。
- **集成方式**: 退出时由 `OnetCliApp` 统一保存，启动后由 `HomePage` 在数据加载完成后拉起恢复提示，再回调到 `home_tabs.rs` 执行恢复。
- **配置来源**: 配置目录通过 `one_core::storage::get_config_dir()` 获取；数据库打开模式来自 `AppSettings`。

### 6. 技术选型理由
- **为什么用独立恢复快照**: 现有 tab 自动恢复链路缺少 builder 注册和连接页自定义 `dump()`，直接扩展会把需求放大成“完整 tab 框架恢复”。
- **优势**: 可以只保存最小恢复元数据；启动时支持先提示、再勾选、再按需恢复；与现有打开入口天然兼容。
- **劣势和风险**: 需要补一层恢复类型定义，并处理工作区页与单连接页的精确恢复。

### 7. 关键风险点
- **生命周期时序**: 连接与工作区是异步加载的，恢复提示不能过早弹出。
- **恢复粒度**: 同一连接可能有多个 SSH/SFTP 实例；数据库/Redis/Mongo 还区分单连接页和工作区页。
- **设置漂移**: 如果恢复继续读当前 `DatabaseOpenMode`，会把上次的工作区页恢复成单连接页。
- **验证不足**: 仓库对该链路现有测试较少，需要把验证重点放在快照模型和编译链路。
