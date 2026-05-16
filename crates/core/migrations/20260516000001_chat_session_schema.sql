-- 为 chat_sessions 增加 schema_name 字段，用于完整恢复数据库连接上下文

ALTER TABLE chat_sessions ADD COLUMN schema_name TEXT;
