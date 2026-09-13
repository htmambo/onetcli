# 数据库 import/export session 生命周期泄漏修复（评审 P1-2）

**Status**: ✅ Phase 1 + Phase 2 评审 APPROVED；Phase 3-7 待开始
**创建时间**: 2026-09-13
**实施时间**: 2026-09-13
**前置依赖**: 无
**关联评审**: Round 1 of commit 4b345b5f + 8e1de28c（SessionId `798c41e0-e9ea-4400-b67c-49b956b03031`）；
Round 1 of session_leak_fix（SessionId `3558b938-17a9-47d7-b52c-516c6883824f`，2026-09-13）；
Phase 1 Round 2-6 of session_leak_fix（SessionId `3ec048af...` / `f2418b44...` / `a385d924...`，2026-09-13）；
Phase 2 Round 7-11 of session_leak_fix（SessionId `302a04dd...` / `ff8581ca...` / `e99bf0d3...`
/ `e93964c9...` / `34af3599...`，2026-09-13）

## Phase 2 Round 11 评审结果（2026-09-13）—— APPROVED ✅

**Verdict**: APPROVED（SessionId `34af3599-e1b5-4d70-99ed-443a1ecbfb65`）

### Phase 2 最终交付
- **SessionGuard 接入 import/export 4 个函数 + 2 个 macros + 4 个 read 函数**：
  消除原 release_session 直接调用的 5 条 session 泄漏路径
- **`set_action` + `finish` 两步可变状态机 → `finish_with(action)` 合一 API**
- **`SessionGuard::action_for<T,E>` 单点决策函数**：10 处重复收敛为 1 处
- **sync-wrapper + `impl Future` 模式**：规避 rust#87441（#[track_caller] 不传播）
- **二次 finish_with hard no-op + error 级别 + debug_assert**：
  防止 Close-after-Reuse 协议违规
- **(None, None) 不可达分支防御**：error + debug_assert 双层
- **`last_release_id` 字段**：二次调用时携带 session_id 定位
- **Drop 命名澄清**：`original_action` / `release_action` 分离（避免 Round 7 shadowing 误会）
- **测试时钟域统一**：`tokio::time::Instant` + `tokio::time::sleep`（避免 paused-time 失效）

### 5 类生命周期测试（覆盖声称修复的全部 5 类问题）
1. `finish_with_business_failure_closes_session` — 业务失败 → Close → 连接断开
2. `finish_with_business_success_releases_for_reuse` — 业务成功 → ReleaseForReuse → 留在池中
3. `future_cancellation_releases_session_via_drop_fallback` — timeout 取消 → Drop 兜底
4. `panic_in_business_closure_releases_session_via_drop` — panic → Drop 兜底
5. `finish_with_then_drop_is_noop` — finish_with 后 Drop no-op（防双重释放）

### 测试统计
- 27/27 manager 测试通过（含 6 个新增 + 1 个 should_panic）
- 454/454 db crate 测试通过（debug build）
- 1/1 release build 测试通过（finish_with_called_twice_is_idempotent_in_release）
- cargo fmt clean
- 0 新增 clippy 警告

### Commits
- Phase 1: `51ba141f` fix(db): SessionGuard + Drop 兜底修复 import/export session 生命周期泄漏
- Phase 2: `fc465b69` fix(db): SessionGuard 接入 import/export 调用点（Phase 2）

### Phase 2 已完成的功能（来自原计划）
- ✅ Phase 5: 幂等保护（Option::take + finish_with）
- ✅ Phase 7 (部分): Commit 拆分（Phase 1 + Phase 2 各自独立 commit）

## Phase 1 Round 6 评审结果（2026-09-13）—— APPROVED ✅

**Verdict**: APPROVED（SessionId `a385d924-a428-4a53-ac21-ba4410411714`）

### Phase 1 最终交付
- **SessionGuard + Drop best-effort close**（P0-1）：覆盖 panic / future cancel 路径
- **`finish(&mut self)` 取消安全**（P1-5 简化）：release 成功后才清空 session
- **`release_session` 共享 helper + NotFound 映射**：finish 与 Drop 走同一路径
- **`SessionReleaseAction` 枚举**：Close / Release / ReleaseForReuse
- **降级函数 `downgrade_action_on_failure`**：ReleaseForReuse 失败 → Close（防脏复用）；
  finish 与 Drop **共享同一函数**，避免内联 match 双源决策
- **`SESSION_NOT_FOUND_PREFIX` + `is_session_not_found` 纯函数**（P1-4）：
  producer/consumer 共享常量、分类集中可测
- **MockConnection 错误注入**：支持 verify 失败 + disconnect_count 观测

## Round 1 评审结果（2026-09-13）

## Round 1 评审结果（2026-09-13）

VERDICT: NEEDS_CHANGES

### 已确认的真实问题（vs 当前代码）
- P0-1：panic / future 取消路径下 session 依然泄漏（无 Drop guard）
- P0-2：双决策源（`run_with_new_session(config, action, f)` 入参 action + `finish_session` 业务结果推导）
- P0-3：ReleaseForReuse 无事务回滚 / 会话态清理（临时表/SET/prepared stmt）
- P1-4：裸 API 未私有化（`release_session`/`create_session` 仍 `pub`）
- P1-5：finish_session 无 `Option::take` 幂等保护
- P2-4：测试缺口（panic、cancel、double-finish、ReleaseForReuse 真实重置、multi_thread 竞态）
- P2-5：561 行混合 commit（漏斗重构 + DumpOutcome/i18n 行为变更混在一起）

### 方案 B 决策（2026-09-13，用户授权）
1. P0-1 SessionGuard + Drop 兜底
2. P0-2 删除 `action` 入参，回收动作统一从业务结果推导
3. P0-3 所有归还路径强制事务回滚；ReleaseForReuse 显式清理会话态
4. P1-4 裸 API 私有化
5. P1-5 finish_session 幂等（Option::take）
6. P2-4 补 panic / cancel / double-finish / ReleaseForReuse 真实重置 / multi_thread 竞态测试
7. P2-5 拆分 commit：
   - commit A：manager.rs 漏斗重构（Drop guard + 错误组合 + 裸 API 私有化）
   - commit B：sql_dump_view.rs DumpOutcome/i18n UI 增强

### 实施方案（待分阶段执行）

#### Phase 1: SessionGuard + Drop guard（P0-1）
- 新增 `pub struct SessionGuard { state, session: Option<SessionHandle>, label }`
- `Drop` 实现：error 日志 + `Handle::try_current()` 下 spawn 兜底 close
- 测试：panic 路径、Drop 时未 finish 触发 close

#### Phase 2: 错误组合策略统一（P0-2）
- 删除 `run_with_new_session` 的 `action` 入参
- 引入 `SessionOutcome` enum（Success / PartialFailed / Failure / NoFile）
- `impl From<&SessionOutcome> for SessionReleaseAction` 单点决策
- Err/PartialFailed/NoFile 默认 Close（状态不可信）
- Success 走 ReleaseForReuse
- `combine_results(business, release)` 函数：
  - (Ok, Ok) → Ok
  - (Ok, Err) → Ok + error 日志（finish 内部兜底 close）
  - (Err, Err) → Err(business) + warn 日志
  - (Err, Ok) → Err(business)

#### Phase 3: 事务回滚 + 会话态清理（P0-3）
- Release / ReleaseForReuse 路径前调用 `rollback_if_active`
- ReleaseForReuse 额外调用 `reset_session_state`（DROP 临时表 / RESET session vars / DEALLOCATE prepared）

#### Phase 4: 裸 API 私有化（P1-4）
- `release_session` / `close_session` / `create_session` / `get_session_connection`
  改为 `pub(crate)` 或 `pub(super)`，仅 `SessionGuard` / `finish_session` / `run_with_new_session` 对外
- 单一漏斗

#### Phase 5: 幂等保护（P1-5）
- `finish_session` 接受 `&mut SessionGuard` 而非 `&str`
- guard.session.take() 保证只执行一次

#### Phase 6: 测试缺口（P2-4）
- panic 路径：业务函数 panic 时 Drop 兜底
- future 取消：tokio::time::timeout 包裹后取消，Drop 兜底
- double-finish：连续调用 finish_session 无副作用
- ReleaseForReuse 真实重置：复用后查询临时表为空、会话变量重置
- multi_thread 竞态：两个 task 并发 get/release session

#### Phase 7: Commit 拆分（P2-5）
- commit A：manager.rs 漏斗重构（Phase 1-6 累积）
- commit B：sql_dump_view.rs DumpOutcome/i18n

### 验证
- `cargo check -p db -p db_view` 0 error 0 warning
- `cargo test -p db --lib` 通过（含 4 个新单测 + Phase 6 新单测）
- `cargo fmt --check` 干净
- `cargo clippy -p db -- -W dead_code` 无新增警告
- 外部评审：每 phase 提交一次 review，按 CLAUDE.md §1.5 verdict 循环

### 风险
- Drop 中调用 `Handle::try_current()` 可能 panic（runtime 不在时）；用 try_current 而非 unwrap
- 事务回滚 / 会话态清理需要在各 plugin（MySQL/SQLite/PG/IPC）支持，IPC 可能没这些 API
- 拆分 commit 会让 Round 1 评审过的 manager.rs 变更被覆盖

## 背景与目标

### 评审反馈（2026-09-13，Round 1 P1-2）
> session 生命周期错误路径（正确性；既有问题，但本次代码被重排触碰）
> 两处在重排后依然存在：
> - `get_session_connection(&session_id).await?` 失败时 `?` 直接跳出整个 async 块 → `create_session` 已成功但 `release_session` 永不执行，session 泄漏；
> - `release_session(...)?` 失败时会**丢弃 `result`**——导出实际成功却向调用方报错，或导出失败的真实原因被释放错误遮蔽。

### 现象
数据库连接会话的生命周期管理在错误路径下不规范：
1. **session 泄漏**：create_session 成功但后续步骤失败时，session 永远不会被释放，长期累积可能导致连接池耗尽
2. **错误遮蔽**：导出/导入实际成功但 release_session 失败时，调用方看到的是 release 错误而非真实的成功结果
3. **反之**：导出失败的真实错误被 release 错误覆盖，调试困难

### 期望
所有错误路径下 session 都能被正确释放，调用方收到的错误信息反映真实失败原因。

## 根因分析

### 当前代码（commit 8e1de28c 中 `crates/db/src/manager.rs` 的 panic 修复版本）

```rust
let clone_self = self.clone();
Tokio::spawn_result(cx, async move {
    let plugin = clone_self.get_plugin(&db_config.database_type)?;
    let session_id = clone_self
        .connection_manager
        .create_session(db_config.clone(), &clone_self.db_manager)
        .await?;

    let result = {
        let mut guard = clone_self
            .connection_manager
            .get_session_connection(&session_id)
            .await?;
        let conn = guard
            .connection()
            .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
        plugin
            .export_data_with_progress(conn, &config, progress_tx)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    };

    clone_self
        .connection_manager
        .release_session(&session_id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    result
})
.await
```

### 问题分析
1. **第 4 行 `get_session_connection(...).await?`**：`?` 直接传播错误 → `release_session` 永不调用 → **session 泄漏**
2. **第 23 行 `release_session(...).await.map_err(...)?`**：`?` 错误遮蔽第 9-19 行 `result` 中的成功结果（即使是 `Ok` 也会被 `?` 检查，但 `release_session` 失败时 result（Ok）会被丢弃并返回 release 错误）
3. 反之：如果 `result` 是 `Err` 而 `release_session` 也失败，`release` 错误覆盖 `result` 错误，调试困难

### 适用范围
同样问题存在于：
- `export_data_with_progress`（manager.rs:2374，非 sync 版本）
- `export_data_with_progress_sync`（manager.rs:2419，sync 版本）
- `import_data`（manager.rs:2458）
- `import_data_with_progress_sync`（manager.rs:2504）
- `with_plugin_session_db!` 宏（manager.rs:82-127）

## 解决方案

### 改造目标
1. **全路径释放**：无论业务结果成功/失败，`release_session` 必须被调用
2. **错误优先级**：业务错误（导出/导入失败）优先级 > release 错误
3. **release 失败处理**：业务成功时 release 失败向上传播；业务失败时 release 失败仅 warn 日志，不遮蔽业务错误

### 改造模式（参考评审示例代码）

```rust
let clone_self = self.clone();
Tokio::spawn_result(cx, async move {
    let plugin = clone_self.get_plugin(&db_config.database_type)?;
    let session_id = clone_self
        .connection_manager
        .create_session(db_config.clone(), &clone_self.db_manager)
        .await?;

    // 内层：所有失败以 Err 落入 result，不会跳过下面的 release
    let result = async {
        let mut guard = clone_self
            .connection_manager
            .get_session_connection(&session_id)
            .await?;
        let conn = guard
            .connection()
            .ok_or_else(|| anyhow::anyhow!("Session connection not found"))?;
        plugin
            .export_data_with_progress(conn, &config, progress_tx)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }
    .await;

    // 全路径释放：导出错误优先；释放失败仅在导出成功时上抛，否则只告警
    match clone_self.connection_manager.release_session(&session_id).await {
        Err(e) if result.is_ok() => return Err(anyhow::anyhow!("{}", e)),
        Err(e) => log::warn!("release session {session_id} failed: {e:#}"),
        Ok(()) => {}
    }

    result
})
.await
```

### 关键文件改动

### 修改（5+1 处）
| 文件 | 函数/宏 | 改动 |
|---|---|---|
| `crates/db/src/manager.rs` | `export_data_with_progress` | 全路径释放改造 |
| `crates/db/src/manager.rs` | `export_data_with_progress_sync` | 全路径释放改造 |
| `crates/db/src/manager.rs` | `import_data` | 全路径释放改造 |
| `crates/db/src/manager.rs` | `import_data_with_progress_sync` | 全路径释放改造 |
| `crates/db/src/manager.rs` | `with_plugin_session_db!` 宏 | 全路径释放改造 |
| `crates/db/src/manager.rs` | `create_session` AsyncApp 版本 | 全路径释放改造（如果存在类似模式） |

### 测试
1. 单测：模拟 `get_session_connection` 失败 → 验证 `release_session` 被调用（可用 mock 或新增 `SessionManager` trait 抽象）
2. 单测：模拟 `release_session` 失败 + 业务成功 → 验证返回 release 错误
3. 单测：模拟 `release_session` 失败 + 业务失败 → 验证返回业务错误（release 失败仅 warn）
4. 现有 `cargo test -p db --lib` 通过

## 风险与回滚

| 风险 | 缓解 |
|---|---|
| 改造后错误信息变化（部分场景下返回的错误从 release 错误改为业务错误） | 是预期改进（业务错误更准确）；如有 UI 显示错误，需校验用户体验 |
| `log::warn!` 调用需要 `log` crate 在 manager.rs 中可见 | db crate 已用 `tracing`（manager.rs:12 附近），改用 `tracing::warn!` |
| 涉及 5+1 处改动，需要逐一 review | 抽公共 helper 减少重复（评审 P2-3 提及的可维护性问题） |

## 实施步骤

1. ⏳ 在 `with_plugin_session_db!` 宏和 4 个函数中应用"全路径释放"模式
2. ⏳ 抽公共 helper 函数（可选，与 P2-3 `_sync` 重构合并）
3. ⏳ cargo check / cargo test 验证
4. ⏳ 外部评审（Round 1 → APPROVED 关闭）
5. ⏳ 用户实测：MySQL/SQLite/PG 转储 SQL + 表导入导出功能正常
6. ⏳ commit + 归档

## 备注

- 既有问题（不是 panic 修复引入），但 panic 修复重排了相关代码正好是修复时机
- 与评审 P2-3（`_sync` 重构 + 公共骨架抽取）有协同：先修 P1-2 验证模式，再做 P2-3 重构抽取公共代码
- 评审 P3-8（guard 跨 await 锁类型确认）和 P3-9（取消语义）可在本任务内一并验证
