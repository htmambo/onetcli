-- connections 表新增创建者字段
ALTER TABLE connections ADD COLUMN owner_id TEXT;
