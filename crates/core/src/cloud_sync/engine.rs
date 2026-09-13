//! 云同步引擎
//!
//! 统一管理同步流程，将同步逻辑从 UI 层解耦。
//!
//! ## 设计原则
//!
//! - 参考 Dropbox Nucleus 架构的三棵树模型
//! - 支持冲突检测和多种解决策略
//! - 提供完整同步和增量同步两种模式

use super::blob_vault::BlobVault;
use super::certificate_sync::CertificateSyncType;
use super::client::CloudApiClient;
use super::connection_sync::ConnectionSyncHandler;
use super::generic_sync::generic_sync;
use super::llm_provider_sync::LlmProviderSyncType;
use super::models::{ConflictResolution, ConflictType, SyncResult};
use super::queue::OperationQueue;
use super::service::{CloudSyncService, SyncError};
use super::sync_backend::SyncBackend;
use super::sync_type::SyncTypeHandler;
use super::workspace_sync::WorkspaceSyncType;
use crate::crypto;
use crate::storage::StorageManager;
use crate::storage::traits::Repository;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

pub type SyncFuture<'a> = Pin<Box<dyn Future<Output = Result<SyncResult, SyncError>> + Send + 'a>>;

pub(crate) trait SyncHandler: Send + Sync {
    fn name(&self) -> &'static str;
    fn sync<'a>(&'a self, engine: &'a SyncEngine) -> SyncFuture<'a>;
}

/// 泛型桥接器：将 `SyncTypeHandler` 适配为 `SyncHandler`
///
/// 通过 `generic_sync` 通用流程执行同步，供 `one-core` 内部的类型化同步使用。
struct TypedSyncBridge<H: SyncTypeHandler> {
    handler: H,
}

impl<H: SyncTypeHandler> SyncHandler for TypedSyncBridge<H> {
    fn name(&self) -> &'static str {
        self.handler.display_name()
    }

    fn sync<'a>(&'a self, engine: &'a SyncEngine) -> SyncFuture<'a> {
        Box::pin(generic_sync(engine, &self.handler))
    }
}

/// 同步引擎
///
/// 核心职责：
/// 1. 协调本地存储和云端 API 的交互
/// 2. 计算同步计划，检测冲突
/// 3. 执行同步操作并更新状态
pub struct SyncEngine {
    /// 云端 API 客户端
    pub(crate) cloud_client: Arc<dyn CloudApiClient>,
    /// 加解密服务
    pub(crate) crypto_service: Arc<std::sync::RwLock<CloudSyncService>>,
    /// 本地存储管理器
    pub(crate) storage: StorageManager,
    /// 冲突解决策略
    pub(crate) conflict_strategy: ConflictResolution,
    pub(crate) handlers: Vec<Box<dyn SyncHandler>>,
    /// Blob 存储后端（可选，WebDAV/S3 等）
    pub(crate) blob_vault: Option<Arc<dyn BlobVault>>,
    /// 同步后端策略
    backend: Arc<dyn SyncBackend>,
}

impl SyncEngine {
    /// 创建新的同步引擎
    pub fn new(
        cloud_client: Arc<dyn CloudApiClient>,
        crypto_service: Arc<std::sync::RwLock<CloudSyncService>>,
        storage: StorageManager,
    ) -> Self {
        Self {
            cloud_client,
            crypto_service,
            storage,
            conflict_strategy: ConflictResolution::UseCloud, // 默认使用云端版本
            handlers: vec![
                Box::new(TypedSyncBridge {
                    handler: WorkspaceSyncType,
                }),
                Box::new(TypedSyncBridge {
                    handler: CertificateSyncType,
                }),
                Box::new(TypedSyncBridge {
                    handler: LlmProviderSyncType,
                }),
                Box::new(ConnectionSyncHandler),
            ],
            blob_vault: None,
            backend: Arc::new(super::sync_backend::SyncServerBackend),
        }
    }

    /// 设置可选的 Blob 存储后端
    pub fn with_opt_blob_vault(mut self, vault: Option<Arc<dyn BlobVault>>) -> Self {
        self.blob_vault = vault;
        self
    }

    /// 设置同步后端策略
    pub fn with_backend(mut self, backend: Arc<dyn SyncBackend>) -> Self {
        self.backend = backend;
        self
    }

    /// 获取当前时间戳（秒）
    pub(crate) fn current_timestamp() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    /// 确保加密服务已解锁
    pub(crate) fn ensure_unlocked(&self) -> Result<(), SyncError> {
        // 如果本地 crypto 模块已解锁但同步服务未解锁，同步密钥状态
        if crypto::has_master_key() {
            if let Some(raw_key) = crypto::raw_master_key_for_sync() {
                let mut service_write = self
                    .crypto_service
                    .write()
                    .map_err(|_| SyncError::StorageError("同步服务锁获取失败".to_string()))?;
                if !service_write.is_unlocked() {
                    tracing::info!("[同步引擎] 从本地 crypto 模块同步密钥状态");
                    service_write.set_master_key_directly(raw_key);
                }
            }
        }

        let service = self
            .crypto_service
            .read()
            .map_err(|_| SyncError::StorageError("同步服务锁获取失败".to_string()))?;

        if !service.is_unlocked() {
            return Err(SyncError::NotUnlocked);
        }

        Ok(())
    }

    /// 确保个人主密钥配置已经同步到云端
    ///
    /// 首次同步时自动创建 `user_config`，后续同步则从云端恢复正确的 `key_version`。
    pub(crate) async fn ensure_personal_key_config(&self) -> Result<(), SyncError> {
        let raw_key = crypto::raw_master_key_for_sync().ok_or(SyncError::NotUnlocked)?;
        let cloud_config = self
            .cloud_client
            .get_user_config()
            .await
            .map_err(|e| SyncError::NetworkError(e.to_string()))?;

        match cloud_config {
            Some(config) => {
                let unlock_result = {
                    let mut service = self
                        .crypto_service
                        .write()
                        .map_err(|_| SyncError::StorageError("同步服务锁获取失败".to_string()))?;

                    if service.key_version() != config.key_version {
                        tracing::info!(
                            "[同步引擎] 从云端用户配置恢复 key_version={}",
                            config.key_version
                        );
                    }

                    service.unlock(&raw_key, &config)
                };

                match unlock_result {
                    Ok(()) => Ok(()),
                    Err(SyncError::InvalidMasterKey) => {
                        let cloud_items = self
                            .cloud_client
                            .list_sync_data(None, None)
                            .await
                            .map_err(|e| SyncError::NetworkError(e.to_string()))?;

                        if cloud_items.is_empty() {
                            tracing::warn!(
                                "[同步引擎] 云端密钥配置与当前主密钥不匹配，但账号下没有同步数据，自动重建云端密钥配置"
                            );

                            let new_config = {
                                let mut service = self.crypto_service.write().map_err(|_| {
                                    SyncError::StorageError("同步服务锁获取失败".to_string())
                                })?;
                                service.setup_master_key(&raw_key)?
                            };

                            self.cloud_client
                                .save_user_config(&new_config)
                                .await
                                .map_err(|e| SyncError::NetworkError(e.to_string()))?;

                            Ok(())
                        } else {
                            Err(SyncError::CloudMasterKeyMismatch(
                                "云端同步密钥与当前本地主密钥不一致，请使用原主密钥解锁，或清空该账号的云端同步数据后重试"
                                    .to_string(),
                            ))
                        }
                    }
                    Err(error) => Err(error),
                }
            }
            None => {
                let config = {
                    let mut service = self
                        .crypto_service
                        .write()
                        .map_err(|_| SyncError::StorageError("同步服务锁获取失败".to_string()))?;

                    if service.key_version() >= 1 {
                        return Ok(());
                    }

                    tracing::info!("[同步引擎] 云端缺少用户密钥配置，自动初始化");
                    service.setup_master_key(&raw_key)?
                };

                self.cloud_client
                    .save_user_config(&config)
                    .await
                    .map_err(|e| SyncError::NetworkError(e.to_string()))?;

                Ok(())
            }
        }
    }

    /// 执行完整同步
    ///
    /// 根据后端策略路由到相应流程：
    /// - `SyncServerBackend`：通过 REST API 同步（团队列表 + handlers）
    /// - `BlobVaultBackend`：通过 BlobVault 加密 blob 同步
    pub async fn sync(&self) -> Result<SyncResult, SyncError> {
        tracing::info!(
            "========== 开始云同步 (后端: {}) ==========",
            self.backend.backend_type()
        );

        self.backend.init_crypto(self).await?;

        self.backend.execute_sync(self).await
    }

    pub(crate) fn take_operation_queue(&self, key: &str) -> Result<OperationQueue, SyncError> {
        let mut service = self
            .crypto_service
            .write()
            .map_err(|_| SyncError::StorageError("同步服务锁获取失败".to_string()))?;

        Ok(service.take_operation_queue(key))
    }

    pub(crate) fn store_operation_queue(
        &self,
        key: &str,
        queue: OperationQueue,
    ) -> Result<(), SyncError> {
        let mut service = self
            .crypto_service
            .write()
            .map_err(|_| SyncError::StorageError("同步服务锁获取失败".to_string()))?;

        service.store_operation_queue(key, queue);
        Ok(())
    }

    /// 使用指定的策略映射应用冲突解决方案
    ///
    /// 允许为每个冲突单独指定解决策略，而不是使用全局策略
    pub async fn apply_conflict_resolutions(
        &self,
        conflicts: Vec<crate::cloud_sync::models::SyncConflict>,
        strategies: std::collections::HashMap<String, ConflictResolution>,
    ) -> Result<SyncResult, SyncError> {
        self.ensure_unlocked()?;
        self.ensure_personal_key_config().await?;

        let mut result = SyncResult::default();

        // 为每个冲突应用指定的策略
        for conflict in &conflicts {
            let cloud_id = &conflict.cloud.id;
            let strategy = strategies
                .get(cloud_id)
                .copied()
                .unwrap_or(self.conflict_strategy);

            let resolved_action = self.create_resolved_action(conflict, strategy);

            if let Err(e) = self.apply_single_conflict(&resolved_action).await {
                result.errors.push(format!("应用冲突解决失败: {}", e));
                result.conflicts.push(conflict.clone());
            }
        }

        Ok(result)
    }

    /// 创建冲突解决操作
    fn create_resolved_action(
        &self,
        conflict: &crate::cloud_sync::models::SyncConflict,
        strategy: ConflictResolution,
    ) -> crate::cloud_sync::connection_sync::ResolvedConflictAction {
        use crate::cloud_sync::connection_sync::ResolvedConflictAction;

        match strategy {
            ConflictResolution::UseCloud => ResolvedConflictAction {
                conflict: conflict.clone(),
                resolution: ConflictResolution::UseCloud,
                result_connection: None,
            },
            ConflictResolution::UseLocal => ResolvedConflictAction {
                conflict: conflict.clone(),
                resolution: ConflictResolution::UseLocal,
                result_connection: Some(conflict.local.clone()),
            },
            ConflictResolution::KeepBoth => {
                let mut copy = conflict.local.clone();
                copy.id = None;
                copy.cloud_id = None;
                copy.last_synced_at = None;
                let timestamp = Self::current_timestamp();
                copy.name = format!("{} (冲突副本 {})", copy.name, timestamp);

                ResolvedConflictAction {
                    conflict: conflict.clone(),
                    resolution: ConflictResolution::KeepBoth,
                    result_connection: Some(copy),
                }
            }
        }
    }

    /// 应用单个冲突解决方案
    async fn apply_single_conflict(
        &self,
        resolved: &crate::cloud_sync::connection_sync::ResolvedConflictAction,
    ) -> Result<(), SyncError> {
        use crate::storage::ConnectionRepository;
        use crate::storage::traits::Repository;

        match resolved.resolution {
            ConflictResolution::UseCloud => {
                if resolved.conflict.conflict_type == ConflictType::LocalModifiedCloudDeleted {
                    return self.delete_local_conflict_connection(resolved.conflict.local.id);
                }

                // 更新本地连接
                let mut updated =
                    self.build_local_connection_from_cloud(&resolved.conflict.cloud)?;
                updated.id = resolved.conflict.local.id;
                updated.cloud_id = Some(resolved.conflict.cloud.id.clone());
                updated.last_synced_at = Some(Self::current_timestamp());

                let repo = self.storage.get::<ConnectionRepository>().ok_or_else(|| {
                    SyncError::StorageError("ConnectionRepository not found".to_string())
                })?;

                repo.update(&updated)
                    .map_err(|e| SyncError::StorageError(e.to_string()))?;

                Ok(())
            }
            ConflictResolution::UseLocal => {
                if resolved.conflict.conflict_type == ConflictType::LocalModifiedCloudDeleted
                    && resolved.conflict.cloud.version < 1
                {
                    return self
                        .recreate_cloud_from_local(&resolved.conflict.local)
                        .await;
                }

                // 更新云端连接
                let mut updated_data =
                    self.prepare_connection_sync_data_upload(&resolved.conflict.local)?;
                updated_data.id = resolved.conflict.cloud.id.clone();
                updated_data.version = resolved.conflict.cloud.version;

                self.cloud_client
                    .update_sync_data(&updated_data)
                    .await
                    .map_err(|e| SyncError::NetworkError(e.to_string()))?;

                self.mark_connection_synced(
                    resolved.conflict.local.id,
                    Some(resolved.conflict.cloud.id.clone()),
                )?;

                Ok(())
            }
            ConflictResolution::KeepBoth => {
                if resolved.conflict.conflict_type == ConflictType::LocalModifiedCloudDeleted {
                    return self
                        .recreate_cloud_from_local(&resolved.conflict.local)
                        .await;
                }

                // 创建本地副本
                if let Some(copy) = &resolved.result_connection {
                    let repo = self.storage.get::<ConnectionRepository>().ok_or_else(|| {
                        SyncError::StorageError("ConnectionRepository not found".to_string())
                    })?;

                    let mut new_conn = copy.clone();
                    repo.insert(&mut new_conn)
                        .map_err(|e| SyncError::StorageError(e.to_string()))?;
                }

                // 同时更新本地连接为云端版本
                let mut updated =
                    self.build_local_connection_from_cloud(&resolved.conflict.cloud)?;
                updated.id = resolved.conflict.local.id;
                updated.cloud_id = Some(resolved.conflict.cloud.id.clone());
                updated.last_synced_at = Some(Self::current_timestamp());

                let repo = self.storage.get::<ConnectionRepository>().ok_or_else(|| {
                    SyncError::StorageError("ConnectionRepository not found".to_string())
                })?;

                repo.update(&updated)
                    .map_err(|e| SyncError::StorageError(e.to_string()))?;

                Ok(())
            }
        }
    }

    fn mark_connection_synced(
        &self,
        local_id: Option<i64>,
        cloud_id: Option<String>,
    ) -> Result<(), SyncError> {
        let local_id = local_id.ok_or_else(|| {
            SyncError::StorageError("连接缺少本地 ID，无法更新同步状态".to_string())
        })?;

        let repo = self
            .storage
            .get::<crate::storage::ConnectionRepository>()
            .ok_or_else(|| SyncError::StorageError("ConnectionRepository not found".to_string()))?;

        repo.update_sync_status(local_id, cloud_id, Some(Self::current_timestamp()))
            .map_err(|e| SyncError::StorageError(e.to_string()))
    }

    fn delete_local_conflict_connection(&self, local_id: Option<i64>) -> Result<(), SyncError> {
        let local_id = local_id.ok_or_else(|| {
            SyncError::StorageError("连接缺少本地 ID，无法应用云端删除结果".to_string())
        })?;

        let repo = self
            .storage
            .get::<crate::storage::ConnectionRepository>()
            .ok_or_else(|| SyncError::StorageError("ConnectionRepository not found".to_string()))?;

        repo.delete(local_id)
            .map_err(|e| SyncError::StorageError(e.to_string()))
    }

    async fn recreate_cloud_from_local(
        &self,
        local: &crate::storage::StoredConnection,
    ) -> Result<(), SyncError> {
        let cloud_data = self.prepare_connection_sync_data_upload(local)?;
        let created = self
            .cloud_client
            .create_sync_data(&cloud_data)
            .await
            .map_err(|e| SyncError::NetworkError(e.to_string()))?;

        self.mark_connection_synced(local.id, Some(created.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud_sync::client::{AuthResponse, CloudApiClient, CloudApiError};
    use crate::cloud_sync::models::{CloudSyncData, ConflictType, SyncConflict, data_type};
    use crate::llm::ChatStream;
    use crate::storage::traits::Repository;
    use crate::storage::{ConnectionRepository, ConnectionType, StoredConnection};
    use async_trait::async_trait;
    use llm_connector::ChatRequest;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex, OnceLock};

    #[derive(Default)]
    struct MockCloudClient {
        created_items: Mutex<Vec<CloudSyncData>>,
        updated_items: Mutex<Vec<CloudSyncData>>,
        user_config: Mutex<Option<crate::cloud_sync::CloudUserConfig>>,
    }

    #[async_trait]
    impl CloudApiClient for MockCloudClient {
        async fn sign_in_with_password(
            &self,
            _email: &str,
            _password: &str,
        ) -> Result<AuthResponse, CloudApiError> {
            Err(CloudApiError::NotAuthenticated)
        }

        async fn sign_up(
            &self,
            _email: &str,
            _password: &str,
        ) -> Result<AuthResponse, CloudApiError> {
            Err(CloudApiError::NotAuthenticated)
        }

        async fn sign_out(&self) -> Result<(), CloudApiError> {
            Ok(())
        }

        async fn get_current_user(
            &self,
        ) -> Result<Option<crate::cloud_sync::UserInfo>, CloudApiError> {
            Ok(None)
        }

        async fn refresh_token(&self, _refresh_token: &str) -> Result<AuthResponse, CloudApiError> {
            Err(CloudApiError::NotAuthenticated)
        }

        async fn get_user_config(
            &self,
        ) -> Result<Option<crate::cloud_sync::CloudUserConfig>, CloudApiError> {
            Ok(self
                .user_config
                .lock()
                .expect("mock cloud client mutex poisoned")
                .clone())
        }

        async fn save_user_config(
            &self,
            _config: &crate::cloud_sync::CloudUserConfig,
        ) -> Result<(), CloudApiError> {
            Ok(())
        }

        async fn list_models(&self) -> Result<Vec<String>, CloudApiError> {
            Ok(Vec::new())
        }

        async fn list_sync_data(
            &self,
            _data_type: Option<&str>,
            _since: Option<i64>,
        ) -> Result<Vec<CloudSyncData>, CloudApiError> {
            Ok(Vec::new())
        }

        async fn create_sync_data(
            &self,
            data: &CloudSyncData,
        ) -> Result<CloudSyncData, CloudApiError> {
            self.created_items
                .lock()
                .expect("mock cloud client mutex poisoned")
                .push(data.clone());
            Ok(data.clone())
        }

        async fn update_sync_data(
            &self,
            data: &CloudSyncData,
        ) -> Result<CloudSyncData, CloudApiError> {
            self.updated_items
                .lock()
                .expect("mock cloud client mutex poisoned")
                .push(data.clone());
            Ok(data.clone())
        }

        async fn delete_sync_data(&self, _id: &str) -> Result<(), CloudApiError> {
            Ok(())
        }

        async fn chat(&self, _request: &ChatRequest) -> Result<String, CloudApiError> {
            Err(CloudApiError::NotFound("not implemented".to_string()))
        }

        async fn chat_stream(&self, _request: &ChatRequest) -> Result<ChatStream, CloudApiError> {
            Err(CloudApiError::NotFound("not implemented".to_string()))
        }
    }

    fn test_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    // S4：env::set_var / env::remove_var 在新版本 Rust 标记 unsafe；仅测试用。
    #[allow(unsafe_code)]
    fn setup_test_storage(temp_home: &PathBuf) -> crate::storage::StorageManager {
        let previous_home = std::env::var_os("HOME");
        unsafe {
            std::env::set_var("HOME", temp_home);
        }

        let storage =
            crate::storage::StorageManager::new().expect("should create isolated storage manager");
        storage.register(ConnectionRepository::new(storage.connection()));

        match previous_home {
            Some(value) => unsafe {
                std::env::set_var("HOME", value);
            },
            None => unsafe {
                std::env::remove_var("HOME");
            },
        }

        storage
    }

    #[test]
    fn use_local_conflict_resolution_updates_local_sync_status() {
        let _lock = test_env_lock()
            .lock()
            .expect("test env mutex should not be poisoned");

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let temp_home = std::env::temp_dir().join(format!("one-core-engine-test-{unique}"));

        crate::crypto::test_support::set_master_key("test-master-key");
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime should be created");
        runtime.block_on(async {
            let storage = setup_test_storage(&temp_home);
            let repo = storage
                .get::<ConnectionRepository>()
                .expect("connection repository should be registered");

            let mut local = StoredConnection {
                id: None,
                name: "冲突连接".to_string(),
                connection_type: ConnectionType::Database,
                params: "{}".to_string(),
                sort_order: None,
                workspace_id: None,
                selected_databases: None,
                remark: None,
                sync_enabled: true,
                cloud_id: Some("cloud-connection-1".to_string()),
                last_synced_at: Some(1),
                created_at: None,
                updated_at: None,
                owner_id: Some("user-1".to_string()),
            };
            repo.insert(&mut local)
                .expect("should insert local connection");
            let local_id = local.id.expect("insert should set local id");
            repo.update_sync_status(local_id, Some("cloud-connection-1".to_string()), Some(1))
                .expect("should set initial sync status");

            let local = repo
                .get(local_id)
                .expect("repo get should succeed")
                .expect("local connection should exist");
            let cloud = CloudSyncData {
                id: "cloud-connection-1".to_string(),
                owner_id: "user-1".to_string(),
                data_type: data_type::CONNECTION.to_string(),
                name: "冲突连接".to_string(),
                encrypted_data: String::new(),
                key_version: 1,
                checksum: String::new(),
                version: 2,
                updated_at: 2_000,
                deleted_at: None,
            };

            let conflict = SyncConflict {
                local: local.clone(),
                cloud: cloud.clone(),
                cloud_name: "冲突连接".to_string(),
                conflict_type: ConflictType::BothModified,
            };

            let cloud_client = Arc::new(MockCloudClient::default());
            let mut service = crate::cloud_sync::CloudSyncService::new();
            service.set_master_key_directly(Zeroizing::new("test-master-key".to_string()));
            // SyncEngine 走全局 crypto 主密钥（crypto::get_raw_master_key），
            // 仅设 service 层密钥不够；挂内存后端 + 全局锁避免与 crypto 单测竞态
            let _key_guard = crate::crypto::test_support::lock();
            crate::crypto::test_support::set_master_key("test-master-key");
            service.set_logged_in("user-1".to_string());

            let engine = SyncEngine::new(
                cloud_client.clone(),
                Arc::new(std::sync::RwLock::new(service)),
                storage.clone(),
            );

            let mut strategies = HashMap::new();
            strategies.insert(cloud.id.clone(), ConflictResolution::UseLocal);

            let result = engine
                .apply_conflict_resolutions(vec![conflict], strategies)
                .await
                .expect("conflict resolution should succeed");

            assert!(
                result.errors.is_empty(),
                "unexpected resolution errors: {:?}",
                result.errors
            );
            assert!(
                result.conflicts.is_empty(),
                "resolved conflict should not remain in unresolved list"
            );

            let updated = repo
                .get(local_id)
                .expect("repo get should succeed")
                .expect("updated local connection should exist");
            assert_eq!(updated.cloud_id.as_deref(), Some("cloud-connection-1"));
            assert_ne!(updated.last_synced_at, Some(1));
            assert!(updated.last_synced_at.is_some());

            let sent = cloud_client
                .updated_items
                .lock()
                .expect("mock cloud client mutex poisoned");
            assert_eq!(sent.len(), 1);
            assert_eq!(sent[0].id, "cloud-connection-1");
        });

        crate::crypto::test_support::clear_master_key();
        let _ = std::fs::remove_dir_all(temp_home);
    }

    #[test]
    fn use_local_deleted_cloud_conflict_recreates_remote_item() {
        let _lock = test_env_lock()
            .lock()
            .expect("test env mutex should not be poisoned");

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let temp_home = std::env::temp_dir().join(format!("one-core-engine-test-{unique}"));

        crate::crypto::test_support::set_master_key("test-master-key");
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime should be created");
        runtime.block_on(async {
            let storage = setup_test_storage(&temp_home);
            let repo = storage
                .get::<ConnectionRepository>()
                .expect("connection repository should be registered");

            let mut local = StoredConnection {
                id: None,
                name: "本地连接".to_string(),
                connection_type: ConnectionType::Database,
                params: "{}".to_string(),
                sort_order: None,
                workspace_id: None,
                selected_databases: None,
                remark: None,
                sync_enabled: true,
                cloud_id: Some("deleted-cloud-id".to_string()),
                last_synced_at: Some(10),
                created_at: None,
                updated_at: Some(20),
                owner_id: Some("user-1".to_string()),
            };
            repo.insert(&mut local)
                .expect("should insert local connection");
            let local_id = local.id.expect("insert should set local id");

            let conflict =
                crate::cloud_sync::conflict::ConflictResolver::detect_local_modified_cloud_deleted(
                    &local,
                    "deleted-cloud-id",
                );

            let verification = crate::crypto::generate_key_verification_v1("test-master-key");
            let cloud_client = Arc::new(MockCloudClient {
                user_config: Mutex::new(Some(crate::cloud_sync::CloudUserConfig {
                    user_id: "user-1".to_string(),
                    key_verification: verification,
                    key_version: 7,
                    updated_at: 1,
                })),
                ..Default::default()
            });

            let mut service = crate::cloud_sync::CloudSyncService::new();
            service.set_master_key_directly(Zeroizing::new("test-master-key".to_string()));
            // SyncEngine 走全局 crypto 主密钥（crypto::get_raw_master_key），
            // 仅设 service 层密钥不够；挂内存后端 + 全局锁避免与 crypto 单测竞态
            let _key_guard = crate::crypto::test_support::lock();
            crate::crypto::test_support::set_master_key("test-master-key");
            service.set_logged_in("user-1".to_string());

            let engine = SyncEngine::new(
                cloud_client.clone(),
                Arc::new(std::sync::RwLock::new(service)),
                storage.clone(),
            );

            let mut strategies = HashMap::new();
            strategies.insert(conflict.cloud.id.clone(), ConflictResolution::UseLocal);

            let result = engine
                .apply_conflict_resolutions(vec![conflict], strategies)
                .await
                .expect("conflict resolution should succeed");

            assert!(
                result.errors.is_empty(),
                "unexpected errors: {:?}",
                result.errors
            );
            assert!(result.conflicts.is_empty(), "conflict should be resolved");

            let created = cloud_client
                .created_items
                .lock()
                .expect("mock cloud client mutex poisoned");
            assert_eq!(created.len(), 1);
            assert_eq!(created[0].key_version, 7);

            let updated = repo
                .get(local_id)
                .expect("repo get should succeed")
                .expect("local connection should still exist");
            assert_eq!(updated.cloud_id.as_deref(), Some(created[0].id.as_str()));
            assert!(updated.last_synced_at.is_some());

            let sent_updates = cloud_client
                .updated_items
                .lock()
                .expect("mock cloud client mutex poisoned");
            assert!(sent_updates.is_empty(), "should create instead of update");
        });

        crate::crypto::test_support::clear_master_key();
        let _ = std::fs::remove_dir_all(temp_home);
    }

    #[test]
    fn use_cloud_deleted_cloud_conflict_deletes_local_connection() {
        let _lock = test_env_lock()
            .lock()
            .expect("test env mutex should not be poisoned");

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let temp_home = std::env::temp_dir().join(format!("one-core-engine-test-{unique}"));

        crate::crypto::test_support::set_master_key("test-master-key");
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime should be created");
        runtime.block_on(async {
            let storage = setup_test_storage(&temp_home);
            let repo = storage
                .get::<ConnectionRepository>()
                .expect("connection repository should be registered");

            let mut local = StoredConnection {
                id: None,
                name: "待删除连接".to_string(),
                connection_type: ConnectionType::Database,
                params: "{}".to_string(),
                sort_order: None,
                workspace_id: None,
                selected_databases: None,
                remark: None,
                sync_enabled: true,
                cloud_id: Some("deleted-cloud-id".to_string()),
                last_synced_at: Some(10),
                created_at: None,
                updated_at: Some(20),
                owner_id: Some("user-1".to_string()),
            };
            repo.insert(&mut local)
                .expect("should insert local connection");
            let local_id = local.id.expect("insert should set local id");

            let conflict =
                crate::cloud_sync::conflict::ConflictResolver::detect_local_modified_cloud_deleted(
                    &local,
                    "deleted-cloud-id",
                );

            let cloud_client = Arc::new(MockCloudClient::default());
            let mut service = crate::cloud_sync::CloudSyncService::new();
            service.set_master_key_directly(Zeroizing::new("test-master-key".to_string()));
            // SyncEngine 走全局 crypto 主密钥（crypto::get_raw_master_key），
            // 仅设 service 层密钥不够；挂内存后端 + 全局锁避免与 crypto 单测竞态
            let _key_guard = crate::crypto::test_support::lock();
            crate::crypto::test_support::set_master_key("test-master-key");
            service.set_logged_in("user-1".to_string());

            let engine = SyncEngine::new(
                cloud_client,
                Arc::new(std::sync::RwLock::new(service)),
                storage.clone(),
            );

            let mut strategies = HashMap::new();
            strategies.insert(conflict.cloud.id.clone(), ConflictResolution::UseCloud);

            let result = engine
                .apply_conflict_resolutions(vec![conflict], strategies)
                .await
                .expect("conflict resolution should succeed");

            assert!(
                result.errors.is_empty(),
                "unexpected errors: {:?}",
                result.errors
            );
            assert!(
                repo.get(local_id)
                    .expect("repo get should succeed")
                    .is_none(),
                "local connection should be deleted when accepting cloud deletion"
            );
        });

        crate::crypto::test_support::clear_master_key();
        let _ = std::fs::remove_dir_all(temp_home);
    }
}
