//! LLM 提供商配置同步处理器

use crate::cloud_sync::engine::SyncEngine;
use crate::cloud_sync::models::CloudSyncData;
use crate::cloud_sync::service::{CloudSyncService, SyncError};
use crate::cloud_sync::sync_type::SyncTypeHandler;
use crate::llm::storage::ProviderRepository;
use crate::llm::types::ProviderConfig;
use crate::storage::traits::Repository;

/// LLM 提供商配置同步类型处理器
pub(crate) struct LlmProviderSyncType;

impl SyncTypeHandler for LlmProviderSyncType {
    type Item = ProviderConfig;

    fn data_type(&self) -> &'static str {
        "llm_provider"
    }

    fn display_name(&self) -> &'static str {
        "LLM 提供商"
    }

    fn queue_key(&self) -> &'static str {
        "llm_provider"
    }

    fn list_local(&self, engine: &SyncEngine) -> Result<Vec<ProviderConfig>, SyncError> {
        let repo = engine
            .storage
            .get::<ProviderRepository>()
            .ok_or_else(|| SyncError::StorageError("ProviderRepository not found".to_string()))?;

        repo.list()
            .map_err(|e| SyncError::StorageError(e.to_string()))
    }

    fn insert_local(
        &self,
        engine: &SyncEngine,
        item: &mut ProviderConfig,
    ) -> Result<(), SyncError> {
        let repo = engine
            .storage
            .get::<ProviderRepository>()
            .ok_or_else(|| SyncError::StorageError("ProviderRepository not found".to_string()))?;

        repo.insert(item)
            .map_err(|e| SyncError::StorageError(e.to_string()))?;
        Ok(())
    }

    fn update_local_item(
        &self,
        engine: &SyncEngine,
        item: &ProviderConfig,
    ) -> Result<(), SyncError> {
        let repo = engine
            .storage
            .get::<ProviderRepository>()
            .ok_or_else(|| SyncError::StorageError("ProviderRepository not found".to_string()))?;

        repo.update_from_cloud(item)
            .map_err(|e| SyncError::StorageError(e.to_string()))
    }

    fn delete_local(&self, engine: &SyncEngine, id: i64) -> Result<(), SyncError> {
        let repo = engine
            .storage
            .get::<ProviderRepository>()
            .ok_or_else(|| SyncError::StorageError("ProviderRepository not found".to_string()))?;

        repo.delete(id)
            .map_err(|e| SyncError::StorageError(e.to_string()))
    }

    fn on_uploaded(
        &self,
        engine: &SyncEngine,
        local_id: i64,
        cloud_id: &str,
    ) -> Result<(), SyncError> {
        let repo = engine
            .storage
            .get::<ProviderRepository>()
            .ok_or_else(|| SyncError::StorageError("ProviderRepository not found".to_string()))?;

        repo.update_sync_status(
            local_id,
            Some(cloud_id.to_string()),
            Some(SyncEngine::current_timestamp()),
        )
        .map_err(|e| SyncError::StorageError(e.to_string()))
    }

    fn decrypt_name(&self, service: &CloudSyncService, data: &CloudSyncData) -> Option<String> {
        service
            .decrypt_sync_data_llm_provider(data)
            .ok()
            .map(|provider| provider.name)
    }

    fn decrypt(
        &self,
        service: &CloudSyncService,
        data: &CloudSyncData,
    ) -> Result<ProviderConfig, SyncError> {
        service.decrypt_sync_data_llm_provider(data)
    }

    fn encrypt(
        &self,
        service: &CloudSyncService,
        item: &ProviderConfig,
    ) -> Result<CloudSyncData, SyncError> {
        service.prepare_llm_provider_sync_data_upload(item)
    }

    fn pending_deletion_entity_type(&self) -> &'static str {
        "llm_provider"
    }
}
