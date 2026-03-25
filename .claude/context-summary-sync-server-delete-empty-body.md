## 项目上下文摘要（sync-server-delete-empty-body）
生成时间：2026-03-25 16:26:06 +0800

### 1. 根因
- `crates/core/src/cloud_sync/sync_server.rs::common_headers()` 给所有请求统一附加了 `Content-Type: application/json`
- `delete_json_with_retry(...)` 发送的是无 body 的 `DELETE`
- `sync_server` 基于 Fastify，请求头声明 JSON 但 body 为空时会报 `FST_ERR_CTP_EMPTY_JSON_BODY`

### 2. 修复方向
- 公共请求头只保留 `Accept: application/json`
- 仅在真正存在请求体时自动补 `Content-Type: application/json`

### 3. 影响范围
- 修复工作区/连接/凭证待删除同步重试时的 `DELETE` 请求
- 不影响登录、注册、保存配置、创建/更新同步项等有 JSON body 的接口
