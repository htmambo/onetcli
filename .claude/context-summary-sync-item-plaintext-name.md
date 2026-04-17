## 项目上下文摘要（sync-item-plaintext-name）
生成时间：2026-03-25 16:46:58 +0800

### 1. 相似实现分析
- **实现1**: `crates/core/src/cloud_sync/service.rs`
  - 模式：连接、工作区、凭证的同步上传数据都在这里统一构造
  - 结论：明文名称字段应该在这里一次性写入 `CloudSyncData`

- **实现2**: `crates/core/src/cloud_sync/generic_sync.rs` 与 `connection_sync.rs`
  - 模式：云端名称映射目前依赖解密 `encrypted_data`
  - 结论：新增明文名称后应优先用 `CloudSyncData.name`，旧数据再解密兜底

- **实现3**: `sync_server/server/src/db/database.ts` 与 `sync_server/server/src/http/routes/sync.ts`
  - 模式：`sync_data` 表字段与 API request/response 一一对应
  - 结论：需要数据库迁移、服务端 schema、路由响应同时补字段

- **实现4**: `sync_server/web/src/views/user/*`
  - 模式：列表、预览、详情都直接基于 API 返回的同步项元数据渲染
  - 结论：Web 端只要收到 `name` 字段即可直接展示，不必解密

### 2. 技术结论
- 明文名称应属于同步元数据，放在 `CloudSyncData` 顶层最合适
- 旧数据兼容必须存在：字段缺失时默认空串，名称映射继续走解密兜底
- 旧的未删除记录需要在下次同步时自动补写明文名称，否则迁移后仍然为空
