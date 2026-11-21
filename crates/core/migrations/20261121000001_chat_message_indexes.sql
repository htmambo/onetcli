-- chat_messages 复合索引
--
-- 1) 按会话读取并按时间排序：会话历史加载（list_by_session / list_by_session_limit）
-- 2) 按角色过滤并按时间倒序取最近 N 条：AI 输入框历史（list_recent_user）

CREATE INDEX IF NOT EXISTS idx_chat_messages_session_created
    ON chat_messages (session_id, created_at, id);

CREATE INDEX IF NOT EXISTS idx_chat_messages_role_created
    ON chat_messages (role, created_at DESC, id DESC);
