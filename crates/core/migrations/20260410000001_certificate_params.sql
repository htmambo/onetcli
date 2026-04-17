-- Certificate params 字段迁移：将敏感字段统一为 params JSON

-- 新增 params 列
ALTER TABLE certificates ADD COLUMN params TEXT;

-- 迁移现有数据：从旧列构建 params JSON
UPDATE certificates SET params = json_object(
    'username', COALESCE(username, ''),
    'password', password,
    'key_path', key_path,
    'passphrase', passphrase
) WHERE params IS NULL;
