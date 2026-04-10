# 同步系统修复

**状态**: ✅ 已完成 (完成时间: 2026-04-10)
**创建时间**: 2026-04-10

## 问题分析

### 问题 1: 使用 sync_server 登录后同步按钮依然不可用

**根因**: `home_tab.rs` 的 `render_sidebar` 方法中（约 L2999-3004），只处理了登出场景（全局用户为 None 时清空 `self.current_user`），但**没有处理登录场景**。当用户从设置页面登录后，`GlobalCurrentUser::set_user` 已更新，但 `HomePage.current_user` 仍然是 `None`，导致 `can_sync = false`，同步按钮保持禁用。

**相关代码**:
- `main/src/home_tab.rs:2497` — `let can_sync = is_logged_in || uses_github_gist;`
- `main/src/home_tab.rs:2999-3004` — `render_sidebar` 中只同步登出状态
- `main/src/setting_tab.rs:2401` — 登录后调用 `GlobalCurrentUser::set_user(Some(user), cx)`

**修复方案**: 在 `render_sidebar` 中增加正向同步：当 `global_user.is_some() && self.current_user.is_none()` 时，将 `self.current_user` 设为全局用户的值。

### 问题 2: GitHub Gist 同步实际上不更新数据

**根因**: `crates/core/src/cloud_sync/blob_vault_driver.rs` 中的 `sync_handler_via_blob()` 函数（L67-71）是**空实现**，注释明确写了"跳过 handlers，待实现 blob 同步逻辑"。实际 sync 操作只是验证了 vault 连接（调用 `vault.list(None)`），但没有真正上传/下载任何数据。

**相关代码**:
- `crates/core/src/cloud_sync/blob_vault_driver.rs:67-71` — 跳过所有 handler
- `crates/core/src/cloud_sync/blob_vault.rs` — BlobVault trait 定义（Gist 已实现）
- `crates/core/src/cloud_sync/sync_backend.rs:89-109` — BlobVaultBackend 调用 `blob_sync()`
- `crates/core/src/cloud_sync/oauth/github_gist.rs` — Gist vault 实现（上传/下载已实现）

**修复方案**: 在 `blob_vault_driver.rs` 中实现真正的 blob 同步逻辑：
1. 利用已有的 `generic_sync` 框架，但改用 BlobVault 作为数据传输层
2. 对于 Connection 同步：从本地读取连接数据 → 序列化为加密 blob → 上传到 Gist
3. 下载时：从 Gist 下载 blob → 解密 → 反序列化 → 合并到本地
4. 利用 BlobVault trait 已有的 `upload/download/list/exists/delete` 方法

**架构思路**:
- BlobVault 的 key 命名: `connections`, `certificates`, `workspaces`
- 上传：本地数据序列化 → 加密 → `vault.upload(key, encrypted_data)`
- 下载：`vault.download(key)` → 解密 → 反序列化 → 应用到本地
- 冲突检测：基于时间戳或版本比较

## 实施顺序

1. 先修复问题 1（简单，1-2 行改动）
2. 再修复问题 2（需要实现 blob 同步逻辑）

## 风险评估

- 问题 1: 低风险，纯 UI 状态同步
- 问题 2: 中等风险，涉及数据同步核心逻辑，需确保不丢数据
