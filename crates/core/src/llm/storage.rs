use anyhow::Result;
use gpui::{App, SharedString};
use rusqlite::params;

use crate::llm::chat_history::{MessageRepository, SessionRepository};
use crate::storage::connection::SqliteConnection;
use crate::storage::row_mapping::FromSqliteRow;
use crate::storage::traits::{Entity, Repository};
use crate::storage::{GlobalStorageState, now};

use super::types::{ProviderConfig, ProviderType};

struct ProviderConfigRow {
    id: i64,
    name: String,
    provider_type: String,
    api_key: Option<String>,
    api_base: Option<String>,
    api_version: Option<String>,
    model: String,
    models: Option<String>,
    max_tokens: Option<i32>,
    temperature: Option<f64>,
    thinking_budget: Option<i32>,
    enabled: i32,
    is_default: i32,
    cloud_id: Option<String>,
    last_synced_at: Option<i64>,
    sync_enabled: i32,
    created_at: i64,
    updated_at: i64,
}

impl FromSqliteRow for ProviderConfigRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(ProviderConfigRow {
            id: row.get("id")?,
            name: row.get("name")?,
            provider_type: row.get("provider_type")?,
            api_key: row.get("api_key")?,
            api_base: row.get("api_base")?,
            api_version: row.get("api_version")?,
            model: row.get("model")?,
            models: row.get("models")?,
            max_tokens: row.get("max_tokens")?,
            temperature: row.get("temperature")?,
            thinking_budget: row.get("thinking_budget")?,
            enabled: row.get("enabled")?,
            is_default: row.get("is_default")?,
            cloud_id: row.get("cloud_id")?,
            last_synced_at: row.get("last_synced_at")?,
            sync_enabled: row.get("sync_enabled")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}

impl TryFrom<ProviderConfigRow> for ProviderConfig {
    type Error = anyhow::Error;

    fn try_from(row: ProviderConfigRow) -> Result<Self> {
        let provider_type = ProviderType::from_str(&row.provider_type)
            .ok_or_else(|| anyhow::anyhow!("Invalid provider type: {}", row.provider_type))?;

        let models = match row.models.as_deref() {
            Some(json) => serde_json::from_str::<Vec<String>>(json).unwrap_or_default(),
            None => Vec::new(),
        };
        let mut models = models;
        if models.is_empty() {
            models.push(row.model.clone());
        } else if !models.iter().any(|m| m == &row.model) {
            models.insert(0, row.model.clone());
        }

        Ok(ProviderConfig {
            id: row.id,
            name: row.name,
            provider_type,
            api_key: row.api_key,
            api_base: row.api_base,
            api_version: row.api_version,
            model: row.model,
            models,
            max_tokens: row.max_tokens,
            temperature: row.temperature.map(|t| t as f32),
            thinking_budget: row.thinking_budget,
            enabled: row.enabled != 0,
            is_default: row.is_default != 0,
            cloud_id: row.cloud_id,
            last_synced_at: row.last_synced_at,
            sync_enabled: row.sync_enabled != 0,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Clone)]
pub struct ProviderRepository {
    conn: SqliteConnection,
}

impl ProviderRepository {
    pub fn new(conn: SqliteConnection) -> Self {
        Self { conn }
    }

    pub fn ensure_onetcli_provider(&self) -> Result<ProviderConfig> {
        // 先查找已有的 OnetCli 类型 provider
        if let Ok(list) = self.list() {
            if let Some(mut existing) = list
                .into_iter()
                .find(|p| p.provider_type == ProviderType::OnetCli)
            {
                if !existing.enabled {
                    existing.enabled = true;
                    let _ = self.update(&existing);
                }
                return Ok(existing);
            }
        }

        // 不存在则创建
        let now = now();
        let has_default = self
            .list()
            .map(|list| list.iter().any(|p| p.is_default))
            .unwrap_or(false);

        let mut config = ProviderConfig {
            id: now,
            name: "OnetCli AI".to_string(),
            provider_type: ProviderType::OnetCli,
            api_key: Some("sk-imtest".to_string()),
            api_base: Some("http://localhost:8000/v1".to_string()),
            api_version: None,
            model: "glm-5".to_string(),
            models: Vec::new(),
            max_tokens: None,
            temperature: None,
            thinking_budget: None,
            enabled: true,
            is_default: !has_default,
            cloud_id: None,
            last_synced_at: None,
            sync_enabled: true,
            created_at: now,
            updated_at: now,
        };

        let _ = self.insert(&mut config);
        Ok(config)
    }

    /// 从云端同步更新（insert_or_update 语义）
    pub fn update_from_cloud(&self, item: &ProviderConfig) -> Result<()> {
        // 默认提供商去重：如果当前项设为默认，先将其他项取消默认
        if item.is_default {
            self.conn.with_connection(|conn| {
                conn.execute(
                    "UPDATE llm_providers SET is_default = 0 WHERE id != ?1",
                    params![item.id],
                )?;
                Ok(())
            })?;
        }

        let id = item.id;
        let name = item.name.clone();
        let provider_type = item.provider_type.as_str().to_string();
        let api_key = item.api_key.clone();
        let api_base = item.api_base.clone();
        let api_version = item.api_version.clone();
        let model = item.model.clone();
        let models = if item.models.is_empty() {
            vec![item.model.clone()]
        } else {
            item.models.clone()
        };
        let models_json = serde_json::to_string(&models).unwrap_or_else(|_| "[]".to_string());
        let max_tokens = item.max_tokens;
        let temperature = item.temperature.map(|t| t as f64);
        let thinking_budget = item.thinking_budget;
        let enabled = if item.enabled { 1i32 } else { 0i32 };
        let is_default = if item.is_default { 1i32 } else { 0i32 };
        let cloud_id = item.cloud_id.clone();
        let last_synced_at = item.last_synced_at;
        let sync_enabled = if item.sync_enabled { 1i32 } else { 0i32 };
        let created_at = item.created_at;
        let updated_at = item.updated_at;

        self.conn.with_connection(|conn| {
            conn.execute(
                "UPDATE llm_providers SET
                    name = ?1, provider_type = ?2, api_key = ?3, api_base = ?4,
                    api_version = ?5, model = ?6, models = ?7, max_tokens = ?8,
                    temperature = ?9, thinking_budget = ?10, enabled = ?11,
                    is_default = ?12, updated_at = ?13, cloud_id = ?14,
                    last_synced_at = ?15, sync_enabled = ?16, created_at = ?17
                 WHERE id = ?18",
                params![
                    name, provider_type, api_key, api_base, api_version, model,
                    models_json, max_tokens, temperature, thinking_budget,
                    enabled, is_default, updated_at, cloud_id, last_synced_at,
                    sync_enabled, created_at, id,
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
                "SELECT id, name, provider_type, api_key, api_base, api_version, model, models, max_tokens, temperature, thinking_budget, enabled, is_default, cloud_id, last_synced_at, sync_enabled, created_at, updated_at FROM llm_providers WHERE cloud_id = ?1",
            )?;
            let mut rows = stmt.query(params![cloud_id])?;
            if let Some(row) = rows.next()? {
                let config_row = ProviderConfigRow::from_row(row)?;
                Ok(Some(config_row.try_into()?))
            } else {
                Ok(None)
            }
        })
    }
}

impl Entity for ProviderConfig {
    fn id(&self) -> Option<i64> {
        Some(self.id)
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }

    fn updated_at(&self) -> i64 {
        self.updated_at
    }
}

impl Repository for ProviderRepository {
    type Entity = ProviderConfig;

    fn entity_type(&self) -> SharedString {
        SharedString::from("ProviderConfig")
    }

    fn insert(&self, item: &mut Self::Entity) -> Result<i64> {
        // 默认提供商去重（兜底）：如果新插入项设为默认，先将所有现有默认项取消。
        // 此路径覆盖「云端下载新增」场景（`to_download` → `insert_local`）：
        // - 同步前本地有默认 A，云端下载新增默认 B，不去重会产生两个默认。
        // - 插入场景下 `item.id` 可能为 0（即将自增），无法用 `id != ?1` 排除，
        //   故直接清空所有默认即可，新插入的行随后会被显式置为 1。
        if item.is_default {
            self.conn.with_connection(|conn| {
                conn.execute("UPDATE llm_providers SET is_default = 0", params![])?;
                Ok(())
            })?;
        }

        let id = item.id;
        let name = item.name.clone();
        let provider_type = item.provider_type.as_str().to_string();
        let api_key = item.api_key.clone();
        let api_base = item.api_base.clone();
        let api_version = item.api_version.clone();
        let model = item.model.clone();
        let models = if item.models.is_empty() {
            vec![item.model.clone()]
        } else {
            item.models.clone()
        };
        let models_json = serde_json::to_string(&models).unwrap_or_else(|_| "[]".to_string());
        let max_tokens = item.max_tokens;
        let temperature = item.temperature.map(|t| t as f64);
        let thinking_budget = item.thinking_budget;
        let enabled = if item.enabled { 1i32 } else { 0i32 };
        let is_default = if item.is_default { 1i32 } else { 0i32 };
        let cloud_id = item.cloud_id.clone();
        let last_synced_at = item.last_synced_at;
        let sync_enabled = if item.sync_enabled { 1i32 } else { 0i32 };
        let created_at = item.created_at;
        let updated_at = item.updated_at;

        let new_id = if id == 0 {
            self.conn.with_connection(|conn| {
                conn.execute(
                    "INSERT INTO llm_providers (name, provider_type, api_key, api_base, api_version, model, models, max_tokens, temperature, thinking_budget, enabled, is_default, cloud_id, last_synced_at, sync_enabled, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                    params![name, provider_type, api_key, api_base, api_version, model, models_json, max_tokens, temperature, thinking_budget, enabled, is_default, cloud_id, last_synced_at, sync_enabled, created_at, updated_at],
                )?;
                Ok(conn.last_insert_rowid())
            })?
        } else {
            self.conn.with_connection(|conn| {
                conn.execute(
                    "INSERT INTO llm_providers (id, name, provider_type, api_key, api_base, api_version, model, models, max_tokens, temperature, thinking_budget, enabled, is_default, cloud_id, last_synced_at, sync_enabled, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
                    params![id, name, provider_type, api_key, api_base, api_version, model, models_json, max_tokens, temperature, thinking_budget, enabled, is_default, cloud_id, last_synced_at, sync_enabled, created_at, updated_at],
                )?;
                Ok(id)
            })?
        };

        item.id = new_id;
        Ok(new_id)
    }

    fn update(&self, item: &Self::Entity) -> Result<()> {
        // 默认提供商去重（兜底）：如果当前项设为默认，先将其他项取消默认。
        // 与 `update_from_cloud` 保持一致，确保"同一时刻仅一个默认"的不变量
        // 在任何调用路径（UI 切换、批量写入、修复脚本）下都被强制约束。
        if item.is_default {
            self.conn.with_connection(|conn| {
                conn.execute(
                    "UPDATE llm_providers SET is_default = 0 WHERE id != ?1",
                    params![item.id],
                )?;
                Ok(())
            })?;
        }

        let id = item.id;
        let name = item.name.clone();
        let provider_type = item.provider_type.as_str().to_string();
        let api_key = item.api_key.clone();
        let api_base = item.api_base.clone();
        let api_version = item.api_version.clone();
        let model = item.model.clone();
        let models = if item.models.is_empty() {
            vec![item.model.clone()]
        } else {
            item.models.clone()
        };
        let models_json = serde_json::to_string(&models).unwrap_or_else(|_| "[]".to_string());
        let max_tokens = item.max_tokens;
        let temperature = item.temperature.map(|t| t as f64);
        let thinking_budget = item.thinking_budget;
        let enabled = if item.enabled { 1i32 } else { 0i32 };
        let is_default = if item.is_default { 1i32 } else { 0i32 };
        let cloud_id = item.cloud_id.clone();
        let last_synced_at = item.last_synced_at;
        let sync_enabled = if item.sync_enabled { 1i32 } else { 0i32 };
        let updated_at = now();

        self.conn.with_connection(|conn| {
            conn.execute(
                "UPDATE llm_providers SET name = ?1, provider_type = ?2, api_key = ?3, api_base = ?4, api_version = ?5, model = ?6, models = ?7, max_tokens = ?8, temperature = ?9, thinking_budget = ?10, enabled = ?11, is_default = ?12, cloud_id = ?13, last_synced_at = ?14, sync_enabled = ?15, updated_at = ?16 WHERE id = ?17",
                params![name, provider_type, api_key, api_base, api_version, model, models_json, max_tokens, temperature, thinking_budget, enabled, is_default, cloud_id, last_synced_at, sync_enabled, updated_at, id],
            )?;
            Ok(())
        })
    }

    fn delete(&self, id: i64) -> Result<()> {
        self.conn.with_connection(|conn| {
            conn.execute("DELETE FROM llm_providers WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    fn get(&self, id: i64) -> Result<Option<Self::Entity>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, provider_type, api_key, api_base, api_version, model, models, max_tokens, temperature, thinking_budget, enabled, is_default, cloud_id, last_synced_at, sync_enabled, created_at, updated_at FROM llm_providers WHERE id = ?1",
            )?;
            let mut rows = stmt.query(params![id])?;
            if let Some(row) = rows.next()? {
                let config_row = ProviderConfigRow::from_row(row)?;
                Ok(Some(config_row.try_into()?))
            } else {
                Ok(None)
            }
        })
    }

    fn list(&self) -> Result<Vec<Self::Entity>> {
        self.conn.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, provider_type, api_key, api_base, api_version, model, models, max_tokens, temperature, thinking_budget, enabled, is_default, cloud_id, last_synced_at, sync_enabled, created_at, updated_at FROM llm_providers ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map([], |row| ProviderConfigRow::from_row(row))?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row?.try_into()?);
            }
            Ok(results)
        })
    }

    fn count(&self) -> Result<i64> {
        self.conn.with_connection(|conn| {
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM llm_providers", [], |row| row.get(0))?;
            Ok(count)
        })
    }

    fn exists(&self, id: i64) -> Result<bool> {
        self.conn.with_connection(|conn| {
            let exists: i64 = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM llm_providers WHERE id = ?1)",
                params![id],
                |row| row.get(0),
            )?;
            Ok(exists == 1)
        })
    }
}

pub fn init(cx: &mut App) {
    let storage_state = cx.global::<GlobalStorageState>();
    let storage = storage_state.storage.clone();

    let conn = storage.connection();
    let provider_repo = ProviderRepository::new(conn.clone());
    let session_repo = SessionRepository::new(conn.clone());
    let message_repo = MessageRepository::new(conn);

    storage.register(provider_repo);
    storage.register(session_repo);
    storage.register(message_repo);
}
