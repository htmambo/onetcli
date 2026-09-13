# 修复数据库导入/导出 sync 函数在 GPUI 后台 executor 上的 tokio reactor panic

**Status**: 🔄 进行中
**创建时间**: 2026-09-13
**前置依赖**: Submenu 点击触发首项修复（SUBMENU_CLICK_FIRST_ITEM_PLAN.md，已评审通过修订）

## 背景与目标

### 用户反馈（2026-09-13）
> 数据库和数据表的右键菜单中的`转储SQL文件`点击后为什么没有反应？
> 刚才说错了，`运行SQL文件`点击后有弹窗。不过后续功能没有继续测试。
> MYSQL导出SQL也同样崩溃：
> thread 'Worker-12' panicked at crates/db/src/mysql/connection.rs:393:27
> there is no reactor running, must be called from the context of a Tokio 1.x runtime

### 现象
- 在 MySQL/SQLite 数据库/表右键菜单点击"转储 SQL 文件" → 弹出 SqlDumpView → 选择目录 → 开始导出 → **Worker 线程 panic**
- Worker 线程 panic 链式触发 `main` 线程 "Task polled after completion"，整个应用崩溃退出
- PostgreSQL 大概率同样受影响（待用户实测验证）

### 期望
- 转储 SQL 文件（结构 / 数据 / 结构和数据）在 MySQL/SQLite/PostgreSQL 上能正常完成
- 表导入导出功能（import_data_with_progress_sync / export_data_with_progress_sync）同样修复
- 不引入新的 panic 路径

## 根因分析（backtrace 已确认）

### 完整调用栈（MySQL backtrace，SQLite 同源）
```
sql_dump_view.rs:264   cx.background_spawn (GPUI BackgroundExecutor)
                              ↓ future.poll (frame 14, BackgroundExecutor)
sql_dump_view.rs:271   start_dump::{closure#2}::{closure#0}::{closure#2}
                              ↓
manager.rs:2433        export_data_with_progress_sync.{closure#0}
                              ↓ .await create_session
manager.rs:391         ConnectionManager::create_session.{closure#0}
                              ↓ plugin.create_connection(...).await?
mysql/plugin.rs:1176   MySqlPlugin::create_connection.{closure#0}
                              ↓ conn.connect().await?
mysql/connection.rs:393 timeout(Duration::from_secs(...), Conn::new(opts)).await  ← panic
                              ↓
tokio::time::sleep::Sleep::new_timeout  (frame 4)
                              ↓
tokio::runtime::scheduler::Handle::current  (frame 3)  ← 找不到 reactor
```

### 根因（一句话）
`export_data_with_progress_sync` / `import_data_with_progress_sync`
（`crates/db/src/manager.rs:2419, 2504`）**没有用 `Tokio::spawn_result` 包裹**对 `create_session` 的调用，
导致 connect() 内部的 `tokio::spawn_blocking` / `tokio::time::timeout` 在
**GPUI BackgroundExecutor** 上 poll 时找不到 tokio reactor 而 panic。

### 现状对照（同模块内姊妹函数）

| 函数 | manager.rs 行号 | 用 Tokio::spawn_result？ | 状态 |
|---|---|---|---|
| `export_data_with_progress` | 2374 | ✅ 是 | 正常 |
| `export_data_with_progress_sync` | 2419 | ❌ 否 | **panic** |
| `import_data` | 2458 | ✅ 是 | 正常 |
| `import_data_with_progress_sync` | 2504 | ❌ 否 | **panic** |

`with_plugin_session_db!` 宏（manager.rs:82-127）也用了 `Tokio::spawn_result` 包裹 `create_session`，
所以通过该宏触发的 connect（包括 list_tables、refresh 等常见 UI 操作）**正常**。

### 受影响路径
| 触发点 | 调用栈 | 是否 panic |
|---|---|---|
| SqlDumpView（转储 SQL 文件 / 表导入导出） | sql_dump_view.rs:264 → export_data_with_progress_sync → connect | ✅ panic |
| TableExportView（导出表数据） | table_export_view.rs:605 → export_data_with_progress_sync → connect | ✅ panic |
| TableImportView（导入表数据） | table_import_view.rs:498 → import_data_with_progress_sync → connect | ✅ panic |
| OpenTableData / ListTables / Refresh 等 | with_plugin_session_db! 宏 → Tokio::spawn_result | ❌ 正常 |
| RunSqlFile（运行 SQL 文件） | 直接 read_sql 不需要 connect | ❌ 正常 |

## 解决方案

### 方案 A（推荐）：把 `_sync` 函数统一为 `&mut AsyncApp` 版本
- 给 `export_data_with_progress_sync` / `import_data_with_progress_sync` 加 `cx: &mut AsyncApp` 参数
- 函数体改用 `Tokio::spawn_result(cx, async move { ... }).await` 包裹整个 future
- 3 个调用方同步改：sql_dump_view.rs:266、table_export_view.rs:607、table_import_view.rs:500
- 与现有 `export_data_with_progress` / `import_data` 签名保持一致（项目主流做法）

### 不采用方案 B（保持"sync"签名）
- 让 `_sync` 函数保持无 cx 参数，内部通过 `cx.update_global(...)` 取 `GlobalTokio` handle 自 spawn
- 多一层间接，且 cx 不在签名里意味着无法访问其他 global（受 `_with_progress` 已用 `&mut AsyncApp` 的现状约束，反而引入不一致）

### 后续清理（不在本任务范围）
- 重命名 `_sync` 为 `_with_progress`（去掉 sync 命名），或反之把 `_with_progress` 也改成同样名字
- 当前保留两个名字，最小化改动面

## 关键文件改动

### 修改（5 个）

| 文件 | 改动 |
|---|---|
| `crates/db/src/manager.rs:2419-2455` | `export_data_with_progress_sync` 加 `cx: &mut AsyncApp` 参数；内部改用 `Tokio::spawn_result` 包裹 |
| `crates/db/src/manager.rs:2504-2542` | `import_data_with_progress_sync` 加 `cx: &mut AsyncApp` 参数；内部改用 `Tokio::spawn_result` 包裹 |
| `crates/db_view/src/import_export/sql_dump_view.rs:264-272` | 调用方传入 `cx`；`export_handle = Tokio::spawn_result(cx, async move { ... }).await` |
| `crates/db_view/src/import_export/table_export_view.rs:605-611` | 同上 |
| `crates/db_view/src/import_export/table_import_view.rs:498-507` | 同上 |

### 验证（用户实测）
1. MySQL 库右键 → 转储 SQL 文件 → 结构 → 选目录 → 完整执行不 panic
2. MySQL 库右键 → 转储 SQL 文件 → 数据 → 选目录 → 完整执行不 panic
3. MySQL 库右键 → 转储 SQL 文件 → 结构和数据 → 选目录 → 完整执行不 panic
4. SQLite 表右键 → 同上三步
5. PostgreSQL 表右键 → 同上三步（如果用户有 PG 库）
6. 表右键 → 导出表数据 → 完整执行不 panic
7. 表右键 → 导入表数据 → 完整执行不 panic

### 自动化验证
- `cargo check -p db -p db_view` 通过
- `cargo clippy -p db -p db_view -- -W dead_code` 无新增警告
- `cargo fmt --check` 通过
- 现有测试通过（`cargo test -p db --lib`）

## 风险与回滚

| 风险 | 缓解 |
|---|---|
| 函数签名变化破坏其他调用方 | 全文已 rg 确认仅 3 个调用方（sql_dump_view/table_export_view/table_import_view），全部同步改 |
| `Tokio::spawn_result` 内部使用 `cx.background_spawn` 包装 JoinHandle，调用方原本也是 `cx.background_spawn` —— 可能引起 task 嵌套层级变化 | Tokio::spawn_result 设计上就是干这个的（gpui_tokio.rs:81-99），无风险 |
| 转储过程中取消行为变化（之前是直接 abort future，现在两层 task 嵌套） | GPUI Task 取消会 defer abort tokio JoinHandle（gpui_tokio.rs:67-70），行为一致 |
| 修复后是否还有遗漏的 connect 路径 | 通过用户实测 + 自动化测试覆盖；如果发现新 panic 路径另立任务 |

## 实施步骤

1. ⏳ 修改 `crates/db/src/manager.rs` 两个 `_sync` 函数
2. ⏳ 修改 3 个调用方传入 `cx`
3. ⏳ `cargo check -p db -p db_view` 通过
4. ⏳ `cargo clippy -p db -p db_view` 无新增警告
5. ⏳ 外部评审（coding-bridge review_code，1 轮）
6. ⏳ 用户实测 MySQL/SQLite/PostgreSQL 转储不 panic
7. ⏳ commit + 归档（独立 commit，不与 Submenu 修复混在一起）

## 备注

- 本任务独立于 SUBMENU_CLICK_FIRST_ITEM_PLAN.md（Submenu 修复），按"风险隔离"原则独立 commit
- Submenu 修复先 commit；本任务后 commit
- 命名：`_*_sync` 是历史遗留命名（之前意图可能是让 `_with_progress` 走 progress_tx、`_sync` 不带；但实际两者都带 progress_tx），不在本任务范围清理
