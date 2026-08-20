# AI 终端操作员假性终结问题诊断

**Status**: ⏳ 待完善（已记录可观测性增强，根因修复待方案设计）
**创建时间**: 2026-08-21
**关联**: [`AI_TERMINAL_OPERATOR_PLAN.md`](Archive/2026-08/AI_TERMINAL_OPERATOR_PLAN.md)

---

## 1. 问题描述

用户实测反馈：AI 终端操作员在多轮任务中偶尔会自动停止，需要用户重新提醒（输入新消息）才会继续后续命令。

### 已知触发场景

- **场景 A**：模型在多轮 `write_to_terminal` 后返回 text-only 总结而未调用 `task_complete`（prompt 纪律问题）
- **场景 B**：`max_tokens` 截断导致 `finish_reason=length`，模型想继续但被截断（生成上限问题）
- **场景 C**：模型 reasoning-only delta 被截断（chunked response 不完整）

三者都最终走向同一条路径：`terminal_operator.rs:151-157` 的 `if outcome.tool_calls.is_empty() { break; }` 终止循环。

---

## 2. 根因分析（已确认）

**文件**：`crates/terminal_view/src/agents/terminal_operator.rs:151-157`

```rust
if outcome.tool_calls.is_empty() {
    // 首轮空响应可能是模型预热问题，重试一次；其后视为正常结束
    if round == 1 && outcome.text.is_empty() {
        continue;
    }
    break;
}
```

**关键设计意图**：`task_complete` 工具调用是唯一明确的完成信号；text-only / 空响应一律视为"主动完成"。此设计在大多数场景下合理（如 `write_tool_call_round_trip` 测试），但在某些场景下与模型行为冲突。

**根因矛盾**：
- 严格遵守 `task_complete` → 模型偶尔忘记调用 → 循环提前终止
- 改用 text-only = 主动完成 → 模型本想继续但受截断/截断 chunk 影响 → 循环提前终止

---

## 3. 已实施的可观测性增强（2026-08-21）

由于"续轮"修复会破坏现有契约（与 `write_tool_call_round_trip` 等测试场景冲突），本轮仅增强可观测性：

**改动**：
- `RoundOutcome` 增加 `finish_reason: Option<String>` 字段（`terminal_operator.rs:67-71`）
- `collect_round` 收集 `finish_reason`（替代原 `break` 模式，正确归集到 Result）
- `run_loop` 的 debug 日志增加 `finish_reason={:?}` 输出
- 新增 `had_tool_in_session` 跟踪：在已有 tool_call 后的 text-only 终止打印 `tracing::warn!`
- `docs/Usage/AI_TERMINAL_OPERATOR.md` 增加"限制"章节说明排查指引

**预期效果**：
下次用户遇到"自动停止"时，开发可通过日志中的
```
[terminal_agent] 第 N 轮已执行过 tool_call 后返回 text-only (text=XB, finish_reason="length")
```
快速定位：
- `finish_reason="stop"` → 模型主动总结（prompt 纪律问题）
- `finish_reason="length"` → max_tokens 截断（生成上限问题）
- `finish_reason="tool_calls"` → 异常（模型声称调工具但未给）

---

## 4. 待完善的修复方案（选项）

### 方案 A：续轮 + reminder 注入（激进）

**思路**：text-only 触发 reminder 注入催模型继续，最多 N 轮。

**优点**：
- 真正自动修复（不依赖用户提醒）

**缺点**（已验证）：
- 与现有 `write_tool_call_round_trip` 等 3+ 个测试场景冲突
- 改变 loop 控制流，依赖模型配合（无法在测试中验证）

**状态**：❌ 已驳回（用户决策 2026-08-21）

### 方案 B：prompt 强化 + 模型行为引导

**思路**：在 `prompt.rs` 明确"text-only 响应不是完成信号，必须调用 `task_complete` 才算完成"，并增加反例。

**优点**：
- 不改 loop 逻辑，零回归风险
- 治本（让模型形成新习惯）

**缺点**：
- 模型未必遵守（取决于 base model 和 fine-tuning）
- 无法在单元测试中验证

**状态**：⏳ 待评估

### 方案 C：UI 层 / 持久化层诊断

**思路**：用户报告的"需要重新提醒才能继续"可能不是 Agent 循环 bug，而是：
- ChatDB 加载历史时 assistant 消息的 `omnihub-tool` 记录被截断
- 下一轮 AgentContext 重建时丢失了部分 tool_call 上下文
- UI 侧 Completed 事件处理

**优点**：
- 可能发现真正的根因

**状态**：⏳ 待调研

### 方案 D：自适应终止判定

**思路**：结合多个信号判定是否完成：
- `task_complete` 工具调用（高置信）
- text-only + round==1（首轮空响应重试）
- text-only + 文本中包含"已完成/完成/finish/done"等关键词（启发式）
- text-only + finish_reason=length 且 had_write=true（截断）

**优点**：
- 更智能，能覆盖大多数场景

**缺点**：
- 启发式不可靠（LLM 文本变化多端）
- 复杂度高

**状态**：⏳ 待评估

---

## 5. 验证现状

```
cargo test -p terminal_view --lib agents → 21 passed（0 失败）
cargo fmt -p terminal_view -- --check → clean
```

---

## 6. 后续行动

1. **短期**：通过可观测性增强，收集实际"自动停止"案例的日志；判断是 prompt 纪律问题还是 max_tokens 截断问题
2. **中期**：根据收集数据决定走方案 B（prompt 强化）或方案 D（自适应终止）
3. **长期**：如方案 B/D 仍不足，调研方案 C（UI/持久化层）
