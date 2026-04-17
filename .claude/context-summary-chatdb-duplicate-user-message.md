## 项目上下文摘要（chatdb-duplicate-user-message）
生成时间：2026-03-26 21:59:33 +0800

### 1. 相似实现分析
- **实现1**: `crates/db_view/src/chatdb/chat_panel.rs`
  - 模式：`send_message()` 先更新 UI 和内存 `chat_history`，再调用 `send_to_ai()`。
  - 可复用：ChatDB 的历史裁剪逻辑、`AgentContext::new(...)` 入口。
  - 需注意：当前实现会在发送前把当前用户输入先压入 `chat_history`。

- **实现2**: `crates/core/src/agent/builtin/general_chat.rs`
  - 模式：Agent 统一按“`ctx.chat_history + ctx.user_input`”构造真正发给模型的 messages。
  - 需注意：如果 `ctx.chat_history` 已经包含当前用户输入，就会重复。

- **实现3**: `crates/db_view/src/chatdb/agents/sql_workflow.rs`
  - 模式：SQL Agent 在生成 SQL 时会把 `ctx.chat_history` 全部拼入，再单独追加 `context.user_question`。
  - 需注意：和 GeneralChatAgent 一样，依赖 `AgentContext` 的历史不包含本轮输入。

- **实现4**: `crates/db_view/src/chatdb/agents/query_workflow.rs`
  - 模式：AI 选表 prompt 先附加对话历史，再附加当前 `user_question`。
  - 需注意：如果历史尾部已经含有本轮用户问题，prompt 里也会重复。

- **实现5**: `crates/core/src/ai_chat/panel.rs`
  - 模式：普通 AI 聊天面板从持久化消息构造历史，不在发送前往内存历史里多塞一份“当前输入”供 Agent 使用。
  - 结论：普通 AI 面板没有同类重复拼装问题。

### 2. 根因结论
- ChatDB 的 `send_message()` 会先执行：
  - `self.chat_history.push(Message::text(Role::User, content.clone()))`
- 随后 `send_to_ai()` 又把 `self.chat_history` 整体传给 `AgentContext`。
- 而多个 Agent 和路由层都默认会再把 `ctx.user_input` 作为当前用户消息追加一次。
- 因此同一条输入在 ChatDB 中会以“两份用户消息”的形式进入模型上下文。

### 3. 修复策略
- 不改 Agent 接口，不改路由器，不改 SQL/BI/GeneralChat 各自实现。
- 只在 ChatDB 入口增加一个小函数：
  - 从 `chat_history` 截取最近 `history_count` 条
  - 如果尾部正好是本轮 `current_input` 对应的用户消息，则剔除
- 这样可以一次性修复：
  - GeneralChatAgent
  - SqlWorkflowAgent
  - ChatBiAgent 的 AI 选表 prompt
  - IntentRouter 的上下文历史

### 4. 测试策略
- 新增纯单元测试，验证：
  - 尾部是当前用户消息时会被剔除
  - 较早位置的同文案历史消息会被保留，不会误删
- 编译验证范围：
  - `db_view`
  - `main`

### 5. 风险点
- 只剔除“历史尾部且角色为 User 且文本与当前输入完全相等”的一条消息，避免误删较早历史。
- `retry_last_operation()` 仍可复用同一逻辑，因为它重新发送时历史尾部本来就是上次失败那条用户消息。
