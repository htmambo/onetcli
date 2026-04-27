-- 清理云端数据库中的团队相关数据
-- 团队功能已从客户端移除，清理残留的团队同步数据

-- 删除所有团队类型的同步数据（软删除 + 硬删除）
DELETE FROM sync_data WHERE data_type = 'team';

-- 清理关联的 auth_sessions（如有团队会话表）
-- 注意：如有 team_sessions 表，在此添加清理语句
