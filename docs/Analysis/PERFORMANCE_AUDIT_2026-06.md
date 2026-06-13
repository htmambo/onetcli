# onetcli 性能与效率审计报告

**日期**: 2026-06-12
**范围**: 全工作区（~340k 行 Rust / 25+ crates），静态只读审计
**方法**: Codex 独立审查 + 人工 Read/grep 证据核验
**分支**: perf/manager-lock-refactor

> 已排除近期已优化项：ConnectionManager 写锁跨 await、per-session 异步锁、SQL 解析器分配点、cloud_sync `build_cloud_name_map`/`calculate_sync_plan`、edit_table 行 Vec 预分配。

---

## Top 3 最高 ROI 改动

1. **Redis `get_db_conn` 写锁跨网络建连** — 修复后消除并发首次访问串行化（低复杂度，收益明确）。
2. **cloud_sync 操作队列全串行网络 I/O** — 引入有界并发，N 个项目从 N 次串行往返降为并发往返。
3. **EditTable 宽表逐行构建全部列** — 仅构建可视列 + 缓存列尺寸 Vec，宽表（百列）渲染热点。

---

## 高优先级

### 1. [高] Redis `get_db_conn` 持写锁跨越网络建连
`crates/redis_view/src/connection.rs:303-310`
- **问题**: 双检锁命中 miss 后，在持有 `db_connections.write()` 写锁（303 行）的情况下 `await open_connection_for_db()`（308 行）。建连期间该连接对象所有 DB 缓存读写全部阻塞。
- **影响**: 同一 Redis 连接并发访问不同 db_index 时被串行化；网络慢/超时会放大为全局停顿。
- **修复**: 双检后 clone `config`，**drop 写锁** → `open_connection_for_db().await` → 重新 `write()` 二次检查再 insert。保留双检语义，仅缩短锁范围。
- **复杂度/风险**: 低。

### 2. [高] cloud_sync 操作队列全串行网络 I/O
`crates/core/src/cloud_sync/generic_sync.rs:176-` (`while let Some(op) = queue.dequeue()`)
- **问题**: upload/update/download/update_local 操作逐条 `.await`，无并发。
- **影响**: 同步 N 个连接 = N 次串行网络往返；首次全量同步/多连接场景延迟线性累积。
- **修复**: 对无顺序依赖的操作（多为独立项）改用有界并发（`buffer_unordered(k)` 或分批 `join`），保留 `mark_failed`/`retry_failed` 语义。k 取 4~8，避免压垮云端。
- **复杂度/风险**: 中（需保持队列重试与错误聚合语义）。

### 3. [高] EditTable 宽表逐可视行构建全部列 + 重建列尺寸 Vec
`crates/one_ui/src/edit_table/state.rs`（visible processor / 行渲染路径）
- **问题**: 行已虚拟化，但每个可视行仍为**全部列**构建元素，且每次 visible 处理重建整列尺寸 `Vec`。
- **影响**: 宽表（上百列）下，每帧元素数 = 可视行 × 全部列，列尺寸 Vec 每帧重分配 → 渲染掉帧。
- **修复**: 引入列可视区裁剪（仅构建水平可视列）；列尺寸 `Vec` 在列结构未变时缓存复用（脏标记失效）。
- **复杂度/风险**: 中（需正确处理水平滚动与冻结列）。

---

## 中优先级

### 4. [中] 启动路径同步串行初始化 + 同步读取恢复状态
`main/src/onetcli_app/mod.rs`、`main/src/connection_restore.rs`（首窗构建 / `HomePage::new` 前后）
- **问题**: init 流程串行；首窗构建时同步读取并反序列化 `tab_state.json` 与 `connection_restore_state.json`。
- **影响**: 冷启动到首帧延迟（磁盘 + JSON 解析在主路径）。
- **修复**: 可并行的子系统 init 并行化；恢复状态改为后台异步加载，首帧先渲染空壳/骨架再回填。
- **复杂度/风险**: 中（需处理首帧与状态回填的时序）。

### 5. [中] `list_detached` 持 registry 读锁跨 per-handle await
`crates/terminal/src/local_pty_host.rs:63-75`
- **问题**: 持 `sessions.read()` 读锁，循环内对每个 handle `await attached.read()` / `last_detached_at.read()`。TTL 任务每 30s 触发。
- **影响**: 遍历期间阻塞 session 的 `insert`/`remove`（写锁）；会话多时窗口拉长。
- **修复**: 先快照 `Vec<(id, handle Arc)>` 后 **drop registry 读锁**，再逐个 await 子锁。
- **复杂度/风险**: 低。

### 6. [中] cloud_sync 全量 clone 构建查找表
`crates/core/src/cloud_sync/generic_sync.rs:113-120`
- **问题**: `local_item_map` / `cloud_data_map` 对每个 item/CloudSyncData 整体 `.clone()` 后入 HashMap。
- **影响**: 大数据量同步时一次性深拷贝全部条目，内存与 CPU 翻倍。
- **修复**: map 存索引或 `&` 引用（`HashMap<_, &H::Item>`），或 `Rc`/`Arc` 共享；按需取用而非预 clone 全集。
- **复杂度/风险**: 中（生命周期/借用调整）。

### 7. [中] 列筛选值重算遍历全表
`crates/one_ui/src/edit_table/state.rs`（filter 值集计算）
- **问题**: 计算某列可选筛选值时遍历全部行，并对每行应用其余所有列的筛选判断。
- **影响**: 多筛选条件叠加时近似 O(行 × 列) 每次重算。
- **修复**: 缓存筛选结果集，仅在依赖筛选变更时失效重算；按列预建值索引。
- **复杂度/风险**: 中。

### 8. [中] UI async 闭包内直接 `std::fs::read_to_string` 阻塞 executor
SQL 编辑器 / 证书导入 / SSH key 导入路径（`cx.spawn` async 闭包内同步文件读）
- **问题**: 在 GPUI async executor 上下文直接做同步磁盘 I/O。
- **影响**: 大文件/慢盘时阻塞 UI executor，界面卡顿。
- **修复**: 改用 `background_executor().spawn` + `spawn_blocking` 做文件读，结果回主线程。
- **复杂度/风险**: 中（需逐处确认实际文件大小与调用频次，部分小文件可不改）。

---

## 低优先级 / 构建与依赖

### 9. [低] 缺少无用依赖常态检测
- `cargo-machete` 未安装，无法定期清理无用依赖。建议 `cargo install cargo-machete` 并纳入 CI lint。

### 10. [低] dev profile 链接可进一步提速
`Cargo.toml [profile.dev]`：当前 `debug = true`。若不强依赖完整变量调试，可评估 `debug = "line-tables-only"` 加快链接、缩小 target/。
- 现有配置已较优（`split-debuginfo = "unpacked"`、热依赖 `opt-level = 3`、release `lto = "fat"` / `codegen-units = 1`）。此项为可选权衡。

### 11. [确认项-非问题] HTTP client 复用良好
`crates/reqwest_client` 仅在设置页测试连接（`setting_tab.rs`）等少数点构造，未发现请求级重复创建 client。无需改动。

### 12. [确认项-非问题] mongo/redis `remove_connection` 写锁
`mongodb_view/manager.rs:75`、`redis_view/manager.rs:62`：连接已先从 DashMap `remove` 取出再 `write().await`，为独占本地句柄，无竞争。无需改动。

---

## 备注与覆盖度说明

- 计划派遣 5 个并行子代理深挖 db 后端内部 / 完整 UI / 构建依赖，因运行环境 `[1m]` 上下文继承限制全部启动失败（provider 侧 400）。该部分由 Codex 审查 + 人工抽样核验补齐，故 **db 驱动内部细粒度热点**与**依赖图重复项（`cargo tree -d`）**覆盖较浅，建议后续单独跑一轮。
- 所有列出的高优先级项均已 Read 源码逐行核验；中优先级第 3/7/8 项来自 Codex 静态分析，落地前应再做一次定点确认。

## 建议落地顺序

1. 先做 #1（Redis 写锁，低风险高确定性）与 #5（PTY 读锁，低风险）。
2. 再做 #2（同步并发）与 #3（宽表渲染），收益最大但需测试。
3. #4/#6/#7/#8 视实测数据决定优先级。
4. #9 纳入 CI；#10 团队讨论调试权衡。

---

## 实施状态（2026-06-12，分支 perf/manager-lock-refactor）

### ✅ 已实施（编译/clippy 验证）
- **#1 Redis 写锁** — `redis_view/connection.rs` `get_db_conn`：建连移出写锁外，建连后再加写锁二次检查并 insert。消除写锁跨网络 I/O。
- **#5 PTY 读锁** — `terminal/local_pty_host.rs` `list_detached`：先快照 handle（Arc clone）释放 registry 读锁，再 await 各子锁。
- **#6 cloud_sync 借用查找表** — `core/cloud_sync/generic_sync.rs`：`local_item_map`/`cloud_data_map` 由 `HashMap<K, owned>` 改为 `HashMap<K, &T>`，消除每次同步对全部条目的深拷贝。下游绑定类型不变（`&H::Item`/`&CloudSyncData`），纯优化无语义变化。

### ⏸️ 本轮推迟（附理由）
- **#2 cloud_sync 操作并发** — 推迟。执行循环将网络 I/O 与本地 DB 状态变更（`handler.on_uploaded(engine, …)`）交织，并依赖单所有者队列的 `mark_failed`/`retry_failed` 重试与错误聚合顺序。盲目引入并发存在**数据一致性/重试语义破坏**风险，且本环境无法对真实云端做并发集成测试。应在具备云端联调条件时单独立项：拆分「可并行网络阶段」与「串行本地落库阶段」。
- **#3 宽表渲染（列裁剪 + 列尺寸 Vec 缓存）** — 推迟。涉及 GPUI 表格水平虚拟化与冻结列时序，回归风险高，需运行时/视觉验证（本环境无法启动 GUI 比对）。
- **#7 列筛选值重算缓存** — 推迟，需先 profiling 确认实际数据量级。
- **#8 UI async 闭包内 `std::fs` 阻塞** — 推迟，需逐处确认文件大小/调用频次，避免对小文件过度改造。

### 📋 基础设施/团队权衡
- **#9** `cargo-machete` 未安装；建议 `cargo install cargo-machete` 并纳入 CI。
- **#10** dev profile `debug` 可选 `line-tables-only`，团队权衡调试体验后决定。
