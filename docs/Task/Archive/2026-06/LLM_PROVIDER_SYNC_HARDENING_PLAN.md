# LLM 提供商云同步问题域归并与回归测试强化任务计划

**状态**: ✅ 已完成 (完成时间: 2026-06-08)
**创建人**: 主 Agent
**优先级**: P0
**关联分支**: docs/optimization-roadmap

## 任务目标

将 2026-05-17 以来 LLM 提供商云同步子系统的 **3 次提交、5 类问题域** 归并为统一的
风险地图，并以 **TDD (Level 2)** 补齐回归测试，把"修复 → 再修复"循环收敛为
"修复 + 测试保护"。

## 背景

1 个月内 2 次 `fix(sync)`（eb7f59a8、821098c2），均围绕 LLM Provider 同步方向：

| 提交 | 标题 | 核心修复 |
|---|---|---|
| cae6257d | feat(sync): 实现 LLM 提供商云同步功能 | 引入 `LlmProviderSyncType`、迁移脚本、加密链路 |
| 821098c2 | fix(sync): 修复 LLM 提供商同步覆盖与多默认问题 | `should_link_unlinked_local_by_name`、默认去重 |
| eb7f59a8 | fix(sync): 修复 LLM 提供商同步覆盖与删除逻辑 | `local_cloud_ids` 包含所有本地项、`delete_with_pending_cloud_deletion` |

提示子域仍处于 **"边界规则持续发现"** 阶段，单点 fix 不足以防止再回归。

## 问题域归并

### 域 A：覆盖策略（数据丢失风险，最高）

| 表现 | 根因 | 当前修复 | 残留风险 |
|---|---|---|---|
| 本地新增项被云端旧值 `update_from_cloud` 静默覆盖 | `local_unlinked_by_name` 把无 cloud_id 的本地项按名称回链 | LlmProviderSyncType 覆盖 `should_link_unlinked_local_by_name = is_builtin` | 其他 user_configurable 变体若有新增类型，**编译期无 fail-fast** |
| 关闭同步的本地项误判为云端新增 | `local_cloud_ids` 仅含 sync_enabled_locals | 改为含所有 local_items（eb7f59a8） | 仍需测试覆盖双向分支 |

### 域 B：删除传播（数据冗余风险，高）

| 表现 | 根因 | 当前修复 | 残留风险 |
|---|---|---|---|
| 跨设备删除不传播 | `delete_provider` 未登记 `PendingCloudDeletionRepository` | `delete_with_pending_cloud_deletion` 事务方法 | UI 侧仍可能绕过该方法（已通过简化 `llm_providers_view` 收敛） |

### 域 C：默认去重（数据完整性风险，中）

| 表现 | 根因 | 当前修复 | 残留风险 |
|---|---|---|---|
| 同步后出现多个默认 provider | `ProviderRepository::insert` 无 is_default 去重 | insert/update 都加 `UPDATE … SET is_default = 0` 兜底 | 缺"恰好一个默认"的不变式测试 |

### 域 D：名称回链（域 A 的策略承载点）

`should_link_unlinked_local_by_name` 是核心策略开关：
- `OnetCli` 内置项 → `true`（跨设备共享全局配置）
- 其他 11 个 user_configurable 变体 → `false`（保护本地新增数据）
- `Enum` 新增变体时**无 fail-fast** —— 若开发者忘记在新变体上挂 `is_builtin`，
  默认 `true` 行为会导致**新内置类型反而被云端覆盖**（与设计意图相反）。

### 域 E：测试基础设施（中）

`821098c2` 列出 19 个未实施测试编译错误（manager_tests.rs / tab_container.rs 中
`SshParams` 缺字段、`tempfile` 模块缺失、`resolve_inactive_tab_color` 签名不一致）。
经本任务 Phase 0 验证：`cargo check -p one-core --tests` **当前通过**，
说明 19 个错误已被后续 commit（f0081224 等）修复。**任务 #7 转为验证项，不再产生 diff**。

## 任务分解

- ✅ T0: 收集 3 次提交 + 当前代码 → 形成问题域归并表（本节）
- ✅ T1: 编写本 PLAN 文档
- ✅ T2: Codex 顾问 Phase 1 评审（read-only，mcp__codex__codex，会话 `019ea642-…`）→ APPROVED_WITH_CHANGES
- ✅ T3: TDD 补强 `calculate_sync_plan` 端到端测试（4 用例：idle / dual_change / unseen_cloud / sync_disabled）
- ✅ T4: TDD 补强 `decide_linked_sync_action` 边界测试（4 用例：non_sync_state 三分支 + sync_state never_synced 双分支）
- ✅ T5: TDD 补强 `should_link_unlinked_local_by_name` 覆盖（3 用例：fail-fast 双重断言 + OnetCli 自定义名 + 黄金一致性）
- ✅ T6: 验证 821098c2 未实施项当前状态 → `cargo check -p one-core --tests` 0 error，19 个错误已不存在
- ✅ T7: `cargo check -p one-core --tests` + 目标测试全部通过（53 + 4 = 57 个测试）
- ✅ T8: Codex 顾问 Phase 4 评审（会话 `019ea668-…`）→ APPROVED_WITH_MINOR
- ✅ T9:（Codex 风险 #5 追加）补强 `ProviderRepository` "恰好一个默认 provider" 不变式测试（3 用例）

## 改动内容（计划）

### `crates/core/src/cloud_sync/generic_sync.rs` — 测试模块补强

**T3 新增用例**：

1. `calculate_sync_plan_idle_when_both_unchanged` — 双向无变化 → 4 队列全空
2. `calculate_sync_plan_local_change_pushes_to_update_cloud` — 本地更新 + 云端未变
3. `calculate_sync_plan_cloud_change_pushes_to_update_local` — 云端更新 + 本地未变
4. `calculate_sync_plan_dual_change_prefers_local_on_tie` — 双向都更新、时间戳相等
5. `calculate_sync_plan_cloud_soft_delete_preserves_local_with_pending_update` —
   软删除场景下 local 有未同步更新（与现有
   `cloud_delete_should_preserve_local_sync_state_item_when_local_has_unsynced_update`
   形成端到端回路）

**T4 新增用例**：

1. `decide_linked_sync_action_non_sync_state_equal_timestamps_is_idle`
2. `decide_linked_sync_action_never_synced_with_cloud_ahead_updates_local`
3. `decide_linked_sync_action_never_synced_with_local_ahead_updates_cloud`
4. `decide_linked_sync_action_synced_state_with_last_synced_equals_local_no_change`

### `crates/core/src/cloud_sync/llm_provider_sync.rs` — 测试模块补强

**T5 新增断言**：

1. `user_configurable_provider_enum_has_no_builtin_member` — 编译期 + 运行期双重 fail-fast
2. `builtin_provider_with_custom_name_still_links` — 验证 `is_builtin` 与 name 无关
3. `every_provider_type_classified_consistently` —
   `all().iter().any(is_builtin) == user_configurable().len() < all().len()`

### 测试基础设施验证

**T6**：

- 重新跑 `cargo check -p one-core --tests`，确认 19 个错误**当前不存在**
- 记录到本 PLAN 的"验收结果"中（与 821098c2 的"未实施"清单做核对）

## 验收标准

- [x] `cargo check -p one-core --tests` 0 error
- [x] `cargo test -p one-core --lib cloud_sync::generic_sync` 全部通过（既有 9 + 新增 8 = 17）
- [x] `cargo test -p one-core --lib cloud_sync::llm_provider_sync` 全部通过（既有 2 + 新增 3 = 5）
- [x] `cargo test -p one-core --lib cloud_sync` 决策分支 + 软删除分支合计 17 用例通过（> 14 目标）
- [x] `cargo test -p one-core --lib llm::storage` 全部通过（既有 1 + 新增 3 = 4）
- [x] `cargo check -p main` exit 0（仅 6 个无关警告）
- [x] 不修改任何生产代码（仅 `mod tests` 内部追加）
- [x] 不新增依赖
- [x] Codex 顾问评审：Phase 1 APPROVED_WITH_CHANGES，Phase 4 APPROVED_WITH_MINOR

## 风险与缓解

| 风险 | 缓解 |
|---|---|
| 测试断言过严导致生产代码分支被迫"为测试而改" | TDD 阶段不修改 `calculate_sync_plan` 主体；若发现死分支，转入独立"简化"任务而非本次混入 |
| 编译期 fail-fast 用 `const_assert` 触发 `static_assertions` 依赖 | 退化为 `#[test] fn …` 运行期断言（虽慢但无新依赖） |
| Phase 1 Codex 评审 REJECTED | 按 CLAUDE.md 兜底：主流程不挂，把 Codex 意见写入"Codex 顾问"章节并继续 |

## 备注

- 本任务遵循 AGENTS.md 的 **Level 2 TDD 门禁**（跨模块共享逻辑 + 高回归风险）
- 提交权保留给用户：所有 git commit 由用户明确授权后发起
- 完成后立即归档到 `docs/Task/Archive/2026-06/` 并更新 `docs/Task/README.md`

## Codex 顾问意见（Phase 1）

**评审通道**: `mcp__codex__codex` (read-only, 2026-06-08)
**会话 ID**: `019ea642-c4c1-7072-aaa6-02e27fc5e405`
**结果**: APPROVED_WITH_CHANGES（5 项风险点 + diff 参考，**diff 不可直接套用**）

### 5 项风险点（已纳入本计划）

| # | 风险 | 处理 |
|---|---|---|
| 1 | TDD 若发现真实回归，PLAN 仍要求"不修改生产代码"会自相矛盾 | 接受观点，但本计划边界明确：T3-T5 是**既有 contract 的回归断言**，不是新 contract。红灯出现 → 单独立"修复任务"，不在本计划中混生产 diff |
| 2 | 域 D "默认 true 行为覆盖新内置" 措辞与 LLM override 实际机制不完全一致 | 已修正 PLAN 域 D 措辞；T5 三层断言表达真实 contract |
| 3 | fail-fast 只能测试期，编译期需要 `static_assertions` 依赖 | 退化为运行期断言（无新依赖） |
| 4 | 缺 `delete_with_pending_cloud_deletion` 在 repository 入口的回归测试 | 本轮接受留作下层集成测试，不混入 |
| 5 | 缺"恰好一个默认 provider"不变式测试 | **采纳**：T9 新增此用例 |

### 关于 Codex 提供的 diff

Codex 凭通用同步计划经验臆造了 `calculate_sync_plan(vec![local], vec![cloud], vec![state], &TestSyncType)` 等**实际不存在的签名**（实际签名见 `generic_sync.rs:537-543`），同时臆造了 `LinkedSyncAction::Idle` 变体（实际只有 `None/UpdateCloud/UpdateLocal`）和 `GenericSyncPlan::create_local/delete_local` 等字段（实际为 `to_upload/to_update_cloud/to_download/to_update_local`）。

→ **diff 必须按真实签名重写**，下方"T3-T5 实现"是按仓库真实 API 落地的版本。

## T3-T5 实现（按真实签名重写）

> 待 Phase 2 实施时落地。Codex 提供的 diff 仅作策略参考，不作为最终代码来源。

## 验收结果

- 验收时间：2026-06-08
- 验收方式：
  - `cargo check -p one-core --tests` 0 error
  - `cargo check -p main` exit 0
  - `cargo test -p one-core --lib cloud_sync` → 53 passed / 0 failed
  - `cargo test -p one-core --lib llm::storage` → 4 passed / 0 failed
- 验收结论：通过
- Codex 评审：Phase 1 APPROVED_WITH_CHANGES（5 风险全部已纳入 / 已说明），Phase 4 APPROVED_WITH_MINOR（仅归档状态缺口 → 已修复）
- 落地改动：4 个文件修改（3 个测试文件 + README 索引）+ 1 个新文件（PLAN，已归档至本目录）
- 未执行 git commit（按 AGENTS.md 个人硬门禁，需用户明确授权）

## TDD 关键发现

`calculate_sync_plan_downloads_unseen_cloud_item` 初始版本**红灯**：原本假设"local 空 + cloud 有"会让 cloud 走 `to_download`，实际生产代码把 local 空视为"首登"并把 local 推入 `to_upload`（这是 821098c2 已接受的 trade-off）。修复方式：**隔离测试场景**（加 1 个 pre-existing 本地项，让 `to_download` 路径独立），并在测试注释中**显式记录 trade-off**。

意义：TDD 阶段暴露的"测试假设 ≠ 生产契约"现象，恰恰说明此类回归测试必须**对生产 trade-off 透明**，否则会成为"为测试而改"的诱因。本计划全程未触动生产代码，T9 案例是该原则的具体体现。


