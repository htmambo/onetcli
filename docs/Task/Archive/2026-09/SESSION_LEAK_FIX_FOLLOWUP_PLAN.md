# 数据库 session 生命周期 follow-up 改进

## 背景

session_leak_fix_plan 主任务（7 阶段全部完成 + 外部评审 Round 6/11/14 verdict=APPROVED）后，继续 follow-up 改进 5 轮外部评审迭代（Round 15-20），系统性收紧 `SessionGuard::finish_with` / `finish` / Drop 三入口的 Err 路径写回契约、文档化设计决策、添加测试矩阵。

## 完成项

### Round 15 → Round 16（commit 5d6eabe7）
- P0-2 action 参数二义性：用 `Option<action_override>` 区分 finish vs finish_with
- P0-1 Err 路径写回契约：提取 `restore_session_with_downgraded_action` 纯函数 + 单测
- 引入 `ReleasePath` 枚举（FinishWith / Finish）+ `as_str()` 替代字符串字面量

### Round 16 → Round 17（commit a2a3e0ae）
- P2-2 类型安全收尾：合并 `ReleasePath` + `Option<action_override>` 为 `ReleaseIntent` 枚举
  - `Finish` / `FinishWith(SessionReleaseAction)`
  - `resolved_action(stored)` 方法
- P1-1 决策方案 B（override 视为最新意图）：doc 显式说明 + 单测钉死

### Round 17 → Round 18（commit dd2e35fb）
- P1-1/P2-1/P2-2：12 组合矩阵测试钉死接线契约
- P2-2 文档去重：enum doc 指向 perform_release doc（权威来源）
- P3-1 双重计算：提取 `compute_writeback_action(intent, stored)` 单一计算点
- P3-2 日志：补降级前 action + 降级后 action
- P3-3 release 模式契约可观测：debug_assert + 无条件 warn

### Round 18 → Round 19（commit b758ad29）
- P3-1 重命名：`restore_session_with_downgraded_action` → `write_back_session_slot`
  - 函数语义从"降级 + 写回"简化为"纯 slot 写入"
- P2-2 字面量规格表：12 格完整规格测试（即使 downgrade 回归也失败）
- P2-1 双层保护推广到 write_back_session_slot（debug_assert + error!）
- perform_release doc 明确 Err 路径覆盖说明 + 未来 seam 建议

### Round 19 → Round 20（commit 257a7e87）
- P2-A 关键发现文档化：`ReleaseForReuse` 永久塌缩为 `Close` 的设计依据
  - 业务成功后连接可信，失败后状态不可信，禁止兑现复用标记
- ⚠️⚠️ 标注修正：原 ⚠️ 标错对象，移到 RfR 行
- P3-B 完备性守卫：12 格覆盖完整性校验（防止新增变体时静默欠覆盖）
- P3-A Drop 入口统一：使用 `compute_writeback_action(Finish, stored)` 而非直调 downgrade
- P3-D 违约测试：cfg(debug_assertions) 下验证非空 slot 触发 panic

### Round 20 → Round 21（commit 869e1479）
- P2-1 设计依据定式收紧："失败⇒脏"升级为统一原则
  - "本函数无法提供'连接干净'的正面证明，而 RfR 的兑现恰恰需要正面证明，
    因此一律不兑现"——同时覆盖失败重试与隐式 Drop
  - 塌缩目标是 Close 而非 Release 的论证
- P2-3 守卫唯一性：any → count == 1（既防漏又防重复/冲突）
- P3-3 不变量命名测试：单独钉死"输出 ⊆ {Close, Release}"

### Round 21 → Round 22（commit d5130f75）
- P2-2 Drop ≡ Finish 等价特征测试：新增 `drop_release_action` 访问器
  - Drop 与测试共用同一函数（先快照再 take session，避免借用冲突）
  - 钉死三种 stored 的 Drop 映射（Close→Close、Release→Release、RfR→Close）
- P3-1 release 模式双重写行为：Drop doc 注释补说明

### Round 22 → Round 23（commit c9748c37）
- 新增 `DbError::code()` 方法：机器可读错误码（wire 格式稳定）
  - 8 个变体 → `db.<variant_snake_case>` 字符串
  - 与 `variant_tag()` 区分：code 是面向外部系统的稳定格式
  - 编译期保证：match 无 `_` 通配臂，新增变体时编译失败
- 新增 `db_error_code_is_stable_per_variant` 单测：钉死 code 全局唯一 + 纯函数

### Round 23 → Round 24（commit 7158209b）
- **取消安全修复**（Round 17 P3-5 backlog 关闭）：
  - 修复前：perform_release await 前 take session，mid-await 取消导致 session 永久泄漏
  - 修复后：as_ref 仅读副本，await 期间 session 仍 Some，Drop 可重试
  - Ok 路径：await 后才 take（取消窗口关闭后安全清空）
  - Err 路径：in-place mutation 替代 write_back_session_slot（保留 cancel-safe）
- 新增 `perform_release_cancel_safety_preserves_session` 测试

### Round 26（commit cf286dba）— backlog #1 release_fn seam 闭环
- **release_fn cfg(test) seam**：测试可通过 `set_test_release_result` 注入下一次 `release_session` 的返回值
- 一次性 take 语义 + thread-local + 编译期擦除
- 闭环 Round 17 P3-5 端到端 Err 测试
- 新增 2 个集成测试：
  - `perform_release_err_writes_back_downgraded_via_mock`
  - `perform_release_err_drop_retry_uses_downgraded_action`

### Round 27（commit 436a969b）— backlog #2 Drop panic 文档化
- Drop doc 注释新增"双重 panic 安全"章节
- 显式声明 Drop 路径不调任何 debug_assert + 未来变更警示

### Round 28（commit 165b9ac7）— backlog #3 verify_failure_count metric
- `ConnectionManager::verify_failure_count()` 公共 getter
- AtomicU64 + Relaxed 内存序
- verify 失败时递增 + warn 日志补字段
- 新增 `verify_failure_count_increments_on_verify_error` 测试

### Round 29（commit b753758a）— backlog #2 修复 Drop panic guard
- Drop 拆分为 `drop()` + `drop_inner()`
- `drop()` 用 `std::panic::catch_unwind` 包住 `drop_inner`
- 即使未来意外引入 panic 也仅记 error 而非升级为 abort

## 最终状态

- **测试**：45/45 manager 测试通过（含 11 个新测试）
  - `release_intent_resolves_stored_or_override`
  - `compute_writeback_action_matrix`
  - `compute_writeback_action_literal_spec`（字面量规格表 12 格）
  - `compute_writeback_action_output_domain_excludes_release_for_reuse`
  - `write_back_session_slot_semantics`
  - `write_back_session_slot_rejects_double_write_in_debug`（cfg(debug_assertions)）
  - `drop_release_action_matches_finish_for_all_stored`
  - `db_error_code_is_stable_per_variant`
  - `perform_release_cancel_safety_preserves_session`
  - `perform_release_err_writes_back_downgraded_via_mock`（mock 注入）
  - `perform_release_err_drop_retry_uses_downgraded_action`（mock 注入）
  - `verify_failure_count_increments_on_verify_error`
- **三层守卫**：编译期穷尽 + 运行期规格 + 显式不变量
- **取消安全**：perform_release mid-await 取消不再导致 session 永久泄漏
- **端到端 Err 测试**：cfg(test) mock seam 让 perform_release Err 路径可集成测试
- **panic guard**：Drop 内部意外 panic 不升级为 abort
- **可观测性**：DbError::code 提供稳定 wire 格式错误码（IPC/metric/告警键）
  + verify_failure_count metric 监控 verify 失败率
- **文档**：方案 B 钉死 + RfR 永久塌缩设计依据 + Drop ≡ Finish 等价性论证

## 文件变更

- 修改：crates/db/src/manager.rs（仅此一个文件）

## 测试状态

- [x] 单元测试通过（39/39 manager 测试通过）
- [x] 代码审查完成（Round 16-20 全部 APPROVED）

## Commits

| Commit | 内容 |
|---|---|
| 5d6eabe7 | perform_release 提取 + ReleasePath 路径枚举 |
| a2a3e0ae | ReleaseIntent 合并 + 方案 B 钉死 |
| dd2e35fb | 接线测试矩阵 + 文档去重 + 单点计算 |
| b758ad29 | write_back_session_slot 重命名 + 字面量规格 |
| 257a7e87 | RfR 设计依据 + Drop 入口统一 |
| 869e1479 | RfR 定式收紧 + 守卫唯一性 + 不变量测试 |
| d5130f75 | drop_release_action 访问器 + Drop ≡ Finish 等价测试 |
| c9748c37 | DbError::code 机器可读错误码 |
| 7158209b | perform_release 取消安全修复（Round 17 P3-5 backlog 关闭） |
| cf286dba | release_fn cfg(test) seam 闭环端到端 Err 测试 |
| 436a969b | Drop 双重 panic 安全不变式文档化 |
| 165b9ac7 | verify_failure_count metric（Round 28 可观测性） |
| b753758a | Drop panic guard（catch_unwind 防 abort 升级） |

## 已知局限（已文档化于 perform_release）

- release_session 通过 NotFound→Ok 转换使 Err 路径在生产中**几乎不可达**
- 端到端 Err 测试未实现（需引入 #[cfg(test)] release_fn seam 注入 mock）
- 当前 Err 分支靠**纯函数单测 + 源代码审计**保证

> OMC trailers:
> Constraint: 仅修改 crates/db/src/manager.rs
> Rejected: 保留隐式设计决策 + 无测试矩阵 + 文档冗余 | 后续读者判定为 bug 风险 + 接线回归无测试
> Directive: 外部评审 Round 15-20（5 轮迭代系统性收紧契约）
> Confidence: 高 | 39/39 单测通过 + 三层守卫 + 完整设计文档
> Scope-risk: SessionGuard 公共 API 签名不变；仅私有实现收紧
> Not-tested: release_session 实际返回非 NotFound 错误的 Err 路径（生产中 NotFound→Ok 转换使该路径几乎不可达）

完成于 2026-09-13