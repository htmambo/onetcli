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

    /// LLM Provider 的按名称回链策略：
    /// - 内置（`OnetCli`）项允许按名称回链，便于跨设备共享同一份全局配置；
    /// - 用户自建项**不**回链，避免本地新增的 API Key/模型等被云端旧值
    ///   通过 `update_from_cloud` 静默覆盖（"新增的也会消失"）。
    fn should_link_unlinked_local_by_name(&self, item: &ProviderConfig) -> bool {
        item.provider_type.is_builtin()
    }
}

#[cfg(test)]
mod tests {
    use super::LlmProviderSyncType;
    use crate::cloud_sync::sync_type::SyncTypeHandler;
    use crate::llm::types::{ProviderConfig, ProviderType};

    fn provider(provider_type: ProviderType, name: &str) -> ProviderConfig {
        ProviderConfig {
            id: 0,
            name: name.to_string(),
            provider_type,
            ..Default::default()
        }
    }

    /// 回归用例：内置 OnetCli 提供商允许按名称回链（跨设备共享同一份配置）。
    #[test]
    fn builtin_provider_should_link_by_name() {
        let handler = LlmProviderSyncType;
        let item = provider(ProviderType::OnetCli, "OnetCli AI");
        assert!(handler.should_link_unlinked_local_by_name(&item));
    }

    /// 回归用例：用户自建提供商不允许按名称回链，避免云端旧值覆盖本地新增。
    /// 覆盖 OpenAI / Anthropic / OpenAICompatible / AzureOpenAI 等所有 user_configurable 类型。
    #[test]
    fn user_providers_should_not_link_by_name() {
        let handler = LlmProviderSyncType;
        for pt in ProviderType::user_configurable() {
            let item = provider(pt, "user-provider");
            assert!(
                !handler.should_link_unlinked_local_by_name(&item),
                "{:?} 应当禁用按名称回链以保护本地新增数据",
                pt
            );
        }
    }
}
