-- 为 chat_messages 增加工具调用结构化字段，支持 agent 多轮工具调用上下文持久化。
--
-- role='assistant' 且携带工具调用时：tool_calls_json 存储 Vec<ToolCall> 的 JSON 序列化。
-- role='tool' 时：tool_call_id 存储对应的 tool call id，content 存储工具结果文本。
-- 既有行的新列默认 NULL，向后兼容（普通 user/assistant 文本消息不受影响）。

ALTER TABLE chat_messages ADD COLUMN tool_call_id TEXT;
ALTER TABLE chat_messages ADD COLUMN tool_calls_json TEXT;
