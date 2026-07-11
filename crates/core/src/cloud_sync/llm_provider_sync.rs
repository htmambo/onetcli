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
    /// - 内置（`OmniHub`）项允许按名称回链，便于跨设备共享同一份全局配置；
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

    /// 回归用例：内置 OmniHub 提供商允许按名称回链（跨设备共享同一份配置）。
    #[test]
    fn builtin_provider_should_link_by_name() {
        let handler = LlmProviderSyncType;
        let item = provider(ProviderType::OmniHub, "OmniHub AI");
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

    // ============================================================================
    // T5：should_link_unlinked_local_by_name 覆盖（fail-fast 断言）
    // ============================================================================

    /// `user_configurable` 列表不应包含任何 builtin 成员；
    /// 同时固定列表长度为 11（防止 enum 增删未更新此处测试而出现策略漂移）。
    /// 失败信息引导维护者同步更新 `ProviderType::is_builtin` 与本测试。
    #[test]
    fn user_configurable_provider_enum_has_no_builtin_member() {
        let user_configurable = ProviderType::user_configurable();
        assert_eq!(
            user_configurable.len(),
            11,
            "user_configurable 数量应为 11；若 ProviderType 新增/删除成员，\
             请同步更新 ProviderType::is_builtin 与本测试的硬编码数量"
        );
        assert!(
            user_configurable
                .iter()
                .all(|pt| !pt.is_builtin()),
            "user_configurable 中不得包含 builtin 成员"
        );
    }

    /// 内置项的回链策略**不依赖**名称字段，仅与 provider_type 绑定。
    /// 验证即便用户重命名 OmniHub 内置项，仍允许按名称回链（跨设备共享全局配置）。
    #[test]
    fn builtin_provider_with_custom_name_still_links() {
        let handler = LlmProviderSyncType;
        let item = provider(ProviderType::OmniHub, "用户重命名后的 OmniHub");
        assert!(
            handler.should_link_unlinked_local_by_name(&item),
            "OmniHub 内置项无论名称如何都应允许按名称回链"
        );
    }

    /// 全员分类一致性：每个 ProviderType 的回链策略应严格等价于 `is_builtin`。
    /// 本测试是 Contract 的"黄金断言"：任何新增/删除 enum 变体、任何对 is_builtin 或
    /// should_link_unlinked_local_by_name 的改动，若让二者不再完全一致，本测试必红。
    /// 同样固定 all() 数量为 12，便于 grep 定位漂移。
    #[test]
    fn every_provider_type_classified_consistently() {
        let all = ProviderType::all();
        assert_eq!(
            all.len(),
            12,
            "ProviderType::all() 数量应为 12；若新增/删除 enum 变体，\
             请同步更新 is_builtin、user_configurable 与本测试"
        );

        let builtin_members: Vec<ProviderType> =
            all.iter().copied().filter(ProviderType::is_builtin).collect();
        assert_eq!(
            builtin_members,
            vec![ProviderType::OmniHub],
            "唯一 builtin 成员应为 OmniHub"
        );

        let handler = LlmProviderSyncType;
        for pt in all {
            let item = provider(pt, "any-name");
            assert_eq!(
                handler.should_link_unlinked_local_by_name(&item),
                pt.is_builtin(),
                "{:?} 的 should_link 策略与 is_builtin 不一致",
                pt
            );
        }
    }
}
