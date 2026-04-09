# Netcatty 同步功能集成实施计划

**状态**: 🔄 进行中 (开始时间: 2026-04-08)

## 目标

将 Netcatty 的多云存储同步功能集成到 onetcli，分阶段实施：

1. **P0-1**: CloudAdapter 统一接口（BlobVault trait 已部分完成，需完善）
2. **P1-1**: WebDAV Adapter UI 暴露（后端已实现，缺设置界面）
3. **P0-2**: 三路合并算法 Rust 实现
4. **P0-3**: Auto-sync 定时器
5. **P1-2**: S3 Adapter 实现

---

## 子任务详情

### T1: P0-1 — CloudAdapter 统一接口完善

**状态**: ✅ 已完成

**已完成**:
- `BlobVault` trait 定义 ✅
- `SyncServerBlobVault` no-op 实现 ✅
- `WebDavVault` 后端实现（使用 `HttpClient`） ✅
- `CloudApiClient::as_blob_vault()` 默认方法 ✅
- `SyncEngine` 添加 `blob_vault: Option<Arc<dyn BlobVault>>` 字段 ✅
- `SyncEngine::with_blob_vault()` / `blob_vault()` / `upload_blob` / `download_blob` 等便捷方法 ✅
- `webdav_adapter` 模块公开导出 ✅

**涉及文件**:
- `crates/core/src/cloud_sync/client.rs` — 添加 `as_blob_vault()`
- `crates/core/src/cloud_sync/engine.rs` — 注入 `Arc<dyn BlobVault>`
- `crates/core/src/cloud_sync/webdav_adapter.rs` — 重写使用 `HttpClient`
- `crates/core/src/cloud_sync/mod.rs` — 导出 `webdav_adapter`

**验收标准**: ✅ `SyncEngine` 可使用任意 `BlobVault` 实现进行数据存储

---

### T2: P1-1 — WebDAV 配置 UI

**状态**: ✅ 已完成

**已完成**:
- `WebDavConfig` / `WebDavVault` 后端实现 ✅
- `WebDavSettings` 配置结构体 + `Default` 实现 ✅
- `AppSettings` 添加 `sync_backend_type` + `webdav_config` 字段 ✅
- 设置界面 General → Sync 分组增加"存储后端类型"下拉框 ✅
- WebDAV 配置项（endpoint、认证方式、用户名密码、Bearer Token、存储路径） ✅
- `webdav_adapter` 公开导出 ✅

**待完成**（可选，下次会话）:
- `CloudSyncService` / `SyncEngine` 初始化时根据配置创建对应 `BlobVault`（需连接 auth service）
- 设置项按后端类型动态显示/隐藏

**涉及文件**:
- `main/src/setting_tab.rs` — UI 扩展
- `crates/core/src/cloud_sync/mod.rs` — 导出 `webdav_adapter`

**验收标准**: ✅ 用户可在设置界面配置 WebDAV 存储后端

---

### T3: P0-2 — 三路合并算法

**状态**: ✅ 已完成

**实现内容**:
- `ThreeWayMergeResult<T>` — 合并结果枚举（`Merged` / `Deleted`）
- `ThreeWayMerger<T>` — 泛型三路合并器，支持任意实体类型
- `fingerprint()` — 递归键排序 JSON fingerprint，确保相同内容产生相同指纹
- `merge_entity()` — 核心合并逻辑：
  - 纯新增 → 保留
  - 纯删除 → 标记删除
  - 两边都改 → 优先本地（冲突安全优先）
  - 一方改一方删 → 保留修改（安全优先）
  - 相同修改 → 保留任一
- 完整单元测试覆盖

**涉及文件**:
- `crates/core/src/cloud_sync/conflict.rs` — 新增 `ThreeWayMerger` 及测试

**验收标准**: ✅ entity 级三路合并，比现有版本号冲突解决更智能

---

### T4: P0-3 — Auto-sync 定时器

**状态**: ✅ 已完成

**实现内容**:
- `SyncStateManager` 新增字段：`sync_interval_secs`、`last_sync_at`、`running`、`stop_tx`
- `set_sync_interval(seconds)` — 设置同步间隔，0 表示禁用
- `last_sync_at()` / `update_last_sync()` — 上次同步时间追踪
- `start_auto_sync()` — 启动定时器，基于 `tokio::time::interval`
- `stop_auto_sync()` — 停止定时器

**涉及文件**:
- `crates/core/src/cloud_sync/state_manager.rs` — 添加定时器

**验收标准**: ✅ 可配置间隔自动同步

---

### T5: P1-2 — S3 Adapter

**状态**: ⏳ 未开始

**依赖**: T1（统一接口）完成后实施

**实现内容**:
- `S3Config` / `S3Vault` 实现 `BlobVault` trait
- 支持任意 S3 兼容存储（MinIO、COS、OBS 等）
- 使用 `aws-sdk-s3` crate

**涉及文件**:
- `crates/core/src/cloud_sync/s3_adapter.rs`（新建）

**验收标准**: S3 存储后端可用

---

### T6: GitHub Gist 端到端集成

**状态**: ✅ 已完成

**实现内容**:
- `GithubGistSettings` 配置结构（client_id + gist_id）
- `BlobVault` trait 完整实现（upload/download/delete/exists/list）
- `authenticate()` 方法串联 Device Flow：获取 token → 查找/创建 vault gist
- OAuth token 通过 base64 编解码存储到 gist
- UI 下拉框增加 `github_gist` 选项
- GitHub Gist 配置面板（client_id 输入 + gist_id 显示）
- `SyncEngine::with_blob_vault()` 接入 GitHub Gist vault

**涉及文件**:
- `crates/core/src/cloud_sync/oauth/github_gist.rs` — BlobVault impl + authenticate
- `crates/core/src/cloud_sync/mod.rs` — 导出 GithubGistSettings/Vault
- `main/src/setting_tab.rs` — GistSettings 结构 + UI 配置区
- `main/src/home_tab.rs` — SyncEngine 注入 vault
- `main/locales/main.yml` — 国际化文案

**验收标准**: ✅ GitHub Gist 可作为同步后端，端到端可用

---

## 实施顺序

```
T1 (P0-1) → T2 (P1-1 UI) → T3 (P0-2) → T4 (P0-3) → T5 (P1-2)
```

T1 是基础，T2 是用户最迫切的需求（"看不到新增的同步服务器"），优先完成。

---

## 当前进度

| 任务 | 状态 |
|------|------|
| T1: CloudAdapter 统一接口 | ✅ 已完成 |
| T2: WebDAV 配置 UI | ✅ 已完成 |
| T3: 三路合并算法 | ✅ 已完成 |
| T4: Auto-sync 定时器 | ✅ 已完成 |
| T5: S3 Adapter | ⏳ 未开始 |
| T6: GitHub Gist 端到端集成 | ✅ 已完成 |
