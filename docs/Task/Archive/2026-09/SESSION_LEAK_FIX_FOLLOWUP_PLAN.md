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

## 最终状态

- **测试**：39/39 manager 测试通过（含 7 个新测试）
  - `restore_session_with_downgraded_action_slot_write_semantics` (已重命名)
  - `release_intent_resolves_stored_or_override`
  - `compute_writeback_action_matrix`
  - `compute_writeback_action_literal_spec`（字面量规格表 12 格）
  - `write_back_session_slot_semantics`
  - `write_back_session_slot_rejects_double_write_in_debug`（cfg(debug_assertions)）
  - `compute_writeback_action_output_domain_excludes_release_for_reuse`
- **三层守卫**：编译期穷尽 + 运行期规格 + 显式不变量
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