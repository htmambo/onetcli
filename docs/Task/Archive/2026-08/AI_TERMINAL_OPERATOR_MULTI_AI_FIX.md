# AI 终端操作员：多 AI 并发隔离、工具上下文持久化与当前终端语义修复

**Status**: ✅ 已完成并归档
**完成时间**: 2026-08-25
**分支**: `feat/ai-terminal-operator`
**关联**: [`PREMATURE_TERMINATION_DIAGNOSIS.md`](PREMATURE_TERMINATION_DIAGNOSIS.md)（前置：假性终结修复与 `since_last_write`）

---

## 1. 背景

在 `PREMATURE_TERMINATION_DIAGNOSIS` 落地假性终结修复后，用户在多终端 + 多 AI 助手并发场景下连续报告三个问题：

1. **运行时崩溃**：`there is no reactor running on thread` panic，AI 助手一发起对话就崩。
2. **多轮工具调用/回复混乱**：同一会话里多轮 `write_to_terminal` / `read_terminal_output` / `task_complete` 的上下文串不住，模型在后续轮次丢失前序工具结果，出现重复调用或答非所问。用户提出"会话可能需要独立标识，比如 session_id?"。
3. **切终端后 AI 仍操作旧终端**：用户观察 AI 助手输出 `focused_id: 2, host_terminal_id: 1`，指出 `focused_id` 在多终端下并不等于"当前终端"，`host_terminal_id` 才符合定义。

三个问题相互独立，根因分别在运行时调度层、持久化层、描述层。

---

## 2. 修复一：tokio reactor panic（commit `dd6d0ea7`）

### 2.1 根因

前一轮为消除 `tokio_handle.enter()` 的 `EnterGuard` "dropped out of order" panic 而移除了 `enter()` 调用，结果 `AgentDispatcher::dispatch` 在主线程（GPUI 前台执行器）上同步运行 `provider.chat()`，reqwest 的 `Client::execute_request` 内部调用 `tokio::time::sleep` 需要当前线程存在 tokio reactor 上下文——主线程没有 reactor，于是 panic。

两个 panic 互为矛盾：
- `enter()` → 提供 reactor 上下文，但 `EnterGuard` 与主线程上嵌套的 `cx.spawn` 子任务（保存 assistant 消息）LIFO 丢弃顺序冲突 → "dropped out of order"。
- 不 `enter()` → 无丢弃顺序问题，但 reqwest 在主线程找不到 reactor → "no reactor running"。

### 2.2 修复

把 `AgentDispatcher::dispatch` 从主线程同步调用改为通过 `Tokio::spawn` 派到 tokio worker 线程运行：worker 线程自带 reactor 上下文，既满足 reqwest 的 reactor 需求，又不需要在主线程 `enter()`，两个 panic 同时消失。

`crates/core/src/ai_chat/panel.rs` `send_via_agent`：

```rust
let dispatch_task = Tokio::spawn(cx, async move {
    let mut rx = AgentDispatcher::dispatch(ctx_agent, &registry, &mut affinity).await;
    (rx, affinity)
});
let (mut rx, affinity) = match dispatch_task.await {
    Ok((rx, affinity)) => (rx, affinity),
    Err(join_err) => { /* error UI update + return */ }
};
```

`Tokio::spawn` 使用全局 `Handle`（不依赖当前线程上下文），是这套修复的关键。

---

## 3. 修复二：工具调用中间态结构化持久化（commit `dd6d0ea7`）

### 3.1 根因

多轮混乱的根因是工具调用中间态从未结构化持久化。run_loop 每轮产生的 `assistant(tool_calls)` + `tool(result)` 消息对只在内存里流转，落库时只存了最终文本摘要。下一轮（或会话恢复后）`build_agent_history_messages` 重建上下文时，LLM 看不到结构化的工具调用/结果，只能看到一段无结构的文字，于是丢失前序工具上下文 → 重复调用、答非所问。

用户"session_id?"的直觉方向正确：需要一个稳定的会话标识把中间态挂上去。项目本就有 `ChatSession.id`，缺的是把工具消息对持久化到该会话的管道。

### 3.2 全链路落地

#### 3.2.1 DB schema 扩展

`crates/core/src/llm/chat_history.rs` `ChatMessage` 增加两列：

```rust
/// role='tool' 时对应的 tool call id；普通消息为 None。
#[serde(skip_serializing_if = "Option::is_none")]
pub tool_call_id: Option<String>,
/// role='assistant' 且携带工具调用时，存 Vec<ToolCall> 的 JSON；普通消息为 None。
#[serde(skip_serializing_if = "Option::is_none")]
pub tool_calls_json: Option<String>,
```

`MessageRepository` 的 `insert` / `update` / 所有 `SELECT`（`get` / `list` / `list_by_session` / `list_recent`）同步带上新列。`FromSqliteRow::from_row` 用 `row.get(...)?` 直接读 `Option`，迁移前的旧行 NULL 安全回落。

新增构造函数：
- `ChatMessage::assistant_tool_calls(session_id, content, tool_calls_json)`
- `ChatMessage::tool_result(session_id, tool_call_id, content)`

迁移文件 `crates/core/migrations/20260624000001_chat_message_tool_calls.sql`：

```sql
ALTER TABLE chat_messages ADD COLUMN tool_call_id TEXT;
ALTER TABLE chat_messages ADD COLUMN tool_calls_json TEXT;
```

在 `crates/core/src/storage/migration.rs` 的 `MIGRATIONS` 数组注册。

#### 3.2.2 AgentContext 携带 session_id

`crates/core/src/agent/types.rs` `AgentContext` 增加 `pub session_id: Option<i64>` 字段及 `with_session_id(Option<i64>)` 链式方法。run_loop 只有知道 session_id 才能把中间态写到正确的会话。

调用点：
- `crates/core/src/ai_chat/panel.rs` `send_via_agent` 构造 AgentContext 时 `.with_session_id(session_id)`
- `crates/db_view/src/chatdb/chat_panel.rs` 构造 AgentContext 时 `.with_session_id(Some(session_db_id))`

#### 3.2.3 run_loop 持久化中间态

`crates/terminal_view/src/agents/terminal_operator.rs` `run_loop` 开头解析 session_id 并取 `MessageRepository`：

```rust
let session_id = ctx.session_id.filter(|&id| id > 0);
let message_repo = session_id.and_then(|_| {
    ctx.storage_manager.get::<one_core::llm::chat_history::MessageRepository>()
});
```

两个闭包 `persist_assistant_tool_calls` / `persist_tool_result` 分别在 `assistant_tool_calls_message` 构造后、`tool_message` 构造前调用，把消息对结构化写入 DB。

**关键编译修复**：`Repository` trait（提供 `insert`）必须在作用域内才能对 `&MessageRepository` 调 `insert`，补 `use one_core::storage::traits::Repository;`。

#### 3.2.4 历史重建

`crates/core/src/ai_chat/panel.rs` 新增自由函数 `chat_message_to_llm_message`，把 `ChatMessage` 转回 LLM `Message`：
- `tool_calls_json` 反序列化重建 `tool_calls`
- `tool_call_id` 重建 `tool_call_id`
- `role='tool'` 映射到 `Role::Tool`

`build_agent_history_messages` 重写为基于该转换函数，保证下一轮/会话恢复时 LLM 看到的是与首次运行时一致的结构化工具上下文。

### 3.3 验证

用户实测确认多轮工具调用上下文不再混乱。

---

## 4. 修复三：当前终端语义更正（commit `a908ca6d`）

### 4.1 根因

用户观察 AI 助手输出 `focused_id: 2, host_terminal_id: 1`，且 `focused_id ≠ 当前终端`。运行时 `resolve_terminal_id_with_host` 本就是 host 优先、focused 兜底——运行时正确。但 prompt 和 schema 的描述层把 `focused_id` 描述成"默认操作终端"，误导 LLM 显式传 `focused_id`，结果在多终端下命中错误的终端。

`focused_id` 的语义是"GPUI 全局最近交互的终端"，用户在 AI 侧栏打字时 focus 跑到侧栏 input，`focused_id` 可能指向别的终端，不适合作为"当前终端"的缺省。

### 4.2 修复

描述层统一改为以 `host_terminal_id` 为"当前终端"缺省，明确警告不要把 `focused_id` 当默认值：

- `crates/terminal_view/src/agents/prompt.rs`：系统提示词说明"你的'当前终端'就是输出里的 `host_terminal_id`"，仅在 `host_terminal_id` 缺失时才回退 `focused_id` 兜底；补充多终端场景说明。
- `crates/terminal_view/src/agents/tools.rs`：工具 schema 的 `terminal_id` 参数描述改为"缺省时自动采用本 AI 助手当前挂载的终端（即输出里的 `host_terminal_id`）"；`get_terminal_list` 输出 JSON 把 `host_terminal_id` 排在 `focused_id` 之前，并在注释里说明语义。

**编译修复**：schema 描述字符串里嵌套了未转义的 `"当前终端"` 导致字面量语法错误，改用中文引号 `「当前终端」`。

### 4.3 验证

用户实测："测试了同时开启三个终端并且在各自的侧边栏中启动 AI 助手，回复/运行看起来正常了。"

---

## 5. 提交记录

| commit | 说明 |
|---|---|
| `dd6d0ea7` | fix(terminal_view,one-core): 修 tokio reactor panic 并持久化工具调用中间态 |
| `f91afb66` | chore(db_view): cargo fmt 归整 db_view crate 格式 |
| `a908ca6d` | fix(terminal_view): prompt/schema 改以 host_terminal_id 为当前终端缺省 |

三个提交均已推送至 `origin/feat/ai-terminal-operator`。

---

## 6. 涉及文件

| 文件 | 改动 |
|---|---|
| `crates/core/src/ai_chat/panel.rs` | `Tokio::spawn` 包装 dispatch；AgentContext 携带 session_id；`build_agent_history_messages` 重写；新增 `chat_message_to_llm_message` |
| `crates/core/src/llm/chat_history.rs` | `ChatMessage` 增 `tool_call_id` / `tool_calls_json`；构造函数；Repository SQL 全量同步 |
| `crates/core/src/agent/types.rs` | `AgentContext` 增 `session_id` 字段与 `with_session_id` |
| `crates/core/src/storage/migration.rs` | 注册 `20260624000001` 迁移 |
| `crates/core/migrations/20260624000001_chat_message_tool_calls.sql` | 新增两列 |
| `crates/terminal_view/src/agents/terminal_operator.rs` | run_loop 持久化中间态；补 `Repository` 导入 |
| `crates/terminal_view/src/agents/prompt.rs` | 当前终端语义更正 |
| `crates/terminal_view/src/agents/tools.rs` | schema 描述 + 输出顺序更正 |
| `crates/db_view/src/chatdb/chat_panel.rs` | AgentContext 调用点补 `with_session_id` |

---

## 7. 备注

- 本轮未走 External Review MCP 评审（三处修复均由用户实测直接验证回归通过）。
- `focused_id` 字段保留，语义不变（全局最近交互终端）；仅描述层不再把它当作"当前终端"缺省。后续可考虑把 `focused_id` 的运行时语义也改为"最近一次主动激活"（见 plan `reactive-painting-grove.md`），不在本次范围。
