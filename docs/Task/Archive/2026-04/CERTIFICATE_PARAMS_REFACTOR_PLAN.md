# Certificate 结构重构：敏感字段统一为 params

**状态**: ✅ 已完成 (完成时间: 2026-04-10)

## 目标

将 `Certificate` 结构中的 `username`、`password`、`key_path`、`passphrase` 独立字段统一放入 `params: serde_json::Value`，为后续 params 整体加密做准备。

## 改动总结

### 1. `crates/core/src/storage/models.rs`
- `Certificate` 结构删除 `username`、`password`、`key_path`、`passphrase` 字段
- 新增 `params: serde_json::Value` 字段
- 新增访问方法：`username()`、`password()`、`key_path()`、`passphrase()`、`ssh_private_key()`
- 新增设置方法：`set_username()`、`set_password()`、`set_key_path()`、`set_passphrase()`
- 更新 `display_subtitle()` 使用 `username()` / `key_path()` 方法
- 更新 `apply_certificate_to_db_config()` 使用 `certificate.username()` / `certificate.password()`
- 更新 `apply_certificate_to_ssh_params()` 使用 `certificate.username()` / `certificate.password()` / `certificate.key_path()` / `certificate.passphrase()`
- 更新 `apply_certificate_to_redis_params()` 使用 `certificate.username()` / `certificate.password()`
- 更新 `apply_certificate_to_mongodb_params()` 使用 `certificate.username()` / `certificate.password()`

### 2. `crates/core/src/cloud_sync/models.rs`
- `CertificatePlainData` 结构删除 `username`、`password`、`key_path`、`passphrase` 字段
- 新增 `params: serde_json::Value` 字段

### 3. `crates/core/src/cloud_sync/service.rs`
- `prepare_certificate_sync_data_upload()` 使用 `certificate.params.clone()`
- `decrypt_sync_data_certificate()` 构建 Certificate 时使用 `params: plain_data.params`

### 4. `crates/core/src/storage/repository.rs`
- `CertificateRow` 结构改为读取 `params` 列
- `From<CertificateRow> for Certificate` 实现中从 params 字符串反序列化，并解密 password/passphrase
- 新增 `encrypt_certificate_params()` 函数，对 params 中的敏感字段加密后序列化
- 所有 SQL SELECT 查询改为 `params` 列
- INSERT/UPDATE SQL 语句改为 `params` 列
- `update_from_cloud()` 使用 `encrypt_certificate_params()`

### 5. `crates/core/src/storage/migration.rs`
- 新增迁移 `20260410000001_certificate_params.sql`

### 6. `crates/core/migrations/20260410000001_certificate_params.sql`
- 新增 `params` 列
- 从旧列迁移数据构建 params JSON

### 7. `crates/core/src/certificate_manager.rs`
- 表单初始化使用 `certificate.username()` 等方法
- `build_certificate()` 构建 params JSON 替代独立字段

### 8. `crates/core/src/cloud_sync/blob_vault_driver.rs`
- `apply_bundle()` 中证书恢复改为 `updated_cert.params = cloud_cert.params.clone()`

### 9. `crates/db_view/src/common/db_connection_form.rs`
- `sync_selected_certificate_fields()` 使用 `certificate.username()` 等方法
- `build_connection()` 使用 `certificate.username()` / `certificate.password()` / `certificate.key_path()` / `certificate.passphrase()`

### 10. `crates/terminal_view/src/ssh_form_window.rs`
- `sync_selected_certificate_inputs()` 使用 `certificate.username()` 等方法
- `build_ssh_params()` 使用 `certificate.username()` / `certificate.password()` / `certificate.key_path()` / `certificate.passphrase()`

### 11. `crates/redis_view/src/redis_form_window.rs`
- `sync_selected_certificate_inputs()` 使用 `certificate.username()` / `certificate.password()`
- `build_redis_params()` 使用 `certificate.username()` / `certificate.password()`

### 12. `crates/mongodb_view/src/mongo_form_window.rs`
- `sync_selected_certificate_inputs()` 使用 `certificate.username()` / `certificate.password()`
- `build_parameters()` 使用 `certificate.username()` / `certificate.password()`

## 验证

- ✅ `cargo check -p one-core` 编译通过
- ✅ `cargo check -p main` 编译通过
- ✅ `cargo clippy` 无新增错误
- ⚠️ 测试失败均为预存的 SshParams 字段缺失问题，与本次重构无关

## 备注

不同 `CertificateKind` 的 params 结构：
- UsernamePassword: `{"username": "...", "password": "..."}`
- SshPrivateKey: `{"username": "...", "key_path": "...", "passphrase": "..."}`
