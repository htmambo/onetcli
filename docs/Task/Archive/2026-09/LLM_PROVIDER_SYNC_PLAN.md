# LLM 提供商配置接入云同步 — 实现方案

## 1. 方案概述

将 `ProviderConfig`（LLM 提供商配置）接入现有云同步体系，使其可以在多设备间同步。采用与 `CertificateSyncType` 相同的通用同步流程（`generic_sync`），服务器端零改动。

---

## 2. 关键结论

| 层级 | 是否需要改动 | 说明 |
|------|-------------|------|
| **sync_server（Node.js）** | ❌ 无需改动 | `data_type` 是纯字符串透传，无枚举校验 |
| **Rust 客户端（core）** | ✅ 需要改动 | 数据模型改造 + SyncTypeHandler 实现 + 引擎注册 |
| **Web 管理后台** | ⚠️ 可选但建议 | 新增类型标签和明文展示解析器 |
| **本地数据库（SQLite）** | ✅ 需要改动 | 新增 `cloud_id`、`last_synced_at`、`sync_enabled` 列 |

---

## 3. 数据模型改造

### 3.1 ProviderConfig 新增同步字段

**文件**: `crates/core/src/llm/types.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: i64,
    pub name: String,
    pub provider_type: ProviderType,
    pub api_key: Option<String>,
    pub api_base: Option<String>,
    pub api_version: Option<String>,
    pub model: String,
    pub models: Vec<String>,
    pub max_tokens: Option<i32>,
    pub temperature: Option<f32>,
    pub thinking_budget: Option<i32>,
    pub enabled: bool,
    pub is_default: bool,
    // ===== 新增同步字段 =====
    pub cloud_id: Option<String>,
    pub last_synced_at: Option<i64>,
    pub sync_enabled: bool,
    // =======================
    pub created_at: i64,
    pub updated_at: i64,
}
```

> 注意：`id` 保持为 `i64`（非 `Option<i64>`），在 `SyncableItem` 实现中通过 `Some(self.id)` 适配。

### 3.2 实现 SyncableItem Trait

**文件**: `crates/core/src/llm/types.rs`（或新建 `crates/core/src/llm/sync.rs`）

```rust
use crate::cloud_sync::sync_type::SyncableItem;

impl SyncableItem for ProviderConfig {
    fn local_id(&self) -> Option<i64> {
        Some(self.id)
    }

    fn set_local_id(&mut self, id: Option<i64>) {
        if let Some(id) = id {
            self.id = id;
        }
    }

    fn item_name(&self) -> &str {
        &self.name
    }

    fn cloud_id(&self) -> Option<&str> {
        self.cloud_id.as_deref()
    }

    fn set_cloud_id(&mut self, cloud_id: Option<String>) {
        self.cloud_id = cloud_id;
    }

    fn updated_at(&self) -> Option<i64> {
        Some(self.updated_at)
    }

    fn last_synced_at(&self) -> Option<i64> {
        self.last_synced_at
    }

    // 使用简单时间戳比较（与 Certificate 一致）
    fn uses_sync_state(&self) -> bool {
        false
    }
}
```

### 3.3 本地数据库 Schema 迁移

**文件**: `crates/core/src/storage/migrations/`（找到最新的迁移文件或在 `connection.rs` 中确保表存在）

当前 `llm_providers` 表需要新增列：

```sql
ALTER TABLE llm_providers ADD COLUMN cloud_id TEXT;
ALTER TABLE llm_providers ADD COLUMN last_synced_at INTEGER;
ALTER TABLE llm_providers ADD COLUMN sync_enabled INTEGER NOT NULL DEFAULT 1;
```

> 如果项目使用启动时自动建表/改表机制，在 `llm/storage.rs` 或迁移文件中更新 `CREATE TABLE` 语句。

---

## 4. Repository 扩展

**文件**: `crates/core/src/llm/storage.rs`

需要在 `ProviderRepository` 中新增以下方法：

```rust
impl ProviderRepository {
    /// 从云端同步更新（insert_or_update 语义）
    pub fn update_from_cloud(&self, item: &ProviderConfig) -> Result<()> {
        let id = item.id;
        self.conn.with_connection(|conn| {
            conn.execute(
                "UPDATE llm_providers SET
                    name = ?1, provider_type = ?2, api_key = ?3, api_base = ?4,
                    api_version = ?5, model = ?6, models = ?7, max_tokens = ?8,
                    temperature = ?9, thinking_budget = ?10, enabled = ?11,
                    is_default = ?12, updated_at = ?13, cloud_id = ?14,
                    last_synced_at = ?15, sync_enabled = ?16
                 WHERE id = ?17",
                params![
                    item.name, item.provider_type.to_string(), item.api_key,
                    item.api_base, item.api_version, item.model,
                    serde_json::to_string(&item.models).unwrap_or_default(),
                    item.max_tokens, item.temperature, item.thinking_budget,
                    item.enabled as i32, item.is_default as i32,
                    item.updated_at, item.cloud_id, item.last_synced_at,
                    item.sync_enabled as i32, id,
                ],
            )?;
            Ok(())
        })
    }

    /// 更新同步状态（上传成功后调用）
    pub fn update_sync_status(
        &self,
        id: i64,
        cloud_id: Option<String>,
        last_synced_at: Option<i64>,
    ) -> Result<()> {
        self.conn.with_connection(|conn| {
            conn.execute(
                "UPDATE llm_providers SET cloud_id = ?1, last_synced_at = ?2 WHERE id = ?3",
                params![cloud_id, last_synced_at, id],
            )?;
            Ok(())
        })
    }

    /// 按 cloud_id 查询（用于下载时去重）
    pub fn get_by_cloud_id(&self, cloud_id: &str) -> Result<Option<ProviderConfig>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM llm_providers WHERE cloud_id = ?1"
            )?;
            let mut rows = stmt.query(params![cloud_id])?;
            if let Some(row) = rows.next()? {
                let row_data: ProviderConfigRow = FromSqliteRow::from_row(row)?;
                Ok(Some(row_data.try_into()?))
            } else {
                Ok(None)
            }
        })
    }
}
```

---

## 5. 云端数据模型与加解密

### 5.1 新增 data_type 常量

**文件**: `crates/core/src/cloud_sync/models.rs`

```rust
pub mod data_type {
    pub const CONNECTION: &str = "connection";
    pub const WORKSPACE: &str = "workspace";
    pub const CERTIFICATE: &str = "certificate";
    // ===== 新增 =====
    pub const LLM_PROVIDER: &str = "llm_provider";
    // ================
}
```

### 5.2 新增 PlainData 结构

**文件**: `crates/core/src/cloud_sync/models.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProviderPlainData {
    pub name: String,
    pub provider_type: String,
    pub api_key: Option<String>,
    pub api_base: Option<String>,
    pub api_version: Option<String>,
    pub model: String,
    pub models: Vec<String>,
    pub max_tokens: Option<i32>,
    pub temperature: Option<f32>,
    pub thinking_budget: Option<i32>,
    pub enabled: bool,
    pub is_default: bool,
    pub owner_id: Option<String>,
}
```

### 5.3 CloudSyncService 加解密方法

**文件**: `crates/core/src/cloud_sync/service.rs`

参照 `prepare_certificate_sync_data_upload` 和 `decrypt_sync_data_certificate` 实现：

```rust
impl CloudSyncService {
    pub(crate) fn prepare_llm_provider_sync_data_upload(
        &self,
        item: &ProviderConfig,
    ) -> Result<CloudSyncData, SyncError> {
        let plain_data = LlmProviderPlainData {
            name: item.name.clone(),
            provider_type: item.provider_type.to_string(),
            api_key: item.api_key.clone(),
            api_base: item.api_base.clone(),
            api_version: item.api_version.clone(),
            model: item.model.clone(),
            models: item.models.clone(),
            max_tokens: item.max_tokens,
            temperature: item.temperature,
            thinking_budget: item.thinking_budget,
            enabled: item.enabled,
            is_default: item.is_default,
            owner_id: None,
        };

        let json = serde_json::to_vec(&plain_data)
            .map_err(|e| SyncError::SerializationError(e.to_string()))?;

        let encrypted = self.encrypt_data(&json)?;
        let checksum = calculate_checksum(&json);

        Ok(CloudSyncData {
            id: item.cloud_id.clone().unwrap_or_default(),
            data_type: data_type::LLM_PROVIDER.to_string(),
            name: item.name.clone(),
            encrypted_data: base64_encode(&encrypted),
            key_version: self.key_version(),
            checksum,
            version: 1,
            created_at: timestamp_ms(),
            updated_at: timestamp_ms(),
            deleted_at: None,
        })
    }

    pub(crate) fn decrypt_sync_data_llm_provider(
        &self,
        cloud_data: &CloudSyncData,
    ) -> Result<ProviderConfig, SyncError> {
        let encrypted = base64_decode(&cloud_data.encrypted_data)
            .map_err(|e| SyncError::DeserializationError(e.to_string()))?;
        let decrypted = self.decrypt_data(&encrypted)?;

        let plain_data: LlmProviderPlainData = serde_json::from_slice(&decrypted)
            .map_err(|e| SyncError::DeserializationError(e.to_string()))?;

        Ok(ProviderConfig {
            id: 0, // 本地插入时重新分配
            name: plain_data.name,
            provider_type: ProviderType::from_str(&plain_data.provider_type)
                .unwrap_or(ProviderType::OpenAi),
            api_key: plain_data.api_key,
            api_base: plain_data.api_base,
            api_version: plain_data.api_version,
            model: plain_data.model,
            models: plain_data.models,
            max_tokens: plain_data.max_tokens,
            temperature: plain_data.temperature,
            thinking_budget: plain_data.thinking_budget,
            enabled: plain_data.enabled,
            is_default: plain_data.is_default,
            cloud_id: Some(cloud_data.id.clone()),
            last_synced_at: Some(cloud_data.updated_at / 1000),
            sync_enabled: true,
            created_at: cloud_data.created_at / 1000,
            updated_at: cloud_data.updated_at / 1000,
        })
    }
}
```

---

## 6. 同步处理器实现

### 6.1 新建文件

**文件**: `crates/core/src/cloud_sync/llm_provider_sync.rs`

```rust
use crate::cloud_sync::models::{CloudSyncData, data_type};
use crate::cloud_sync::service::CloudSyncService;
use crate::cloud_sync::sync_type::{SyncTypeHandler, SyncableItem};
use crate::cloud_sync::{SyncEngine, SyncError};
use crate::llm::types::ProviderConfig;
use crate::storage::traits::Repository;

pub(crate) struct LlmProviderSyncType;

impl SyncTypeHandler for LlmProviderSyncType {
    type Item = ProviderConfig;

    fn data_type(&self) -> &'static str {
        data_type::LLM_PROVIDER
    }

    fn display_name(&self) -> &'static str {
        "LLM 提供商"
    }

    fn queue_key(&self) -> &'static str {
        "llm_provider"
    }

    fn list_local(&self, engine: &SyncEngine) -> Result<Vec<ProviderConfig>, SyncError> {
        let repo = engine.storage.get::<crate::llm::storage::ProviderRepository>()
            .ok_or_else(|| SyncError::StorageError("ProviderRepository not found".to_string()))?;
        repo.list().map_err(|e| SyncError::StorageError(e.to_string()))
    }

    fn insert_local(&self, engine: &SyncEngine, item: &mut ProviderConfig) -> Result<(), SyncError> {
        let repo = engine.storage.get::<crate::llm::storage::ProviderRepository>()
            .ok_or_else(|| SyncError::StorageError("ProviderRepository not found".to_string()))?;
        repo.insert(item).map_err(|e| SyncError::StorageError(e.to_string()))
    }

    fn update_local_item(&self, engine: &SyncEngine, item: &ProviderConfig) -> Result<(), SyncError> {
        let repo = engine.storage.get::<crate::llm::storage::ProviderRepository>()
            .ok_or_else(|| SyncError::StorageError("ProviderRepository not found".to_string()))?;
        repo.update_from_cloud(item).map_err(|e| SyncError::StorageError(e.to_string()))
    }

    fn delete_local(&self, engine: &SyncEngine, id: i64) -> Result<(), SyncError> {
        let repo = engine.storage.get::<crate::llm::storage::ProviderRepository>()
            .ok_or_else(|| SyncError::StorageError("ProviderRepository not found".to_string()))?;
        repo.delete(id).map_err(|e| SyncError::StorageError(e.to_string()))
    }

    fn on_uploaded(&self, engine: &SyncEngine, local_id: i64, cloud_id: &str) -> Result<(), SyncError> {
        let repo = engine.storage.get::<crate::llm::storage::ProviderRepository>()
            .ok_or_else(|| SyncError::StorageError("ProviderRepository not found".to_string()))?;
        repo.update_sync_status(local_id, Some(cloud_id.to_string()), Some(SyncEngine::current_timestamp()))
            .map_err(|e| SyncError::StorageError(e.to_string()))
    }

    fn decrypt_name(&self, service: &CloudSyncService, data: &CloudSyncData) -> Option<String> {
        service.decrypt_sync_data_llm_provider(data).ok().map(|c| c.name)
    }

    fn decrypt(&self, service: &CloudSyncService, data: &CloudSyncData) -> Result<ProviderConfig, SyncError> {
        service.decrypt_sync_data_llm_provider(data)
    }

    fn encrypt(&self, service: &CloudSyncService, item: &ProviderConfig) -> Result<CloudSyncData, SyncError> {
        service.prepare_llm_provider_sync_data_upload(item)
    }

    fn pending_deletion_entity_type(&self) -> &'static str {
        "llm_provider"
    }
}
```

### 6.2 注册到模块系统

**文件**: `crates/core/src/cloud_sync/mod.rs`

```rust
mod llm_provider_sync;
```

### 6.3 注册到 SyncEngine

**文件**: `crates/core/src/cloud_sync/engine.rs`

1. 顶部新增 `use`：
   ```rust
   use super::llm_provider_sync::LlmProviderSyncType;
   ```

2. `handlers` vec 中新增：
   ```rust
   handlers: vec![
       Box::new(TypedSyncBridge { handler: WorkspaceSyncType }),
       Box::new(TypedSyncBridge { handler: CertificateSyncType }),
       Box::new(TypedSyncBridge { handler: LlmProviderSyncType }),
       Box::new(ConnectionSyncHandler),
   ],
   ```

---

## 7. Web 管理后台（可选但建议）

### 7.1 类型标签映射

**文件**: `sync_server/web/src/utils/syncItemType.ts`

```typescript
const syncItemTypeAliases: Record<string, string> = {
  connection: "connection",
  workspace: "workspace",
  certificate: "credential",
  credential: "credential",
  llm_provider: "llm_provider", // 新增
};

const syncItemTypeLabels: Record<string, string> = {
  connection: "连接项",
  workspace: "工作区",
  credential: "凭证",
  llm_provider: "LLM 提供商", // 新增
};
```

### 7.2 明文展示解析器

**文件**: `sync_server/web/src/utils/syncPayloadTable.ts`

```typescript
function parseLlmProvider(data: any): { key: string; value: string }[] {
  return [
    { key: "名称", value: data.name || "-" },
    { key: "类型", value: data.provider_type || "-" },
    { key: "模型", value: data.model || "-" },
    { key: "API Base", value: data.api_base || "-" },
    { key: "启用", value: data.enabled ? "是" : "否" },
    { key: "默认", value: data.is_default ? "是" : "否" },
  ];
}

// 在 buildPayloadTable 的 switch 中新增：
switch (type) {
  // ... 现有 case
  case "llm_provider":
    return parseLlmProvider(data);
  default:
    return parseGeneric(data);
}
```

---

## 8. 冲突处理策略

采用与 `Certificate` 相同的**简单时间戳比较策略**：

- `uses_sync_state()` 返回 `false`
- 同步引擎在 `decide_linked_sync_action` 中直接比较 `local_updated_at` 和 `cloud_updated_at`
- 不依赖 `last_synced_at` 做精细变更检测

这是合理的，因为 LLM 提供商配置修改频率低，简单时间戳比较已足够。

---

## 9. 安全与隐私考量

| 字段 | 处理方式 | 说明 |
|------|---------|------|
| `api_key` | 加密后上传到云端 | 随 `LlmProviderPlainData` 整体加密，安全性与其他同步数据一致 |
| `api_base` | 加密后上传 | 可能包含内部域名，建议加密 |
| `model` / `models` | 加密后上传 | 明文相对不敏感，但保持统一加密 |

云端始终存储加密后的 blob，服务器不解析业务内容。

---

## 10. 实现步骤清单（按顺序）

1. [ ] **数据库 Schema**：给 `llm_providers` 表添加 `cloud_id`、`last_synced_at`、`sync_enabled` 列
2. [ ] **数据模型**：`ProviderConfig` 新增同步字段，实现 `SyncableItem` trait
3. [ ] **Repository**：`ProviderRepository` 新增 `update_from_cloud`、`update_sync_status`、`get_by_cloud_id`
4. [ ] **云端模型**：`data_type` 模块新增常量，`LlmProviderPlainData` 结构体
5. [ ] **加解密**：`CloudSyncService` 新增 `prepare_llm_provider_sync_data_upload` 和 `decrypt_sync_data_llm_provider`
6. [ ] **同步处理器**：新建 `llm_provider_sync.rs`，实现 `LlmProviderSyncType`
7. [ ] **引擎注册**：`mod.rs` + `engine.rs` 注册新 handler
8. [ ] **Web 后台**：类型标签和明文展示解析器（可选）
9. [ ] **编译验证**：`cargo check -p one-core`
10. [ ] **功能测试**：添加/修改 LLM 提供商后执行同步，验证多设备同步

---

## 11. 风险与注意点

1. **`id` 类型差异**：`ProviderConfig.id` 是 `i64`（非 `Option<i64>`），在 `SyncableItem::local_id()` 中需要 `Some(self.id)` 适配。插入云端下载的数据时，`id` 由本地 SQLite 自增分配，`cloud_id` 用于关联云端记录。
2. **`ProviderType` 反序列化**：`LlmProviderPlainData.provider_type` 是字符串，反序列化时需调用 `ProviderType::from_str()`，若遇到未知类型需有 fallback 策略。
3. **默认提供商冲突**：`is_default` 字段同步后可能出现多设备都有 `is_default = true` 的情况。建议在 `update_from_cloud` 或同步完成后，在本地做默认提供商去重（只有一个 `is_default = true`）。
4. **API Key 安全**：确保 `api_key` 包含在加密 blob 中，不要作为明文字段上传到 `CloudSyncData.name` 等位置。
