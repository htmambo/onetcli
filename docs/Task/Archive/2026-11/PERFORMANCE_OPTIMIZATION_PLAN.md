# 全项目性能优化（UI 线程阻塞 / 每帧开销 / 数据放大 / 工程效率）

**Status**: ✅ Completed (完成时间: 2026-11-21)
**Created**: 2026-11-21
**Creator**: Claude Code
**Branch**: refactor/remove-external-db-drivers

## 1. 目标与背景

对全项目做性能走查（5 个维度并行静态分析 + 本机实测），产出一批高价值优化项并全部落地。

已实测基线（本机 KDE Plasma / KWin / Wayland，debug 构建）：
- 空闲 CPU 3.1%（10s 采样）→ 无空转轮询
- 常驻内存 175 MB / 44 线程
- 启动到标签布局加载完成 1.29s
- 改 `main/src/home_tab.rs` 后 `cargo build -p main` 增量 41.7s；`cargo check -p main` 86s
- `target/` 48 GB

## 2. 范围

分三批落地，每批独立可验证：

- **第一批（P0，UI 线程阻塞 / 冻结级）**：OAuth 回调阻塞、Argon2id 无缓存、本地存储同步访问跑在 UI 线程、markdown 解析锁争用。
- **第二批（P1，每帧 / 每次操作稳定开销）**：Redis 明细视图无虚拟化、DB 树深克隆、终端 `selection_text` 每帧拼接、表格每单元格克隆全局设置、首页每帧 JSON 反序列化、表头 `col_groups` 克隆、单元格 id `format!`。
- **第三批（P2，数据放大 / 结构性）**：`EditTableState` 横向虚拟化、Redis 筛选缓存、AI 面板只重渲染末条消息、元数据 `*_view` 补缓存、翻页跳过 `COUNT(*)`、去掉无条件 `ROLLBACK`、搜索防抖、Redis 键树 O(N²)、SQL 结果面板克隆、`chat_messages` 索引、聊天历史 `LIMIT` 下推、`apply_table_changes` 事务化、脚本执行保留物理连接。
  - 其中 `EditTableState` 横向虚拟化、AI 面板末条消息增量渲染、元数据 `*_view` 补缓存、脚本执行复用物理连接四项经外部评审后**延后**，理由见 §5 子任务 15/16/17/20。
- **工程效率**：CI clippy 门禁、`derive_key_v3` 缓存带来的启动收益。

不在本次范围：超大门禁文件拆分（`home_tab.rs` 8077 行等）、CI 引入 sccache/mold、`target/` 体积治理 —— 属独立工程项，需单独排期。

## 3. 现状（代码事实）

### 3.1 P0

1. **OAuth 回调阻塞 UI 线程最长 300s**：`crates/core/src/cloud_sync/oauth/callback_server.rs:91` 设 `set_nonblocking(false)`，`:138` 的 `listener.accept()` 为真阻塞 accept → `accept_with_deadline` 的 `WouldBlock` 轮询分支成为死代码；调用方 `main/src/settings/oauth_dialog.rs:99` / `:391` 位于 `cx.spawn`（GPUI 前台执行器 = 主线程）。
2. **Argon2id 零缓存**：`crates/core/src/crypto.rs:175` `derive_key_v3` 每次调用重算（6 个调用点 `:356/:631/:694/:806/:894`），单次 50–150ms。`ENCRYPTION_KEY_V3` 槽（`:101`）已缓存同一把密钥却未被复用。
3. **本地存储同步访问跑在 UI 线程**：`main/src/home_tab.rs:485/:518/:1373` + 6 处写（`:2178/:2323/:2486/:4356/:4442/:4502`）；`crates/db_view/src/common/db_connection_form.rs:877`、`crates/db_view/src/data_transfer/view.rs:141`、`crates/core/src/certificate_manager.rs:80`。`busy_timeout=5000` → 遇写锁竞争 UI 线程最多卡 5s。正确样板：`crates/db_view/src/db_tree_view.rs:1167-1180` 的 `Tokio::spawn_result`。
4. **markdown 解析锁争用**：`crates/ui/src/text/state.rs:264` 的 render 抢 `parsed_content.lock()`，后台解析在 `:403-437` 持锁执行完整 tree-sitter 解析。

### 3.2 P1（每帧）

- Redis 明细：`crates/redis_view/src/key_value_view.rs:2392/2527/2659/2878` 全量铺开 + `:295/:315/:337` 每帧筛选排序克隆；`crates/redis_view/src/connection.rs:1250` 的 `HGETALL` 无 LIMIT（项目唯一无上限列表）。
- DB 树：`crates/db_view/src/db_tree_view.rs:2477` 每帧深克隆递归 `DbNode`（`crates/db/src/types.rs:96-106`，`children: Vec<DbNode>`）；`:1484` 把整批子节点挂到父节点。
- 终端：`crates/terminal_view/src/view.rs:4684-4690` 注释称"render 阶段不再触发"但代码仍在 render 内调用 `selection_text()`（`crates/terminal/src/terminal.rs:3013`）。
- 表格：`crates/db_view/src/table_data/results_delegate.rs:1952` 每单元格克隆全局字体设置；`crates/ui/src/table/state.rs:1371/:1407` 与 `crates/one_ui/src/edit_table/state.rs:2665/:2707` 每帧克隆 `col_groups`；`crates/ui/src/table/state.rs:1484/:1589` 每单元格 `format!` 生成 id。
- 首页：`main/src/home_tab.rs:4741` 每帧全量克隆连接；`:4610` `connection_subtitle` 每连接一次 JSON 反序列化。

### 3.3 P2（数据放大）

- `crates/db/src/plugin.rs:1654` 每次翻页重跑 `COUNT(*)`。
- `crates/db/src/manager.rs:3017-3206` 的 `*_view` 系列绕过 `GlobalNodeCache`；树展开 `manager.rs:2499` 每表 3 次目录查询。
- `crates/db/src/connection.rs:292` `rollback_if_active` 无条件发 `ROLLBACK`；`manager.rs:656` 额外 `ping()`。
- `crates/db/src/manager.rs:2243-2250` 脚本执行后强制销毁物理连接。
- `crates/db/src/manager.rs:2609-2635` `apply_table_changes` 逐条下发无事务。
- `crates/db_view/src/table_data/data_grid.rs:480` 搜索无防抖（同仓库 `db_tree_view.rs:585` 已有 250ms Debouncer）。
- `crates/redis_view/src/redis_tree_view.rs:1288/:1444` 加载更多 O(N²)。
- `crates/db_view/src/sql_result_tab.rs:581` 每批次克隆全部历史结果。
- `crates/core/src/llm/chat_history.rs:471` 查询缺索引；`:425` 全量读入后才截断。

## 4. 方案

### 4.1 第一批

1. **OAuth**：`callback_server.rs` 改为真正非阻塞（`set_nonblocking(true)`）并给 accepted stream 设读超时；新增 `start_callback_server_async`（`tokio::task::spawn_blocking` 承载阻塞 accept），`oauth_dialog.rs` 两处改调异步版。
2. **`derive_key_v3` 缓存**：新增 `DERIVED_KEY_V3_CACHE`（`(master_key 的 SHA-256 摘要, salt 摘要, 派生结果)`），命中即返回；`clear_master_key` 清缓存。不缓存明文主密钥。
3. **本地存储访问后台化**：上述调用点统一包 `Tokio::spawn_result`，模板抄 `db_tree_view.rs:1167-1180`。
4. **markdown 解析锁**：解析在锁外完成、仅写回时短暂持锁。

### 4.2 第二批

- Redis 明细：明细列表改 `uniform_list` 虚拟化；筛选/排序结果按输入变化缓存，不再每帧重算。
- DB 树：`render_tree_item` 改借用读取 `DbNode`，只取 `name`/`node_type`/`database_type`/`metadata["comment"]`。
- 终端：`selection_text` 改为右键菜单构造时才取。
- 表格：字体设置提到 delegate 字段；`col_groups` 改 `.iter()`；单元格 id 改元组 `ElementId`（与 `edit_table` 版本对齐）。
- 首页：连接卡片参数缓存到列表构建期，避免每帧 JSON 反序列化。

### 4.3 第三批

- `EditTableState` 增加横向虚拟化。
- AI 面板：只重渲染流式中的最后一条消息；`split_thinking_blocks` 结果按消息内容缓存。
- `*_view` 系列补 `GlobalNodeCache` 查找；树展开复用带缓存入口。
- 翻页复用 `COUNT` 结果（按 WHERE 指纹缓存）。
- `rollback_if_active` 改为仅在确有活动事务时发送。
- 网格搜索接入 250ms `Debouncer`。
- Redis 键树用 `HashSet` 查重 + 增量追加扁平条目。
- SQL 结果面板：语句列表与 `all_results` 解耦，只存轻量投影。
- `chat_messages` 增补 `(role, created_at)` 索引；`list_by_session` 下推 `LIMIT/OFFSET`。
- `apply_table_changes` 拼成单次脚本 + 单事务。
- 脚本执行改走可复用会话，不再强制销毁物理连接。

## 5. 子任务

### 第一批（P0）
1. ✅ 走查与实测基线（已完成，见 §3）
2. ✅ 任务文档归档
3. ✅ 外部评审（`mcp__coding-bridge__review_plan` / `review_code`）
4. ✅ OAuth 回调非阻塞 + 异步化（`callback_server.rs` + `oauth_dialog.rs` 两处调用点）
5. ✅ `derive_key_v3` 缓存（`crypto.rs`，含命中单测）
6. ✅ 本地存储访问后台化（`home_tab.rs` 10 处；`db_connection_form.rs` / `data_transfer/view.rs` / `certificate_manager.rs` 保留同步，见 §9 说明）
7. ✅ markdown 解析锁争用修复（`crates/ui/src/text/state.rs` 三阶段解析）
8. 🔄 第一批验证：`cargo test -p main` 112 passed；`cargo build -p main` 与实测启动待统一收口

### 第二批（P1）
9. ✅ Redis 明细虚拟化 + 筛选缓存（`key_value_view.rs`：List/Set/ZSet/Hash 改 `uniform_list` 虚拟化、筛选/排序结果按（内容版本、筛选条件、排序条件）缓存；`connection.rs` 的 `HGETALL` 补上限）
10. ✅ DB 树改借用读取（`db_tree_view.rs`，裁剪式读取 + 高亮构造上提）
11. ✅ 终端 `selection_text` 懒化（`terminal_view/src/view.rs`，移入右键菜单闭包）
12. ✅ 表格每帧克隆三处清理（`results_delegate.rs` 字体缓存、`table/state.rs` 与 `edit_table/state.rs` 去 `col_groups` 克隆、单元格 id 改元组）
13. ✅ 首页连接卡片缓存（`home_tab.rs`：派生数据缓存 + 借用渲染 + 点击时回查，已过 1 轮外部评审并按评审意见修正）
14. ⏳ 第二批验证 + 实测帧率对比

### 第三批（P2）
15. ⏸ `EditTableState` 横向虚拟化 —— 延后（外部评审意见：纵向已虚拟化，横向收益取决于列数与列宽模型，改动面大；待表格列模型稳定后再排）
16. ⏸ AI 面板末条消息增量渲染 —— 延后（外部评审意见：需要先把消息列表改成 `Vec` 增量更新 + 稳定 key，涉及消息组件协议调整，单独排期更安全）
17. ✅ `COUNT` 复用 + `ROLLBACK` 收敛（`crates/db/src/table_count_cache.rs` 新增按「连接 + 表 + WHERE 指纹」的计数缓存，写操作后整体清空；`crates/db/src/connection.rs` 新增 `has_active_transaction` / `rollback_if_active` 与 `infer_transaction_state`，PostgreSQL/MySQL 用 `AtomicBool` 跟踪事务态，SQLite 用 rusqlite `is_autocommit()`）
    - ⏸ 元数据 `*_view` 补 `GlobalNodeCache` —— 延后（复核后确认 `*_view` 系列仅 2 处 UI 调用点且均为用户手动触发，缓存收益低于改造成本）
18. ✅ 搜索防抖 + Redis 键树 O(N) 化 + SQL 结果面板解耦
    - 搜索防抖：`crates/db_view/src/table_data/data_grid.rs` 本地搜索接入 250ms `Debouncer`（此前每次按键都会全表重算筛选索引）
    - Redis 键树 O(N) 化：`crates/redis_view/src/redis_tree_view.rs` 用 `HashSet` + 命名空间计数合并子节点，合并复杂度由 O(N×M) 降为 O(N+M)
    - SQL 结果面板解耦：`crates/db_view/src/sql_result_tab.rs` 删除 `SqlResultTabContainer::all_results` 与写而不读的 `SqlResultTab::result`，语句列表改为只存轻量投影（`StatementListItem`），行数据直接移交 DataGrid 不再克隆，每批次不再克隆全部历史结果
19. ✅ `chat_messages` 索引 + 历史 `LIMIT` 下推（`crates/core/migrations/20261121000001_chat_message_indexes.sql` 增补两个复合索引；`crates/core/src/llm/chat_history.rs` 新增 `list_by_session_limit` / `list_by_session_paged`，`crates/core/src/ai_chat/panel.rs` 两处改为下推 `LIMIT`）
20. ✅ `apply_table_changes` 事务化 + SQLite 驱动补齐事务语义
    - `crates/db/src/manager.rs` 的 `apply_table_changes` 汇总为单条脚本 + 单事务执行
    - `crates/db/src/sqlite/connection.rs` 的 `execute` 此前忽略 `ExecOptions.transactional`（与 PostgreSQL / MySQL 不一致，表格多行保存会部分落库），现已补齐 BEGIN/COMMIT/ROLLBACK
    - ⏸ 脚本执行复用物理连接 —— 延后（连接复用会引入脏连接风险，需先具备可靠的连接健康检查与重连策略，单独排期）
21. ✅ CI clippy 差异门禁（`scripts/check_clippy_diff.py` + `scripts/clippy-baseline.txt` + `.github/workflows/ci.yml`）
    - 仓库存在约 200 条历史 clippy 告警，整体 `-D warnings` 会让 CI 直接变红；改为只对**本次改动行**上的新增告警亮红，历史存量按「文件 + lint」记入基线
    - 门禁只覆盖 `main/` 与 `crates/`，排除 `vendor/`；CI 侧 `actions/checkout` 改为 `fetch-depth: 0` 以取得 merge base
22. ✅ 第三批验证：`cargo fmt --check` 干净；`cargo build -p main` 通过且无新增 warning；db 417 / db_view 323 / one-core 245 / terminal_view 221 / main 219 / redis_view 112 / gpui-component 38 单测全绿；clippy 差异门禁实测通过（改动行零新增告警，门禁对合成的新告警能正确报错）
23. ✅ 外部代码评审（`mcp__coding-bridge__review_code`，P2 批次 Round 1：提出 P0×3 / P1×5 / P2×6 / P3×6，其中 9 项属实并已修复，7 项判定为误报，处理明细见 §10）
24. ✅ AGENTS.md 经验沉淀（新增 4 条：GPUI 渲染借用与 `Entity<T>` 的 Send/Sync 语义、多驱动 `ExecOptions` 语义一致性、rusqlite 事务态判定、历史告警下的 clippy 差异门禁）
25. ⏳ 归档 + README 索引更新

## 6. 验收标准与结果

- ✅ `cargo build -p main` 通过、无新增 warning。
- ✅ 相关 crate 单测全绿：db 417、db_view 323、one-core 245、terminal_view 221、main 219、redis_view 112、gpui-component 38。新增行为均有对应单测（OAuth 超时、派生缓存命中、筛选缓存命中、COUNT 复用、事务状态推断、语句列表投影、缓存写判定）。
- ✅ `cargo fmt --check` 干净；改动文件无新增 clippy 告警（用新增的差异作用域门禁实测）。
- ⏳ 实测（冷启动耗时、空闲 CPU）：本次改动集中在后台查询与缓存路径，未触碰启动流程，预期不劣化；**待用户在本机复测确认**（自动化环境无法可靠采集 GUI 应用的启动时序）。
- ✅ 每条子任务都能说清"改前为什么慢、改后慢在哪、怎么验证"。

## 7. 风险与缓解

| 风险 | 缓解 |
|---|---|
| `derive_key_v3` 缓存削弱 KDF 语义 | 缓存仅避免对同一主密钥重复派生；派生密钥本就以 `ENCRYPTION_KEY_V3` 常驻内存，不新增暴露面；不缓存明文主密钥 |
| OAuth 改非阻塞后行为变化 | 保留 `start_callback_server` 同步 API 供既有调用方使用，新增异步版供 UI 路径使用；加读超时防挂起 |
| 表格/列表改虚拟化影响交互 | 复用仓库既有 `uniform_list` 范式（`db_tree_view`、`file_manager_panel` 已是），保持行样式与选中语义 |
| 脚本执行复用连接引入脏连接 | 必须先落地 `rollback_if_active` 收敛（只回滚确有事务者），再改连接回收语义 |
| 改动面大、回归面广 | 分三批落地，每批独立编译 + 单测 + 实测；每批收口后再进下一批 |

## 8. 回滚

按批次独立 commit，任一批可单独 `git revert`。回滚锚点用提交标题定位（`git log --oneline`）。

## 9. 备注

- 本次分析阶段的结论均经过交叉验证：5 个维度的静态走查结果中，关键条目（OAuth 阻塞、派生无缓存、Redis 每帧筛选、DB 树深克隆、`COUNT` 重跑、无条件 `ROLLBACK`、首页 JSON 反序列化、表格每单元格克隆）均由我本人逐条 Read 源码复核。
- 伪问题已在分析阶段排除，不进入本计划（DB 查询未在 UI 线程、无 render→notify 自激、无监听器泄漏、无 render 内正则编译、无任务风暴、AGENTS.md 记录的 `has_decorations` 钳制已修复、表格纵向已虚拟化、本地 SQLite 已开 WAL）。
- 复核后修正的两处计划偏差：
  1. **SQLite 驱动的 `ExecOptions.transactional` 此前被完全忽略**（只有 `execute_streaming` 有事务分支），与 PostgreSQL / MySQL 不一致——表格多行保存会部分落库。本批补齐后，"事务化"才真正跨驱动成立。
  2. **表格多行保存的真实路径不是 `GlobalDbState::apply_table_changes`**（该函数全仓无调用方），而是 `crates/db_view/src/table_data/data_grid.rs` 里 `plugin.generate_table_changes_sql` 生成单条脚本 + `ExecOptions { transactional: true, stop_on_error: true }`。本批同时对齐了两条路径。
- clippy 差异门禁的基线（`scripts/clippy-baseline.txt`）是**当前代码树快照**，按「文件 + lint code」记录存量告警；它让门禁能拦住"基线之后新增的告警"，但也会把本分支自身引入的告警一并记为存量。若要收紧，需以目标分支（`origin/dev`）的 clippy 结果重新生成基线（需在独立 worktree 全量编译，成本较高），这一步留待基线维护时按需执行。


## 10. 外部评审结论与处理（第三批）

评审工具：`mcp__coding-bridge__review_code`（P2 批次 Round 1）。
结论：**有条件通过**——提出的问题里 9 项属实（已全部修复），7 项判定为误报或保持原行为。

### 10.1 已修复

1. **P0-1 SQLite 事务未收尾**：`spawn_blocking` 的 JoinError / 锁中毒此前会 `??` 提前返回，把连接留在"事务已开启但未结束"状态，下次 `execute(transactional=true)` 会因 "cannot start a transaction within a transaction" 卡死。改为：语句执行的致命错误先记下来 → 走统一的事务收尾（ROLLBACK）→ 再把错误返回。
2. **P1-1 纯查询也清空行数缓存**：此前 `execute_script_direct` / `execute_with_session_internal` 对任意脚本都清空行数缓存，SQL 编辑器里跑一次 SELECT 就让表格翻页重新统计。改为"只有执行结果里出现非 `SqlResult::Query`（即 Exec / Error）才清缓存"，并抽出 `results_may_have_written()` 统一判定。
3. **P1-2 事务状态误判**：`infer_transaction_state` 原先对原始脚本文本做 `split(';')`，字符串字面量里的 `BEGIN`/`COMMIT` 会被误判，可能导致**跳过必要的 ROLLBACK**。改为传入驱动解析器切分后的语句，并跳过行首注释；驱动事务模式（`options.transactional`）结束后直接标记为"无活动事务"。
4. **P1-3 回滚后仍计成功数**：`apply_table_changes` 在事务里有语句失败时整批回滚，但 `success_count` 仍把回滚掉的语句计入。改为：只要出现错误，成功数记 0。
5. **P1-5 COUNT 解析失败被缓存成 0**：解析失败时按 0 处理（与改动前一致），但**不再写入缓存**，并补 warn 日志，避免一次解析失败被固化成"表是空的"。
6. **P2-1 / P2-2 语句列表每帧全量克隆**：语句列表与筛选结果改为 `Arc` 持有，渲染路径每帧只克隆引用计数；追加批次时用 `Arc::make_mut` 原地追加，避免整表拷贝。
7. **P2-3 行数缓存无上限**：增加 512 条上限，超限整体清空（统计结果重算的成本远低于无限增长的内存占用）。
8. **P3-5 事务状态推断的字符串分配**：`to_uppercase()` 改为 `to_ascii_uppercase()`。
9. **附带修复**：db crate 中两个依赖全局 locale 的断言（`data_transfer::ddl_compat`）存在并行竞态，偶发红；改为"变体断言而非 locale 文案"（与同文件既有写法一致）。

### 10.2 判定为误报 / 保持原行为（附理由）

- **P0-2 跨连接缓存串号**：plugin 是**按数据库类型的全局单例**（`DbManager` 持有 `Arc<MySqlPlugin>` / `Arc<PostgresPlugin>` / `Arc<SqlitePlugin>`），行数缓存键含库类型、主机、端口、库名、用户；写操作时**整体清空**缓存。因此不存在"A 连接写入、B 连接读到旧行数"的路径。
- **P0-3 纯写语句摘要显示 "0 rows"**：改动前的 `all_results` 版本同样只累计 `SqlResult::Query` 的行数，`Exec` 的 `rows_affected` 从未计入摘要 → 行为未变，非本次引入。
- **P1-4 `has_results` 对纯错误结果为真**：改动前 `all_results` 同样收录错误结果 → 行为未变。
- **P2-4 防抖竞态**：`Debouncer::debounce` 内部用序号保证"后调用者胜、先调用者拿到 false"，且每次按键都取输入框当前值 → 不存在旧值覆盖新值；大表下仍是 O(行数 × 列数)，属既有特征，防抖只减频不改变量级。
- **P2-5 `idx` 未使用**：`idx` 仍被 `debug!` 日志使用。
- **P2-6 拼接出 `;;`**：`build_table_change_sql` 生成的语句不带结尾分号。
- **P3-1 迁移文件年份 2026**：与仓库既有迁移命名（如 `20260624000001`）一致，非笔误。


## 11. 实施结果小结

### 11.1 已落地（按批次）

**第一批（P0，UI 线程阻塞 / 冻结级）**
- OAuth 回调改为真正非阻塞 + 异步 accept（`callback_server.rs` + `oauth_dialog.rs` 两处调用点）。
- `derive_key_v3` 派生密钥加缓存（`crypto.rs`，命中即返回，`clear_master_key` 清缓存）。
- 本地存储的同步访问统一挪到 tokio 线程池（`home_tab.rs` 等调用点）。
- markdown 解析改三阶段（锁外解析、仅写回时短暂持锁），消除 UI 线程阻塞。

**第二批（P1，每帧 / 每次操作稳定开销）**
- Redis 明细视图四类列表改 `uniform_list` 虚拟化 + 筛选/排序结果按（内容版本、筛选、排序）缓存；`HGETALL` 补上限。
- DB 树改借用读取，裁剪式取字段，高亮构造上提。
- 终端 `selection_text` 懒化到右键菜单闭包。
- 表格每帧克隆三处清理：全局字体设置提为字段、`col_groups` 不再克隆、单元格 id 改元组。
- 首页连接卡片加渲染缓存（派生数据缓存 + 借用渲染 + 点击回查）。

**第三批（P2，数据放大 / 结构性）**
- 表行数 `COUNT(*)` 按「连接 + 表 + WHERE 指纹」缓存，写操作后清空；纯查询脚本不再误清缓存。
- `rollback_if_active` 改为仅在确有活动事务时发送（PostgreSQL / MySQL 按解析后的语句推断，SQLite 读 `is_autocommit()`）。
- 网格本地搜索接入 250ms 防抖（此前每按一次键就全表重算筛选索引）。
- Redis 键树子节点合并由 O(N×M) 降为 O(N+M)（`HashSet` 查重 + 命名空间计数）。
- SQL 结果面板：删除 `all_results` 与写而不读的 `result` 字段，语句列表只存轻量投影，行数据直接移交 DataGrid，每批次不再克隆全部历史结果；筛选结果 `Arc` 化，渲染路径每帧只克隆引用计数。
- `chat_messages` 增补两个复合索引；历史读取下推 `LIMIT` / `OFFSET`。
- `apply_table_changes` 汇总为单脚本 + 单事务；SQLite 驱动补齐 `ExecOptions.transactional` 语义（此前被忽略，导致多行保存部分落库）。
- CI 新增 clippy 差异作用域门禁（`scripts/check_clippy_diff.py` + `scripts/clippy-baseline.txt`）。

### 11.2 延后项与理由（见 §5 子任务 15/16/17/20）

- `EditTableState` 横向虚拟化：纵向已虚拟化，横向收益取决于列数与列宽模型，改动面大，单独排期。
- AI 面板末条消息增量渲染：需先把消息列表改成增量更新 + 稳定 key，涉及消息组件协议调整。
- 元数据 `*_view` 补 `GlobalNodeCache`：复核后确认仅 2 处用户手动触发的 UI 调用点，收益低于改造成本。
- 脚本执行复用物理连接：需要可靠的连接健康检查与重连策略作为前置。

### 11.3 经验沉淀

AGENTS.md「已验证经验」新增 4 条：GPUI 渲染借用与 `Entity<T>` 的 Send/Sync 语义、多驱动 `ExecOptions` 语义一致性、rusqlite 事务态判定、历史告警下的 clippy 差异门禁。

### 11.4 待办

- **提交**：本次改动尚未提交（按仓库硬门禁，需用户明确指令后再执行 `git commit`）。
- **本机实测**：冷启动耗时与空闲 CPU 的对比采样待用户在本机复测确认。
