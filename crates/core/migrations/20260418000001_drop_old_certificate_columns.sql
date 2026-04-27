-- Certificate params 迁移完成后，删除冗余的旧列
-- 这些字段已迁移至 params JSON 中
-- 迁移运行器会自动跳过"no such column"错误（幂等）

ALTER TABLE certificates DROP COLUMN username;
ALTER TABLE certificates DROP COLUMN password;
ALTER TABLE certificates DROP COLUMN key_path;
ALTER TABLE certificates DROP COLUMN passphrase;
