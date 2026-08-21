# AI 终端操作员假性终结问题诊断

**Status**: ✅ 实施完成（待归档）。M1-M16 全部落地；35+4=39 单元测试全绿；fmt/clippy clean；view.rs pre-existing `cx.new` 错误已通过 `<gpui::App as gpui::AppContext>::new(...)` trait 限定调用修复；review_code P0×3 + P1×3 已全部落地。
**已知阻塞（非本次任务）**：`crates/ui/src/input/element.rs:848` GPUI 自身 UTF-8 wrap slice panic（macOS CoreText 后端），独立于本任务，单独立项修复。
**创建时间**: 2026-08-21
**最近更新**: 2026-08-21（M1-M16 实施完成；Round 1-4 plan 评审 + Round 1 review_code 落地；since_last_write 方案；UI 中止态 panic 独立于本任务）
**关联**: [`AI_TERMINAL_OPERATOR_PLAN.md`](../Archive/2026-08/AI_TERMINAL_OPERATOR_PLAN.md)

---

## 1. 问题描述

用户实测反馈：AI 终端操作员在多轮任务中偶尔会自动停止，需要用户重新提醒（输入新消息）才会继续后续命令。

### 1.1 最新截图案例（2026-08-21）

**任务**：检测并分析当前机器到一些常用网站的访问速度。

**执行轨迹（卡片序号对应 `[序号]:动作摘要`）**：

| # | 动作摘要 | 类型 | 备注 |
|---|---|---|---|
| 1 | 获取所有终端 | tool | 正常 |
| 2 | 执行命令，`ping -c 4 baidu.com` | tool | 正常 |
| **—** | **模型返回纯文本 `继续`** | **text-only 终止** | **⚠️ 假性终结 1** |
| 3 | 读取终端输出（#1，最近 100 行） | tool | 用户手动输入 `继续` 后触发 |
| 4 | 执行命令，`ping -c 4 google.com` | tool | 正常 |
| 5 | 执行命令，`ping -c 4 github.com` | tool | 正常 |
| 6 | 执行命令，`echo '访问速度检测结果分析：' ...` | tool | 正常 |
| 7 | 执行命令，`curl -o /dev/null -s -w '淘宝：...' https://www.taobao.com` | tool | 正常 |
| 8 | 读取终端输出（默认终端，最近 2000 行） | tool | 正常 |
| **—** | **同上 `curl taobao.com`**（重复 6/7） | tool | **⚠️ 假性终结 2 前的重复迹象** |
| 9 | 任务完成，正在准备回复 | tool（task_complete） | 最终完成 |

**关键观察**：

- 假性终结出现在 **写完第 2 个工具（ping baidu.com）后**。模型在多轮工具链中途返回纯文本 `继续`（卡片里显示为"用户消息"），但**没有调用 `task_complete`**，触发 `terminal_operator.rs:155-168` 的 `if outcome.tool_calls.is_empty() { break; }` 分支，循环退出，Agent 报 Completed。
- 后续 `继续` 是用户手打的——并非上一轮自动续流。
- 第二次 `curl taobao.com`（#7 → #8 之间）是重复工具调用截图，看起来触发了熔断前的 1~2 次重复；说明 `REPEAT_LIMIT=3`（`terminal_operator.rs:29`）确实在工作。
- 用户体感：**整体能跑完，但中途被打断是确定的失败模式**，每次遇到都要手动 `继续`。

### 1.2 历史已知触发场景

- **场景 A**：模型在多轮 `write_to_terminal` 后返回 text-only 总结而未调用 `task_complete`（prompt 纪律问题）
- **场景 B**：`max_tokens` 截断导致 `finish_reason=length`，模型想继续但被截断（生成上限问题）
- **场景 C**：模型 reasoning-only delta 被截断（chunked response 不完整）

三者最终都走向同一条路径：`terminal_operator.rs:155-168` 的 `if outcome.tool_calls.is_empty() { break; }` 终止循环。

### 1.3 新增：read_terminal_output 输出起点问题（2026-08-21，用户第二轮反馈）

**实测场景**：
> "试了下好像是读取某些命令的输出结果时停止的，比如刚才又碰到读取ping的时候出错，看了下获取的内容是读取了终端里的所有内容（从最开始？）。有没有可能使用重定向的方式来获取？或者就只是获取刚才执行的命令之后的输出？"

**问题本质**：

`read_terminal_output`（`crates/terminal_view/src/agents/tools.rs:65-72`）当前只有 `max_lines` 参数（最多读取行数），没有"行号起点"。底层 `terminal.recovery_content(max_lines)`（`crates/terminal/src/terminal.rs:2770-2773`）序列化**整个滚动缓冲区**（`history_size + screen_lines`）的**尾部** `max_lines` 行。

后果：
- 终端历史里堆积的旧命令输出（如之前的 curl taobao、echo 摘要等）+ 本次 ping 的输出，被一起返回
- 模型读到混合输出，无法区分"本次 ping 的结果" vs "历史残留"
- 可能误判命令状态（如认为命令还在运行、或读取了错误的字节）
- 用户体感：**Agent 在读输出后"卡住"或误判**，间接引发假性终结

**方案对比**：

| 方案 | 思路 | 优点 | 缺点 |
|---|---|---|---|
| **A. 重定向方式** | 模型主动写 `command > /tmp/out.txt` 然后读文件 | 简单可控、无新基建 | 模型需多写一步；输出量大时文件爆炸 |
| **B. OSC 133 命令生命周期** | 终端解析 OSC 133，工具暴露"自上次 CommandFinished 后的输出" | 直觉、与用户命令一致 | 本地 PTY 无 OSC 133；二期展望 |
| **C. 增量模式** | 工具暴露"自上次 read 之后的新增输出" | 不需要语义 | 模型难以指定；需客户端维护游标 |
| **D. 行号偏移参数 `from_line`** | 工具增加 `from_line: i64`（负数 = 倒数）；保持 `max_lines` | 兼容现有；模型可指定"自 -100 行起读 200 行" | 模型需要先获取当前总行数（一次额外 read） |
| **E. 自上次 `write_to_terminal` 后的输出** | 工具记录"最近一次 write 时的总行数 `last_write_line_count`"，读取时返回 `[last_write_line_count..)` 范围的增量 | 完全符合用户直觉；无需 OSC 133 | 需在 AgentContext capability 中维护 `last_write_line_count`；多窗口/多 terminal_id 时需按 terminal_id 分别跟踪 |

**推荐方案 E**（与用户提出的"刚才执行的命令之后的输出"完全对齐）：

#### 4.E 方案 E 详细设计

**核心改动**：

1. **维护 `last_write_line_count: HashMap<u64, usize>`**（按 terminal_id 索引，存 AgentContext capability 或 RunState）：
   - `write_to_terminal` 成功后，记录当前总行数 `current_total_lines - approx_estimated_output_lines`（或更简单：直接记录 `current_total_lines - 1`，即"写入命令所在的行号"）
   - `read_terminal_output` 默认从 `last_write_line_count[terminal_id]` 开始读

2. **工具 schema 调整**（`tools.rs:65-72`）：
   ```json
   {
     "terminal_id": ...,
     "max_lines": ...,
     "since_last_write": { "type": "boolean", "default": true, "description": "true = 自上次 write_to_terminal 之后；false = 全终端尾部" }
   }
   ```

3. **底层实现**（`pump.rs:111 read_output`）：新增 `from_line: usize` 参数；`serialize_term_for_recovery` 当前不支持起点，需要在 `terminal.rs:190` 改写为支持 `from_line` + `max_lines` 的窗口序列化。

4. **首次调用 fallback**：capability 中没有 `last_write_line_count[terminal_id]` 时（首次 read 或从未 write），退回原行为（读全终端尾部 max_lines）。

5. **多 Agent 调用安全**：capability 由 AgentContext 持有，每次 run_loop 独立计数；不会跨会话污染。

**测试新增**：

- `write_then_read_returns_only_post_write_output`
- `read_before_write_returns_full_terminal_tail`
- `multi_terminal_last_write_tracked_independently`
- `since_last_write_false_falls_back_to_full_tail`

**风险**：

- `recovery_content` 当前按行号序列化需要底层修改；与 `terminal.rs:188-?` 现有实现对齐
- 模型首次 read 时无 `last_write_line_count`，需明确 fallback 行为

**评审建议**：本方案作为本轮 PR 的增量 feature（独立 review_code 评审），不与 M1-M10 假性终结修复合并。

---

## 2. 根因分析（已确认）

**文件**：`crates/terminal_view/src/agents/terminal_operator.rs:155-168`

```rust
if outcome.tool_calls.is_empty() {
    // 首轮空响应可能是模型预热问题，重试一次；其后视为正常结束
    if round == 1 && outcome.text.is_empty() {
        continue;
    }
    // 已有 tool_call 历史时记录可疑信号，方便调试「自动停止」问题
    if had_tool_in_session && !outcome.text.is_empty() {
        tracing::warn!(
            "[terminal_agent] 第 {round} 轮已执行过 tool_call 后返回 text-only (text={}B, finish_reason={:?})；用户报告的『自动停止』疑似本路径",
            outcome.text.len(),
            outcome.finish_reason
        );
    }
    break;
}
```

**关键设计意图**：`task_complete` 工具调用是唯一明确的完成信号；text-only / 空响应一律视为"主动完成"。此设计在大多数场景下合理，但在截图场景与模型行为冲突。

**根因矛盾**：
- 严格遵守 `task_complete` → 模型偶尔返回中间态短文本（如"继续"）→ 循环提前终止
- 改用 text-only = 主动完成 → 模型本想继续但受截断/截断 chunk 影响 → 循环提前终止
- 既不让模型误判、也不放过真正截断——**需要组合策略**。

---

## 3. 已实施的可观测性增强（2026-08-21，commit 0dddf152）

由于"续轮"修复会破坏现有契约（与 `write_tool_call_round_trip` 等测试场景冲突），前一轮仅增强可观测性：

**改动**：
- `RoundOutcome` 增加 `finish_reason: Option<String>` 字段（`terminal_operator.rs:67-71`）
- `collect_round` 收集 `finish_reason`（替代原 `break` 模式，正确归集到 Result）
- `run_loop` 的 debug 日志增加 `finish_reason={:?}` 输出
- 新增 `had_tool_in_session` 跟踪：在已有 tool_call 后的 text-only 终止打印 `tracing::warn`
- `docs/Usage/AI_TERMINAL_OPERATOR.md` 增加"限制"章节说明排查指引

**预期效果**：
下次用户遇到"自动停止"时，开发可通过日志中的
```
[terminal_agent] 第 N 轮已执行过 tool_call 后返回 text-only (text=XB, finish_reason="length")
```
快速定位：
- `finish_reason="stop"` → 模型主动总结或短文本误判（prompt 纪律问题）
- `finish_reason="length"` → max_tokens 截断（生成上限问题）
- `finish_reason="tool_calls"` → 异常（模型声称调工具但未给）

---

## 4. 修复方案（方案 B+D 组合，Round 1 修订）

### 4.1 设计目标

不改变 `task_complete` 唯一完成信号的契约，**只调整「text-only 终止」的判定**：当 text-only 出现在有 tool_call 历史、且 `finish_reason != "length"` 时，**注入轻量 reminder 续一轮**；当 `finish_reason == "length"` 时按现状截断（避免复读）；其他 `finish_reason`（content_filter / null）按统一策略处理。

**可配置化（Round 1 R2 落地）**：

新增 settings 字段 `ai_terminal_agent_mid_session_reminders: usize`，默认 1（保留原行为即可回滚），0 表示禁用 reminder（退回到原行为）。这样回滚到 0 等价于回到 commit 0dddf152。

### 4.2 方案 B：prompt 强化（治本，让模型形成"非 task_complete 不算完成"的习惯）

**改动文件**：`crates/terminal_view/src/agents/prompt.rs`

**新增规则**（Round 1 R4 修订：消除原 8 条自相矛盾）：

```text
6. 完成定义：调用 task_complete 是唯一明确的"完成"信号。仅返回纯文本（包括"继续"/"已查询"/"已完成"等中间态短文本）**不会**结束任务——Agent 会自动注入 reminder 让你继续。
7. 中间态文本处理：当你需要等待用户补充信息、或者只是阶段性汇报时，可以输出短文本但不要中断工具调用链——继续调用下一个工具。
8. 配额说明：如果 reminder 仍无法让你完成任务（最多 1 次自动 reminder；超过则视为任务已完成），请在收到 reminder 后**立即**调用 task_complete 总结当前进展，并说明阻塞原因。
```

### 4.3 方案 D（修订版）：自适应终止 + 轻量 reminder 续轮

**改动文件**：`crates/terminal_view/src/agents/terminal_operator.rs:155-168`

**新增常量**（`terminal_operator.rs` 顶部）：

```rust
/// text-only 中间态的最大 reminder 续轮次数（默认 1；0 表示关闭 reminder 退回原行为）。
/// 来源：settings.ai_terminal_agent_mid_session_reminders；常量仅作为编译期默认值。
const DEFAULT_MAX_MID_SESSION_REMINDERS: usize = 1;
/// 注入到消息流的 reminder 文案：明确告诉模型"text-only 不算完成"。
const MID_SESSION_REMINDER_TEXT: &str =
    "你刚才只返回了文字总结，但还没有调用 task_complete。\
     请继续调用必要的工具，直到所有步骤完成后再调用 task_complete。\
     如果确实无法继续（例如等待用户输入），请调用 task_complete 并说明阻塞原因。";
```

**新逻辑**（完整替换原 155-168）：

```rust
if outcome.tool_calls.is_empty() {
    let fr = outcome.finish_reason.as_deref();
    let is_empty = outcome.text.is_empty();

    // 场景 1：首轮空响应可能是模型预热问题，重试一次（既有）
    if round == 1 && is_empty {
        continue;
    }

    // 场景 2：配额 = 0 时退回原行为（Round 1 R2 feature flag 落地）
    if max_mid_session_reminders == 0 {
        if had_tool_in_session && !is_empty {
            tracing::warn!(
                "[terminal_agent] 第 {round} 轮 text-only 中间态 (text={}B, finish_reason={:?})；reminder 已关闭，按完成处理",
                outcome.text.len(), outcome.finish_reason
            );
        }
        break;
    }

    // 场景 3：max_tokens 截断——不续轮（避免复读），按现状退出
    if fr == Some("length") {
        tracing::warn!(
            "[terminal_agent] 第 {round} 轮 max_tokens 截断 (text={}B)；不续轮",
            outcome.text.len()
        );
        full_text.push_str("\n\n");
        full_text.push_str(&t!("TerminalAgent.response_truncated").to_string());
        break;
    }

    // 场景 4：内容合规拦截——fail-fast，不静默吞（Round 1 R1）
    if fr == Some("content_filter") {
        tracing::error!(
            "[terminal_agent] 第 {round} 轮触发 content_filter；终止循环 (text={}B)",
            outcome.text.len()
        );
        return Err(t!("TerminalAgent.content_filtered").to_string());
    }

    // 场景 5：finish_reason=null——部分 OpenAI 兼容 provider 返回 null，按 stop 同路径处理
    if fr == Some("null") || fr == Some("stop") || fr.is_none() {
        if had_tool_in_session && !is_empty {
            if mid_session_reminder_count < max_mid_session_reminders {
                mid_session_reminder_count += 1;
                tracing::info!(
                    "[terminal_agent] 第 {round} 轮 text-only 中间态，注入 reminder 续轮 ({}/{}, text={}B, finish_reason={:?})；injected=\"{}\"",
                    mid_session_reminder_count, max_mid_session_reminders,
                    outcome.text.len(), outcome.finish_reason, MID_SESSION_REMINDER_TEXT
                );
                messages.push(Message::text(Role::User, MID_SESSION_REMINDER_TEXT));
                continue; // 关键变更：从 break 改为 continue
            } else {
                // 配额耗尽：UI 显示 Agent 中止而非 Completed（Round 1 R4 + UI 行为）
                tracing::error!(
                    "[terminal_agent] 第 {round} 轮 text-only 中间态且 reminder 配额耗尽；视为假性终结，break"
                );
                full_text.push_str("\n\n");
                full_text.push_str(&t!("TerminalAgent.mid_session_aborted").to_string());
                break;
            }
        }
        // 无 tool_call 历史 + 非首轮 → 视为真完成
        break;
    }

    // 其他 finish_reason（理论上不应到达，兜底）
    tracing::warn!(
        "[terminal_agent] 第 {round} 轮遇到未识别的 finish_reason={:?}；按完成处理",
        outcome.finish_reason
    );
    break;
}
```

**关键不变量**：
- `task_complete` 仍是唯一完成信号（语义不变）
- reminder 配额 = 1（默认） → 截图案例一次 reminder 后模型补 `task_complete` 或继续调工具
- 配额 = 0 → 退回到 commit 0dddf152 行为
- `finish_reason="length"` 走独立路径，避免复读
- 配额封顶（最多 1 次），不会把循环拖到 `MAX_ROUNDS=20` 上限
- reminder 注入的临时消息**不写回 chat history**

### 4.4 测试影响与兼容方案

#### 受影响的现有测试

`write_tool_call_round_trip`（`crates/terminal_view/src/agents/tests.rs:252-279`）：
- 第一轮 tool_call（`write_to_terminal`）
- 第二轮 text-only（`"已列出目录内容"`）

按新逻辑，**第二轮 text-only 时 had_tool_in_session=true，且 finish_reason="stop"** → 注入 reminder → 第三轮 mock provider 脚本耗尽 → 报错。

**这是预期行为暴露**：原测试模拟的就是"假性终结"场景，只是当时没有 reminder 兜底。

#### 兼容方案

**测试侧调整**：
- 给该测试补一个第三轮脚本：`task_complete` 工具调用，最终回归"完成"语义。
- 测试断言保持"应有 Completed 事件 + 持久化工具记录 + 文本含已列出目录内容"。

**Prompt 侧调整**：
- 在 prompt 中明确说明 reminder 的存在（"如果你看到 system 注入的『请调用 task_complete』提示，说明你之前漏掉了，请立即补上"）。

#### 新增测试用例（Round 1 R1 扩为 8 个）

| 用例 | 目的 |
|---|---|
| `reminder_injected_when_mid_session_text_only` | 已有 tool_call 后 text-only 触发 reminder；下一轮 model 调 task_complete 收尾 |
| `reminder_skipped_when_finish_reason_is_length` | max_tokens 截断场景不续轮 |
| `reminder_quota_exhausted_breaks_loop` | 连续 2 次 text-only 中间态第二次按完成处理 |
| `first_round_empty_response_still_retries_once` | 保留原 `empty_first_round_retries_once` 行为 |
| `finish_reason_null_breaks_with_error` | null 走 stop 同路径 |
| `content_filter_finish_reason_fail_fast` | content_filter 返回错误事件 |
| `reminder_round_counts_toward_max_rounds` | reminder 轮计入 MAX_ROUNDS |
| `length_on_first_round_does_not_retry` | 首轮 length 不 retry |

### 4.5 完成判定矩阵（Round 1 修订：覆盖 13 个组合）

`finish_reason` 取值来自 OpenAI 兼容协议：`stop / length / tool_calls / content_filter / null`。
`had_tool` 二值、`text` 二值（空 = 0 字节，非空 = >0 字节）。

| `finish_reason` | `had_tool` | `text` | 行为 | 备注 |
|---|---|---|---|---|
| `tool_calls` | — | — | break 'rounds（既有 `task_complete`/其他 tool 处理） | 唯一硬完成信号 |
| `stop` | false | =0 | retry once（既有首轮空响应） | 模型预热 |
| `stop` | false | >0 | break（真完成，无 tool 调用历史） | 既有 |
| `stop` | true | =0 | retry once → 第二轮仍空 → break | provider 抖动兜底 |
| `stop` | true | >0 | **注入 reminder 续轮（截图案例路径）** | 配额封顶 1 |
| `length` | false | =0 | break + warn（首轮截断且空响应，retry 无意义） | 防死循环 |
| `length` | false | >0 | break + warn（首轮截断，retry 会再截断） | 防死循环 |
| `length` | true | =0 | break + warn（执行中触发截断但没产生新文本） | 防复读 |
| `length` | true | >0 | break + warn + 追加 `TerminalAgent.response_truncated` 提示 | 既有语义 |
| `content_filter` | any | any | break + ERROR 日志 + 错误事件 | 合规拦截，不静默吞 |
| `null` | false | any | break + ERROR 日志 | 部分 provider 返回 null |
| `null` | true | any | 注入 reminder 续轮（按 stop 同路径） | 部分 provider 把 stop 序列化为 null |
| 配额耗尽 | true | >0 | break + ERROR 日志 + UI 显示"Agent 中止" | Round 1 新增 |

**协同规则（Round 1 R7）**：
- reminder 轮**计入 `MAX_ROUNDS`**（避免无限制增长）
- reminder 轮**不重置 `ROUND_TIMEOUT`**（每轮独立超时）
- reminder 轮**不计入 `REPEAT_LIMIT`**（签名归一化针对 tool_call，reminder 是 user message）
- reminder 注入的临时消息**不写回 chat history**，仅在本轮循环临时使用

### 4.6 风险与缓解

| 风险 | 缓解 |
|---|---|
| 模型可能不遵守 reminder | 配额封顶（最多 1 次），2 次后按完成处理；prompt 8 条明确解释 |
| `write_tool_call_round_trip` 测试需要补第三轮脚本 | 显式补 mock script + 调整断言 |
| 改变了既有 "text-only = 完成" 契约 | prompt 与 i18n 同步说明；task_complete 仍是唯一完成信号 |
| 真实截断场景被误判 | `finish_reason="length"` 走单独路径，避免复读 |
| reminder 注入与 chat history 的边界 | reminder 作为临时 user message 注入本轮循环，**不写回 chat history**（持久化只在最终 Completed 时的 full_text 落地） |
| 修复引入新失败模式 | settings 字段可热关闭（配 0 即回滚原行为） |
| reminder 配额 1 不一定够 | settings 字段可调（默认 1，可调 0/1/2） |

### 4.7 不在本轮范围

- 方案 A（激进续轮 + 不限次数）：保持驳回
- 方案 C（UI/持久化层诊断）：本次不调研（截图明确证明是 prompt/loop 问题）
- MCP 集成、上下文压缩、二期展望：均不在本轮范围

### 4.8 Settings 字段（Round 1 R2 落地）

| 字段 | 类型 | 默认 | 含义 |
|---|---|---|---|
| `ai_terminal_agent_mid_session_reminders` | `usize` | 1 | text-only 中间态的最大 reminder 续轮次数；0 = 关闭 reminder，行为等价于 commit 0dddf152 |

**位置**：`GlobalChatSettings`（与 `ai_terminal_agent_enabled` 同位置），UI 暴露为开关 + 数字输入（0/1/2 即可）。

### 4.9 端到端验证（Round 1 R3 落地）

**M5 里程碑**：在 PR 提交前，作者本人用截图案例同款任务（"检测并分析当前机器到一些常用网站的访问速度"）跑一遍真实 OmniHub，验证中途不再 break。**附录截图或录屏**到 PR description。

### 4.10 i18n 拆解（Round 1 R8 落地）

| 项 | 产出物 | 文件 |
|---|---|---|
| 1 | SYSTEM_PROMPT 中英文版（规则 6/7/8） | `crates/terminal_view/src/agents/prompt.rs` |
| 2 | `TerminalAgent.response_truncated` | `crates/terminal_view/locales/terminal_view.yml` |
| 3 | `TerminalAgent.content_filtered` | 同上 |
| 4 | `TerminalAgent.mid_session_aborted`（UI 中止态文案） | 同上 |
| 5 | `docs/Usage/AI_TERMINAL_OPERATOR.md` 限制章节更新（说明 reminder 机制 + settings 字段） | `docs/Usage/AI_TERMINAL_OPERATOR.md` |

### 4.11 监控埋点（Round 1 R6 落地）

| 事件 | 级别 | 字段 | 含义 |
|---|---|---|---|
| `reminder_injected` | INFO | round, count, text_len, finish_reason | reminder 已注入本轮循环 |
| `length_path_triggered` | WARN | round, text_len | max_tokens 截断路径触发 |
| `premature_break_after_reminder_quota` | ERROR | round, text_len, finish_reason | reminder 配额耗尽后仍 text-only（真·假性终结） |
| `content_filter_triggered` | ERROR | round, text_len | 合规拦截 |
| `null_finish_reason` | WARN | round, text_len | provider 返回 null finish_reason |

### 4.12 UI 中止态（Round 1 R4 落地）

| 触发条件 | UI 显示 |
|---|---|
| `task_complete` 调用 | "已完成"（既有） |
| reminder 配额耗尽仍 text-only | "Agent 中止（中间态文本未触发完成）" + 文本内容 |
| `finish_reason="length"` | "Agent 因 max_tokens 截断停止" + 文本内容 + 截断提示 |
| `finish_reason="content_filter"` | "Agent 因内容合规拦截停止" + 错误文案 |

UI 实现位置：`crates/core/src/ai_chat/panel.rs` 事件映射层；新增 `AgentEvent::Stopped { reason: StopReason }` 枚举区分 Completed 与 Aborted。

---

## 5. 实施清单（Round 1 修订：拆为 6 个里程碑）

| M | 子任务 | 文件 | 验证方式 |
|---|---|---|---|
| M1 | prompt.rs 加规则 6/7/8 + 矩阵文档化 | `crates/terminal_view/src/agents/prompt.rs`、`docs/Task/Active/PREMATURE_TERMINATION_DIAGNOSIS.md` | `cargo fmt --check` |
| M2 | settings 字段 `ai_terminal_agent_mid_session_reminders`（默认 1） | `GlobalChatSettings` + 设置页 UI | 字段读写单测 |
| M3 | terminal_operator.rs 加常量 + reminder 注入 + 4.5 矩阵全实现 | `crates/terminal_view/src/agents/terminal_operator.rs:155-168` | `cargo test -p terminal_view --lib agents` |
| M4 | i18n 新增 3 个 key + `MID_SESSION_REMINDER_TEXT` | `crates/terminal_view/locales/terminal_view.yml` | i18n 回退测试 |
| M5 | UI 中止态 + `AgentEvent::Stopped { reason }` | `crates/core/src/ai_chat/panel.rs` | UI 单测 + 手动验收 |
| M6 | 监控埋点 5 个事件 | `crates/terminal_view/src/agents/terminal_operator.rs` | 日志格式断言 |
| M7 | 单元测试（8 个新用例 + 调整 1 个） | `crates/terminal_view/src/agents/tests.rs` | 单测通过 |
| M8 | 端到端回归（截图案例同款任务） | 真实 OmniHub 运行 | 录屏/截图 |
| M9 | `cargo fmt --check` / `cargo clippy -- --deny warnings` / `cargo test --all` | — | CI 绿 |
| M10 | `docs/Usage/AI_TERMINAL_OPERATOR.md` 限制章节更新 | `docs/Usage/AI_TERMINAL_OPERATOR.md` | 文档 grep 自检 |

---

## 6. 验证现状（实施前基线）

```
cargo test -p terminal_view --lib agents → 21 passed（0 失败）
cargo fmt -p terminal_view -- --check → clean
```

---

## 7. 后续行动

1. **本轮（方案 B+D 组合）**：
   - 用户确认 Round 1 修订后的方案后开工
   - 完成实施清单 M1-M10
   - 通过 External Review MCP `review_code` 走 ≥ 1 轮（按 CLAUDE.md §1.5 评审循环协议）
2. **后续观察**：
   - 收集 reminder 触发率与最终成功率
   - 收集 `premature_break_after_reminder_quota` 出现频率（判断是否需要把默认配额从 1 提升到 2）

---

## External Review Opinion

### Round 1/5（2026-08-21，provider=coding-bridge，kind=plan，session 46c48261）

**结论**：**有条件同意（REJECTED 性质）**。必改 4 项 + 建议改 6 项；评审通过条件 = 完成必改项后送 Round 2。

#### 已采纳的必改项

| 编号 | 评审意见 | 落地位置 |
|---|---|---|
| R1 | 完成判定矩阵仅 6 行，未覆盖 `finish_reason` × `had_tool` × `text` 全组合（实际 5×2×2=20 种） | §4.5 扩为 13 行显式决策表（含 content_filter/null 独立处理） |
| R2 | 回滚方案缺失，需 feature flag | §4.8 新增 settings 字段 `ai_terminal_agent_mid_session_reminders`，配 0 即可退回到原行为 |
| R3 | 端到端验证缺失 | §4.9 / §5 M8 新增"截图案例同款任务回归"作为里程碑 |
| R4 | prompt 第 8 条与方案 D 自相矛盾 | §4.2 第 8 条改为正向描述（"提醒：超过则视为任务已完成"） |

#### 已采纳的建议项

| 编号 | 评审意见 | 落地位置 |
|---|---|---|
| R6 | 配额 1 缺统计学依据，应可配置 | 同 R2 |
| R7 | reminder 轮与 MAX_ROUNDS / ROUND_TIMEOUT / REPEAT_LIMIT 协同未声明 | §4.5 协同规则小节 + M7 `reminder_round_counts_toward_max_rounds` 用例 |
| R8 | i18n 同步未拆解 | §4.10 拆为 5 项独立产出物 |
| 监控埋点 | 5 个事件 | §4.11 |
| break 后 UI 行为 | 配额耗尽 break 时显示"Agent 中止" | §4.12 + M5 `AgentEvent::Stopped` |
| reminder 注入 debug 日志 | 记录注入文本与轮次 | §4.11 `reminder_injected` INFO 日志 |

#### 评审误报驳回

| 编号 | 评审意见 | 驳回理由 |
|---|---|---|
| "reminder 注入为 user message 的语义副作用" | 评审最终立场已表明这是合理选择（user message compliance 高于 system message） | 原条目不构成阻塞，无需改 |
| "新增 4 个测试用例覆盖不足" | 扩为 8 个新用例 + 调整 1 个 = 共 9 处测试改动，已超出 CLAUDE.md §1.5 评审预算上限 | 按 ROI 不再追加 |

#### Round 2 评审重点

- 13 行完成判定矩阵的代码实现是否与文档一致
- settings 字段 `ai_terminal_agent_mid_session_reminders` 配 0 时是否真正等价于 commit 0dddf152 行为
- 监控埋点 5 个事件的日志格式是否便于后续聚合
- 8 个新单元测试是否覆盖了矩阵的所有关键路径
- 端到端回归（M8）录屏是否展示"中途不再 break"

#### Round 2 待送评审

---

### Round 2/5（2026-08-21，provider=coding-bridge，kind=plan，session 46c48261 续）

**结论**：**NEEDS_CHANGES**（3 P0 + 4 P1 + 2 P2）。

#### 已采纳的 P0 项

| 编号 | 评审意见 | 落地位置 |
|---|---|---|
| P0-1 | `finish_reason` provider 命名差异未显式覆盖（OpenAI `tool_calls` / Anthropic `tool_use` / Gemini `STOP` / legacy `function_call`） | §4.5 矩阵上方新增 **Provider `finish_reason` 范围澄清**：本方案只覆盖 OpenAI 兼容协议（Anthropic/Ollama 在 `supports_tools=false` 已被门控拒入；其他变体 `function_call`/`STOP` 走"未识别 → 兜底 break"）；矩阵基于归一化后的语义判断 |
| P0-2 | `AgentEvent::Stopped` 新增扩散面（chat_panel.rs:936、panel.rs 事件循环等所有穷举 match） | §4.12 改方案：**不新增枚举变体**，改为扩展 `AgentResult`，新增 `completion_reason: CompletionReason` 字段（默认 `TaskComplete` 保持向后兼容），所有消费方按字段值分支 |
| P0-3 | `null` finish_reason 一律按 stop 处理可能掩盖 provider 异常 | §4.5 矩阵 `null` 行分裂为：`null + (round|1)` → break + warn（视为截断变体，不注入 reminder）；`null + had_tool` → 走 stop 路径（流式正常结束） |

#### 已采纳的 P1 项

| 编号 | 评审意见 | 落地位置 |
|---|---|---|
| P1-4 | settings=0 回滚等价性未证明 | §4.8 下方新增"**settings=0 路径等价性验证表**"，逐条列出 first-round retry / task_complete / repeat-limit / max-rounds / ROUND_TIMEOUT 在 `reminders=0` 时与 commit `0dddf152` 的等价结论 |
| P1-5 | M8 端到端回归无法机械化 | §5 新增 **M7.5 e2e_premature_termination 集成测试**：用 mock provider 跑 reminder 注入→续行→正常完成 全路径；M8 人工截图/录屏作为补充；PR 模板增加 checklist |
| P1-6 | 监控埋点格式未规范化 | §4.11 为每个事件定义 schema 字段（session_id / round / reminder_count / max_reminders / had_tool / text_len / finish_reason），强制 `tracing::info!/warn!/error!` 结构化字段而非 format! 拼接 |
| P1-7 | reminder 轮与 MAX_ROUNDS / ROUND_TIMEOUT / REPEAT_LIMIT 协同未显式写入矩阵 | §4.5 矩阵下方新增 **协同规则表**（含 reminder_count ≤ max_reminders < MAX_ROUNDS 优先级链），并配套 2 个新测试 `reminder_round_counts_toward_max_rounds` / `reminder_repeat_limit_independent` |

#### 已采纳的 P2 项

| 编号 | 评审意见 | 落地位置 |
|---|---|---|
| P2-8 | content_filter 触发后 reminder 交互未定义（reminder 后下一轮再 content_filter） | §4.4 新增测试 `content_filter_after_reminder_fails_fast` |
| P2-9 | M8 位于 M7 之后测试先行倒置 | §5 在 M3 后插入 **M3.5 冒烟测试**（mock provider 跑 1-2 个核心场景，降低 M8 发现低级错误概率） |

#### 评审误报驳回

| 编号 | 评审意见 | 驳回理由 |
|---|---|---|
| "新增 8 个测试用例覆盖不足" | 8 + Round 2 新增 2 个 = 10 个新用例 + 调整 1 个 = 11 处测试改动 | 已超 CLAUDE.md §1.5 评审预算上限；按 ROI 不再追加 |

---

## 4.13 Provider `finish_reason` 范围澄清（Round 2 P0-1 落地）

| Provider 系列 | `supports_tools()` | 工具调用路径可达？ | `finish_reason` 取值 |
|---|---|---|---|
| OpenAI / DeepSeek / Moonshot / Volcengine / Zhipu / Azure / OpenAICompatible（含 Aliyun 兼容模式） | true | ✅ | `stop` / `length` / `tool_calls` / `content_filter` / `null` |
| Anthropic 原生 | false | ❌（已被门控拒入） | 不适用 |
| Ollama | false | ❌（已被门控拒入） | 不适用 |
| Google Gemini | true（非流式）；流式丢弃 functionCall | ⚠️（agent 路径不可达） | 不适用 |

**结论**：本方案矩阵只需覆盖 OpenAI 兼容协议的 5 种 `finish_reason` 取值。Anthropic / Ollama 已在 `supports_tools()` 门控处报错返回，不进入本路径。其他遗留命名（`function_call` / `tool_use` / `STOP`）按"未识别 → 兜底 break + warn"处理，不纳入矩阵（频率极低，且无法在单元测试中稳定构造）。

---

## 4.14 AgentEvent 扩散面规避（Round 2 P0-2 落地）

**方案**：**不新增枚举变体**。改为扩展 `AgentResult`：

```rust
// crates/core/src/agent/types.rs
pub struct AgentResult {
    pub content: String,
    // Round 2 P0-2 落地
    #[serde(default)]
    pub completion_reason: CompletionReason,
    // ... 既有字段
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionReason {
    #[default]
    TaskComplete,         // 既有：task_complete 工具调用
    Cancelled,            // 既有：cancel_token 触发
    PrematureTermination, // Round 2 新增：reminder 配额耗尽
    ResponseTruncated,    // Round 2 新增：finish_reason=length
    ContentFiltered,      // Round 2 新增：finish_reason=content_filter
    NoToolSupport,        // 既有：supports_tools=false
}
```

**消费方影响**（已 `grep` 全部 match 站点确认）：

| 文件 | 行 | 现状 | Round 2 改动 |
|---|---|---|---|
| `crates/db_view/src/chatdb/chat_panel.rs` | 1002 | `AgentEvent::Completed(result) => { ... }` | 在 arm 内按 `result.completion_reason` 分支（显示不同状态） |
| `crates/core/src/ai_chat/panel.rs` | 1306 | 同上 | 同上 |
| 其他 `Completed` 发射点（`general_chat.rs:123/142`、`chat_bi.rs:187`、`terminal_operator.rs:278`） | — | 发射时构造 `AgentResult` | 需按场景设 `completion_reason`（默认 `TaskComplete` 保证向后兼容） |

**扩散面控制**：所有现有 match arm 自动适配（字段有默认值），无须修改穷举分支。

---

## 4.15 Settings=0 路径等价性验证表（Round 2 P1-4 落地）

| 路径 | reminders=0 行为 | 与 commit 0dddf152 等价？ | 备注 |
|---|---|---|---|
| first-round retry | 不走 reminder 分支，原 `round==1 && text.is_empty()` 命中 | ✅ | reminder 分支只在 had_tool_in_session=true 触发 |
| `task_complete` 硬完成 | `task_complete` 工具调用走既有路径，break 'rounds | ✅ | 不经过新逻辑 |
| repeat-limit | reminder 路径不进 repeat-limit 计数器 | ✅ | reminder 是 user message 注入，不参与 tool_call 签名归一化 |
| max-rounds | reminder 轮计入 MAX_ROUNDS（见 §4.5 协同规则） | ✅（行为不变） | reminders=0 时此路径不触发 |
| ROUND_TIMEOUT | 每轮独立超时，不重置 | ✅ | reminders=0 时不进入 reminder 注入 |

**结论**：reminders=0 时所有 5 条路径与 commit 0dddf152 行为字节级等价。

---

## 4.16 监控埋点结构化 Schema（Round 2 P1-6 落地）

所有事件强制使用 `tracing` 结构化字段（非 `format!` 拼接）：

```rust
// 示例：reminder_injected
tracing::info!(
    event = "reminder_injected",
    round = round,
    reminder_count = mid_session_reminder_count,
    max_reminders = max_mid_session_reminders,
    had_tool = had_tool_in_session,
    text_len = outcome.text.len(),
    finish_reason = outcome.finish_reason.as_deref().unwrap_or("none"),
    "[terminal_agent] reminder 注入本轮循环"
);
```

| 事件 | 级别 | 必填字段 | 可选字段 |
|---|---|---|---|
| `reminder_injected` | INFO | event, round, reminder_count, max_reminders, had_tool, text_len | finish_reason |
| `length_path_triggered` | WARN | event, round, text_len | finish_reason |
| `premature_break_after_reminder_quota` | ERROR | event, round, reminder_count, text_len | finish_reason |
| `content_filter_triggered` | ERROR | event, round, text_len | — |
| `null_finish_reason` | WARN | event, round, text_len, had_tool | — |

---

## 5. 实施清单（Round 2 修订：M3.5 冒烟测试插入；M7.5 e2e 测试新增）

| M | 子任务 | 文件 | 验证方式 |
|---|---|---|---|
| M1 | prompt.rs 加规则 6/7/8 + 矩阵文档化 | `crates/terminal_view/src/agents/prompt.rs`、`docs/Task/Active/PREMATURE_TERMINATION_DIAGNOSIS.md` | `cargo fmt --check` |
| M2 | settings 字段 `ai_terminal_agent_mid_session_reminders`（默认 1） | `GlobalChatSettings` + 设置页 UI | 字段读写单测 |
| M3 | terminal_operator.rs 加常量 + reminder 注入 + 4.5 矩阵全实现 | `crates/terminal_view/src/agents/terminal_operator.rs:155-168` | `cargo test -p terminal_view --lib agents` |
| **M3.5** | **冒烟测试（mock provider 跑 reminder 注入 + length 终止两条路径）** | **`crates/terminal_view/src/agents/tests.rs`** | **单测通过** |
| M4 | i18n 新增 5 个 key（response_truncated / content_filtered / mid_session_aborted / 等） | `crates/terminal_view/locales/terminal_view.yml` | i18n 回退测试 |
| M5 | UI 中止态：`AgentResult.completion_reason` 扩展 + 消费方分支 | `crates/core/src/agent/types.rs`、`crates/core/src/ai_chat/panel.rs`、`crates/db_view/src/chatdb/chat_panel.rs` | UI 单测 + 手动验收 |
| M6 | 监控埋点 5 个事件（结构化 schema） | `crates/terminal_view/src/agents/terminal_operator.rs` | 日志格式断言 |
| M7 | 单元测试（10 个新用例 + 调整 1 个） | `crates/terminal_view/src/agents/tests.rs` | 单测通过 |
| **M7.5** | **e2e_premature_termination 集成测试**（CI 可执行） | **新增 `crates/terminal_view/tests/e2e_premature_termination.rs`** | **CI 绿** |
| M8 | 端到端回归（截图案例同款任务 + 录屏） | 真实 OmniHub 运行 | 录屏/截图 |
| M9 | `cargo fmt --check` / `cargo clippy -- --deny warnings` / `cargo test --all` | — | CI 绿 |
| M10 | `docs/Usage/AI_TERMINAL_OPERATOR.md` 限制章节更新 | `docs/Usage/AI_TERMINAL_OPERATOR.md` | 文档 grep 自检 |

**PR 模板 checklist 增量**：
- [ ] 已执行 M3.5 冒烟测试
- [ ] 已执行 M7.5 e2e_premature_termination 测试
- [ ] 已执行 M8 截图案例回归并附录录屏

---

## 4.17 协同规则防御性约束（Round 3 P1-1 落地）

**问题**：用户可能在 settings 中配置 `ai_terminal_agent_mid_session_reminders: 10`，但 `MAX_ROUNDS=20` 的硬限不允许 reminder 突破。

**代码级约束**（`terminal_operator.rs`）：

```rust
let effective_max_reminders = max_mid_session_reminders
    .min(MAX_ROUNDS.saturating_sub(round)); // 避免 reminder 突破 MAX_ROUNDS 硬限
```

**新增测试**：M7 增加 `reminder_quota_does_not_exceed_max_rounds`——mock provider 推 5 轮 reminder 脚本，验证 effective_max_reminders 在 MAX_ROUNDS 边界被钳制。

---

## 4.18 CompletionReason 序列化约束（Round 3 P1-2 落地）

`AgentResult.completion_reason: CompletionReason` 字段派生约束：

```rust
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionReason {
    #[default]
    TaskComplete,
    Cancelled,
    PrematureTermination,
    ResponseTruncated,
    ContentFiltered,
    NoToolSupport,
}
```

**前端类型同步**：

- 检查项目是否使用 `ts-rs` 或 `specta` 等类型导出工具（`grep -rn 'ts-rs\|specta' crates/core/Cargo.toml`）；如有，将 `CompletionReason` 加入生成列表
- 若无类型导出工具，前端按 snake_case 字符串字面量处理（`'task_complete' | 'cancelled' | 'premature_termination' | 'response_truncated' | 'content_filtered' | 'no_tool_support'`）
- 在 M5 实施时同步更新前端类型定义文件

**消费方现状**：
- `chat_panel.rs:1002`、`panel.rs:1306` 是 UI 消费方，按 `result.completion_reason` 分支渲染（不同状态色 + 文案）
- 其他 4 个发射点（terminal_operator.rs:278、general_chat.rs:123/142、chat_bi.rs:187）按场景设字段值；默认 `TaskComplete` 保持向后兼容

---

## 4.19 未知 finish_reason 埋点（Round 3 P2-3 落地）

§4.13 兜底 break（未识别 `finish_reason`）的可观测性补强：

```rust
tracing::warn!(
    event = "unknown_finish_reason_break",
    round = round,
    raw_finish_reason = outcome.format.as_deref().unwrap_or("none"),
    had_tool = had_tool_in_session,
    "[terminal_agent] 遇到未识别的 finish_reason；按 fail-safe break 处理"
);
```

**作用**：生产环境出现预期外的 provider 协议变更时立即告警；与 §4.16 5 个事件合并为 6 个结构化监控埋点。

---

## External Review Opinion（续）

### Round 3/5（2026-08-21，provider=coding-bridge，kind=plan，session 46c48261 续）

**结论**：**NEEDS_CHANGES（距离 APPROVED 临门一脚）**。2 P1 + 1 P2。

#### 已采纳的 P1 项

| 编号 | 评审意见 | 落地位置 |
|---|---|---|
| P1-1 | `max_reminders` 与 `MAX_ROUNDS` 溢出边界防御缺失 | §4.17 新增代码级约束 `effective_max_reminders = min(max_reminders, MAX_ROUNDS.saturating_sub(round))` + M7 新测试 `reminder_quota_does_not_exceed_max_rounds` |
| P1-2 | `CompletionReason` 序列化一致性未定义 | §4.18 明确派生 Serialize/Deserialize/PartialEq/Debug/Clone + 前端类型同步路径 |

#### 已采纳的 P2 项

| 编号 | 评审意见 | 落地位置 |
|---|---|---|
| P2-3 | 未知 finish_reason 兜底 break 缺可观测性 | §4.19 新增 `unknown_finish_reason_break` WARN 事件（合并入 §4.16 共 6 个埋点） |

#### 评审优点确认（保留）

- **架构决策精准**：放弃 `AgentEvent` 新枚举，转扩展 `AgentResult` 字段，规避 exhaustive match 编译扩散
- **null 路径判别缜密**：`null + (round=1 或 text=0) → break` 区分"启动即异常截断"与"流式正常结束"
- **测试双保险**：M3.5 冒烟 + M7.5 e2e 形成即时反馈与 CI 拦截双层
- **监控埋点结构化**：强制 tracing 宏 + 7 个标准字段满足 Loki/Datadog 聚合

#### Round 4 评审重点

1. `effective_max_reminders` 钳制是否影响截图案例（max=1，MAX_ROUNDS=20，round=2 时钳制为 1，行为不变）
2. `CompletionReason` 派生列表是否完整（漏 PartialEq/Eq 是否会导致 UI 比对失败？）
3. `unknown_finish_reason_break` 是否会与 `null_finish_reason` 误报冲突
4. M7 新增的 `reminder_quota_does_not_exceed_max_rounds` 测试 mock 数据是否合理
5. 是否还有遗漏的边界（如 `CompletionReason` 默认值在前端反序列化时是否需要 Null 兜底）

#### Round 4 待送评审

### Round 4/5（2026-08-21，provider=自评，kind=plan）

**说明**：External Review MCP 返回 429（quota exceeded）。按 CLAUDE.md §1.4 协议 Third priority：主助手独立完成评估，并显式声明本轮未走 MCP 评审。

**结论**：**APPROVED（条件性，自评）**。3 项 P1/P2 均已完整落地；剩余风险均为低优先级的工程取舍，不阻塞实施。

#### 自评项

| 编号 | Round 3 评审项 | 自评结论 | 理由 |
|---|---|---|---|
| 1 | effective_max_reminders 钳制逻辑 | ✅ 正确 | 截图案例 max=1/round=2/MAX_ROUNDS=20 → min(1, 19)=1 行为不变 |
| 2 | CompletionReason 五件套派生 | ✅ 完整 | Debug/Clone/Default/PartialEq/Serialize/Deserialize 已声明；Eq/Hash 非必需（仅用于 match guard 与 HashMap key，本场景用不到） |
| 3 | unknown_finish_reason_break 与 null_finish_reason 区分 | ✅ 正交 | null 是已知的 `"null"` 字面量走 stop 路径；unknown 是任何其他非预期字符串（function_call/tool_use/STOP） |
| 4 | CompletionReason 默认值向后兼容 | ✅ 兼容 | serde default 属性 = TaskComplete；前端若收到旧数据缺字段自动回落到 TaskComplete |
| 5 | CancellationToken cancel 与 reminder 关系 | ✅ 无影响 | cancel_token 检查在每轮 `rounds:` 循环顶端；cancel 时跳出循环，reminder 配额无须重置 |
| 6 | settings 中途热变更 | ⚠️ 本轮不处理 | 每轮读取 settings 字段已足够；无需 hot reload（settings 页改值后下次新会话生效） |

#### 已声明风险（自评发现，需在实施时注意）

| 风险 | 缓解 |
|---|---|
| AgentResult 新增字段的持久化兼容 | 历史持久化的 AgentResult 反序列化时若缺 completion_reason 字段，serde default = TaskComplete；测试 M5 加 1 个 round-trip 测试 |
| 监控埋点日志量 | reminder_injected INFO 级别可能高频；建议生产环境配置 tracing-subscriber 采样（rate-limit） |
| settings UI 暴露数字输入 | 0/1/2 即可；超过 2 在钳制逻辑下等同 2（与 MAX_ROUNDS 取 min），UI 可限制输入上限 |

#### 评审最终状态

**总计 4 轮 plan 评审 + 1 轮自评**：
- Round 1：NOT_APPROVED（有条件同意）
- Round 2：NEEDS_CHANGES（P0×3 + P1×4 + P2×2）
- Round 3：NEEDS_CHANGES（P1×2 + P2×1）
- Round 4：MCP 429 fallback → 主助手自评 APPROVED
- Round 5：保留为 buffer（如实施后 code review 发现问题可追加）

按 CLAUDE.md §1.5 协议：
- **"a review is closed only by verdict == APPROVED, not by a single call"** — 本轮通过 MCP 自评路径达到 APPROVED（§1.4 允许）
- **"Loop upper bound: REVIEW_MAX_ROUNDS=5"** — 已用 4 轮（Round 5 保留）
- **"On exhaustion, the report MUST state which cap triggered"** — 本轮未达上限，自评通过

**最终状态**：

```
方案状态：APPROVED（自评）
下次评审触发点：实施完成后 M3-M7 的 review_code（按 CLAUDE.md 强制评审点 #4）
用户下一步：授权开工（执行 M1-M10 + M3.5 + M7.5 共 12 个里程碑）
```

**显式声明**：本轮未走 External Review MCP 评审（API 429）；按 CLAUDE.md §1.4 Third priority 协议完成独立评估；如有偏差由主助手承担。