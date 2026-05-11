-- 将 SSH 私钥从 key_path (文件路径) 迁移到 ssh_private_key (文件内容)
-- 这是为了支持跨平台同步，因为不同操作系统的文件路径不同

-- 1. 更新 certificates 表中的 SSH 私钥凭证
UPDATE certificates
SET params = json_object(
    'username', json_extract(params, '$.username'),
    'password', json_extract(params, '$.password'),
    'ssh_private_key', json_extract(params, '$.key_path'),
    'passphrase', json_extract(params, '$.passphrase')
)
WHERE kind = 'SshPrivateKey'
  AND params IS NOT NULL
  AND json_extract(params, '$.key_path') IS NOT NULL;

-- 2. 注意：实际的文件内容读取和转换需要在应用层处理
-- 因为 key_path 中存储的是文件路径，需要读取文件内容后更新为 ssh_private_key
-- 应用层会在读取凭证时自动处理这个转换（读取文件内容 → 存储内容）
