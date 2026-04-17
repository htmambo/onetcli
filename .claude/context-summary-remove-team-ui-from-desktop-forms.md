## 项目上下文摘要（remove-team-ui-from-desktop-forms）
生成时间：2026-03-25 13:45:14 +0800

### 1. 相似实现分析
- **实现1**: `crates/db_view/src/common/db_connection_form.rs`
  - 模式：连接表单状态集中在窗口实体中管理，保存时统一构建 `StoredConnection`
  - 可复用：工作区选择、云同步开关、保存时写入 `owner_id`
  - 需注意：编辑态和新建态共用同一保存链路，若只删 UI 不改保存逻辑，旧字段仍可能继续透传

- **实现2**: `crates/terminal_view/src/ssh_form_window.rs`
  - 模式：窗口配置只传 UI 所需数据，保存时对 `StoredConnection` 做最终归一
  - 可复用：移除团队选择后统一 `conn.team_id = None`
  - 需注意：不改底层模型，避免扩大到同步与存储层

- **实现3**: `crates/redis_view/src/redis_form_window.rs`
  - 模式：选择组件和表单项是强绑定的，删 UI 时必须同时删状态字段和构造逻辑
  - 可复用：删除 `teams` 配置透传、`TeamSelectItem`、`team_select` 与对应渲染
  - 需注意：如果 `home_tab` 还继续传 `teams`，会导致编译失败

- **实现4**: `main/src/home_tab.rs`
  - 模式：各连接窗口由主页统一创建配置；连接列表徽标也由这里渲染
  - 可复用：统一去掉各表单配置中的 `teams` 透传，并删除连接卡片“团队”徽标
  - 需注意：这是桌面端团队入口的总汇点，必须同步清理

### 2. 项目约定
- **命名约定**：窗口配置使用 `*FormWindowConfig`，窗口主体使用 `*FormWindow`
- **文件组织**：主页只负责打开窗口和列表展示，具体表单状态放在各自 crate 内
- **代码风格**：优先小范围删除无用状态和渲染，不对底层同步模型做顺手重构

### 3. 可复用组件清单
- `main/src/home_tab.rs`：桌面端连接窗口打开入口与连接卡片展示
- `crates/db_view/src/common/db_connection_form.rs`：数据库连接表单统一保存模式
- `crates/terminal_view/src/ssh_form_window.rs`：已完成的“无团队 UI”参考实现
- `crates/redis_view/src/redis_form_window.rs`：已完成的“无团队 UI”参考实现
- `crates/core/src/certificate_manager.rs`：凭证管理弹窗与编辑器窗口

### 4. 测试策略
- **验证方式**：本地 `cargo fmt --all` 与 `cargo check -p main`
- **覆盖重点**：窗口配置字段删减后仍能通过编译；保存链路统一写 `team_id = None`
- **补充建议**：手动打开数据库/SSH/Redis/Mongo/串口/凭证编辑窗口，确认不再出现团队选择项

### 5. 依赖和集成点
- **内部依赖**：`StoredConnection`、`Certificate`、`GlobalCloudUser`
- **集成方式**：主页创建窗口配置，各表单构建并保存本地实体
- **边界说明**：本轮不删除底层 `team_id/owner_id` 字段，不改 `cloud_sync` 的团队实现

### 6. 技术选型理由
- **为什么只移除桌面端 UI**：用户要求针对应用窗口内容处理；底层团队字段仍可能被其他入口或历史数据依赖
- **优势**：改动面小，能立即消除用户可见的团队概念，并避免旧 `team_id` 继续触发不支持的同步路径
- **风险**：历史数据中的团队字段不会被迁移清理，需要依赖后续保存覆盖或单独迁移

### 7. 关键风险点
- **边界条件**：编辑历史连接或凭证时，必须在保存时显式清空 `team_id`
- **集成风险**：主页若继续透传 `teams` 会导致结构体字段不匹配
- **验证缺口**：目前没有 GUI 自动化，只能靠本地手动点开窗口做最终确认
