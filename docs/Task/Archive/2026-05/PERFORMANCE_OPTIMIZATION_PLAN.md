# 全局性能优化任务计划

**状态**: 🔄 进行中 (开始时间: 2026-05-20)

## 任务目标

修复应用中发现的所有性能和效率问题，涵盖 CRITICAL / HIGH / MEDIUM / LOW 四个优先级。

## 任务分解

### CRITICAL

- ⏳ C1: SQL 查询注入 LIMIT（executor.rs）
- ⏳ C2: 主线程同步文件 I/O 异步化（themes.rs, connection_restore.rs, certificate_manager.rs, tab_persistence.rs, storage/manager.rs）
- ⏳ C3: EditorTableDelegate 数据 Arc 化（results_delegate.rs）

### HIGH

- ⏳ H1: Terminal 字体缓存优化（terminal_element.rs）
- ⏳ H2: GlobalDbState Arc 化（db/manager.rs）
- ⏳ H3: DB Cache Manager 消除双重 Clone（cache_manager.rs）
- ⏳ H4: CellEditor 渲染 Clone 优化（one_ui/edit_table/delegate.rs）

### MEDIUM

- ⏳ M1: Cloud Sync 队列加上限（cloud_sync/service.rs）
- ⏳ M2: HTTP 请求加总超时（reqwest_client.rs）
- ⏳ M3: Tab 持久化 debounce（tab_persistence.rs）
- ⏳ M4: SFTP 下载任务池化（russh_impl.rs）
- ⏳ M5: 终端历史去重优化（history.rs）
- ⏳ M6: 过滤计算优化（results_delegate.rs）
- ⏳ M7: DataGrid notify 合并（data_grid.rs）
- ⏳ M8: 缓存递归失效优化（cache.rs）

### LOW

- ⏳ L1: 终端历史 to_lowercase 优化（history.rs）
- ⏳ L2: 证书管理器 clone→into（certificate_manager.rs）
- ⏳ L3: Icon path clone 优化（icon.rs）

## 验收标准

- `cargo check --all` 通过
- `cargo clippy -- --deny warnings` 通过
- 无功能回归
