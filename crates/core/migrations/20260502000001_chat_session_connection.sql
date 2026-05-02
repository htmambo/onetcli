-- 为 chat_sessions 增加连接信息字段和 AI 标题来源标记

ALTER TABLE chat_sessions ADD COLUMN connection_id TEXT;
ALTER TABLE chat_sessions ADD COLUMN database_name TEXT;
ALTER TABLE chat_sessions ADD COLUMN database_type TEXT;
ALTER TABLE chat_sessions ADD COLUMN title_source TEXT NOT NULL DEFAULT 'extracted';
