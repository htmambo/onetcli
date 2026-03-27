# 连接恢复实施方案

**目标：** 退出时保存当前打开的连接页快照，并在下次启动时提示恢复且支持勾选恢复。

**架构：** 通过独立的连接恢复快照文件承接该能力，而不是直接依赖现有 tab 自动恢复链路。连接页只输出最小恢复元数据，启动后由首页弹窗过滤、展示和执行恢复。

**技术栈：** Rust、gpui、gpui_component、serde、现有 `TabContainer` / `StoredConnection` / `Workspace`

---

### Task 1: 新增恢复快照模型与持久化

**Files:**
- Create: `crates/core/src/connection_restore.rs`
- Modify: `crates/core/src/lib.rs`

**Step 1: 定义恢复类型和恢复项结构**
- 覆盖 SSH 终端、串口终端、SFTP、数据库单连接页、数据库工作区页、Redis 单连接页、Redis 工作区页、MongoDB 单连接页、MongoDB 工作区页。

**Step 2: 定义快照文件结构和 JSON 读写函数**
- 复用配置目录获取逻辑，文件名单独使用 `connection_restore_state.json`。

**Step 3: 补充单元测试**
- 覆盖空快照、读写往返、非法文件容错。

### Task 2: 为连接页输出最小恢复元数据

**Files:**
- Modify: `crates/terminal_view/src/view.rs`
- Modify: `crates/sftp_view/src/lib.rs`
- Modify: `crates/db_view/src/database_tab.rs`
- Modify: `crates/redis_view/src/redis_tab.rs`
- Modify: `crates/mongodb_view/src/mongo_tab.rs`

**Step 1: 为各类连接页实现 `dump()`**
- 只输出恢复所需字段，不输出内部复杂状态。

**Step 2: 对数据库页补足活动连接字段**
- 让工作区页也能输出恢复时优先使用的连接 ID。

**Step 3: 验证非连接页保持不参与恢复**
- `Home`、`Settings`、本地终端、AI 聊天等不进入恢复快照。

### Task 3: 接入退出保存

**Files:**
- Modify: `main/src/onetcli_app.rs`

**Step 1: 在退出时同时保存 tab 状态和连接恢复快照**
- 使用当前 `TabContainer::dump()` 结果构建快照。

**Step 2: 保持现有 tab 状态保存逻辑不退化**
- 原有 `save_tab_state(...)` 继续执行。

### Task 4: 首页接入恢复提示与选择 UI

**Files:**
- Create: `main/src/connection_restore.rs`
- Modify: `main/src/main.rs`
- Modify: `main/src/home_tab.rs`

**Step 1: 加载并解析恢复快照**
- 在首页数据就绪后过滤无效恢复项。

**Step 2: 实现恢复弹窗视图**
- 列表展示、勾选、全选、跳过、恢复所选。

**Step 3: 清理一次性快照**
- 用户恢复或跳过后删除快照，避免重复提示。

### Task 5: 实现按恢复类型精确恢复

**Files:**
- Modify: `main/src/home/home_tabs.rs`
- Modify: `main/src/home_tab.rs`

**Step 1: 抽出“按指定模式打开数据库/Redis/Mongo 标签页”的内部方法**
- 恢复逻辑不再依赖当前 `AppSettings.database_open_mode`。

**Step 2: 在首页实现批量恢复入口**
- 逐个恢复用户选中的项，兼容多 SSH / 多 SFTP 实例。

**Step 3: 处理无效活动连接退化逻辑**
- 工作区页活动连接失效时，回退到当前可用的第一个同类型连接。

### Task 6: 本地验证与审查

**Files:**
- Modify: `.claude/operations-log.md`
- Modify: `.claude/verification-report.md`

**Step 1: 运行 `cargo fmt --all`**

**Step 2: 运行恢复快照相关单元测试**

**Step 3: 运行 `cargo check -p main`**

**Step 4: 生成验证结论和风险说明**
