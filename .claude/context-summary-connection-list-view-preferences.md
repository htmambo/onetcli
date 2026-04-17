## 项目上下文摘要（connection-list-view-preferences）
生成时间：2026-03-26 12:11:31 +0800

### 1. 相似实现分析
- **实现1**: `main/src/home_tab.rs:2166`
  - 模式：首页工具栏统一承载新建、同步、搜索、刷新、筛选等交互
  - 可复用：`Button + dropdown_menu + PopupMenuItem`
  - 需注意：`HomePage` 内部回调直接改状态并 `cx.notify()`

- **实现2**: `crates/sftp_view/src/file_list_panel.rs:28`
  - 模式：本地状态维护 `SortColumn + SortOrder`，排序切换后立即重排
  - 可复用：排序枚举建模、升降序切换、`sort_by` 统一比较器
  - 需注意：比较器要有稳定的次级排序键，避免列表闪动

- **实现3**: `main/src/setting_tab.rs:183`
  - 模式：`AppSettings` 通过 `serde` 持久化到 `settings.json`
  - 可复用：新增偏好字段并使用 `#[serde(default)]` 保证老配置兼容
  - 需注意：运行时更新后要立即 `save()`，避免只停留在内存态

### 2. 项目约定
- **命名约定**: 偏好类枚举采用 `ConnectionListSortField`、`ConnectionListSortOrder`、`ConnectionListViewMode`
- **文件组织**: 持久化模型放 `main/src/setting_tab.rs`，首页交互和渲染放 `main/src/home_tab.rs`
- **导入顺序**: 标准库、外部 crate、本地模块分组导入
- **代码风格**: 视图层继续使用链式 UI 构建；复杂条件尽量抽成辅助函数

### 3. 可复用组件清单
- `main/src/home_tab.rs`: 主页工具栏、工作区分组、连接卡片渲染
- `crates/sftp_view/src/file_list_panel.rs`: 排序字段和方向切换模式
- `main/src/setting_tab.rs`: 全局设置加载、保存、默认值

### 4. 测试策略
- **测试框架**: Rust 单元测试
- **测试模式**: 为排序比较器补纯逻辑测试
- **参考文件**: `main/src/home_tab.rs` 现有 `summarize_sync_result` 测试
- **覆盖要求**: 名称升序、更新时间降序、创建时间升序

### 5. 依赖和集成点
- **外部依赖**: `chrono` 通过 workspace 复用，用于时间显示格式化
- **内部依赖**: `AppSettings`、`HomePage`、`StoredConnection`
- **集成方式**: 首页工具栏更新设置，工作区分组内部统一按当前偏好排序
- **配置来源**: `settings.json`

### 6. 技术选型理由
- **为什么用这个方案**: 排序/视图偏好属于纯客户端展示偏好，放在 `AppSettings` 最符合现有结构
- **优势**: 不改数据库 schema，不影响同步，不引入额外状态容器
- **劣势和风险**: 仅做本地偏好持久化，当前不会跨设备同步

### 7. 关键风险点
- **并发问题**: 无并发写热点，主要是 UI 回调更新全局设置
- **边界条件**: 旧配置文件缺少新字段；连接时间戳为空；同名连接排序稳定性
- **性能瓶颈**: 排序在当前页内存列表上完成，数据量有限，可接受
- **验证限制**: 目前仅做编译与排序单测，未做 GUI 自动化截图级校验
