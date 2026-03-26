//! 工作空间同步处理器
//!
//! 通过实现 `SyncTypeHandler` trait，将工作空间同步逻辑接入通用同步流程 `generic_sync`。

use crate::cloud_sync::engine::SyncEngine;
use crate::cloud_sync::models::{CloudSyncData, Team};
use crate::cloud_sync::service::{CloudSyncService, SyncError};
use crate::cloud_sync::sync_type::{PendingDeletionDecision, SyncTypeHandler, SyncableItem};
use crate::storage::traits::Repository;
use crate::storage::{ConnectionRepository, PendingCloudDeletion, Workspace, WorkspaceRepository};

/// 工作空间同步类型处理器
pub struct WorkspaceSyncType;

impl WorkspaceSyncType {
    fn restore_workspace_after_remote_update(
        &self,
        engine: &SyncEngine,
        pending: &PendingCloudDeletion,
        current_cloud: &CloudSyncData,
    ) -> Result<(), SyncError> {
        let workspace_repo = engine
            .storage
            .get::<WorkspaceRepository>()
            .ok_or_else(|| SyncError::StorageError("WorkspaceRepository not found".to_string()))?;
        let connection_repo = engine
            .storage
            .get::<ConnectionRepository>()
            .ok_or_else(|| SyncError::StorageError("ConnectionRepository not found".to_string()))?;

        let restored_workspace =
            match workspace_repo
                .get_by_cloud_id(&current_cloud.id)
                .map_err(|e| SyncError::StorageError(e.to_string()))?
            {
                Some(existing) => {
                    let mut restored = {
                        let service = engine.crypto_service.read().map_err(|_| {
                            SyncError::StorageError("同步服务锁获取失败".to_string())
                        })?;
                        self.decrypt(&service, current_cloud)?
                    };
                    restored.id = existing.id;
                    workspace_repo
                        .update_from_cloud(&restored)
                        .map_err(|e| SyncError::StorageError(e.to_string()))?;
                    workspace_repo
                        .get_by_cloud_id(&current_cloud.id)
                        .map_err(|e| SyncError::StorageError(e.to_string()))?
                        .ok_or_else(|| SyncError::StorageError("工作空间恢复后丢失".to_string()))?
                }
                None => {
                    let mut restored = {
                        let service = engine.crypto_service.read().map_err(|_| {
                            SyncError::StorageError("同步服务锁获取失败".to_string())
                        })?;
                        self.decrypt(&service, current_cloud)?
                    };
                    workspace_repo
                        .insert(&mut restored)
                        .map_err(|e| SyncError::StorageError(e.to_string()))?;
                    restored
                }
            };

        let restored_workspace_id = restored_workspace
            .id
            .ok_or_else(|| SyncError::StorageError("恢复后的工作空间缺少本地 ID".to_string()))?;

        if let Some(metadata) = &pending.metadata {
            for connection_id in &metadata.affected_connection_ids {
                let Some(mut connection) = connection_repo
                    .get(*connection_id)
                    .map_err(|e| SyncError::StorageError(e.to_string()))?
                else {
                    continue;
                };

                let should_restore = connection.workspace_id.is_none()
                    && connection.updated_at.unwrap_or(0) <= pending.created_at;
                if !should_restore {
                    continue;
                }

                connection.workspace_id = Some(restored_workspace_id);
                connection_repo
                    .update(&connection)
                    .map_err(|e| SyncError::StorageError(e.to_string()))?;
            }
        }

        Ok(())
    }
}

impl SyncTypeHandler for WorkspaceSyncType {
    type Item = Workspace;

    fn data_type(&self) -> &'static str {
        "workspace"
    }

    fn display_name(&self) -> &'static str {
        "工作空间"
    }

    fn queue_key(&self) -> &'static str {
        "workspace"
    }

    fn list_local(&self, engine: &SyncEngine) -> Result<Vec<Workspace>, SyncError> {
        let repo = engine
            .storage
            .get::<WorkspaceRepository>()
            .ok_or_else(|| SyncError::StorageError("WorkspaceRepository not found".to_string()))?;

        repo.list()
            .map_err(|e| SyncError::StorageError(e.to_string()))
    }

    fn insert_local(&self, engine: &SyncEngine, item: &mut Workspace) -> Result<(), SyncError> {
        let repo = engine
            .storage
            .get::<WorkspaceRepository>()
            .ok_or_else(|| SyncError::StorageError("WorkspaceRepository not found".to_string()))?;

        repo.insert(item)
            .map_err(|e| SyncError::StorageError(e.to_string()))?;

        Ok(())
    }

    fn update_local_item(&self, engine: &SyncEngine, item: &Workspace) -> Result<(), SyncError> {
        let repo = engine
            .storage
            .get::<WorkspaceRepository>()
            .ok_or_else(|| SyncError::StorageError("WorkspaceRepository not found".to_string()))?;

        repo.update_from_cloud(item)
            .map_err(|e| SyncError::StorageError(e.to_string()))
    }

    fn delete_local(&self, engine: &SyncEngine, id: i64) -> Result<(), SyncError> {
        let repo = engine
            .storage
            .get::<WorkspaceRepository>()
            .ok_or_else(|| SyncError::StorageError("WorkspaceRepository not found".to_string()))?;

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
            .get::<WorkspaceRepository>()
            .ok_or_else(|| SyncError::StorageError("WorkspaceRepository not found".to_string()))?;

        repo.update_sync_status(
            local_id,
            Some(cloud_id.to_string()),
            Some(SyncEngine::current_timestamp()),
        )
        .map_err(|e| SyncError::StorageError(e.to_string()))
    }

    fn decrypt_name(&self, service: &CloudSyncService, data: &CloudSyncData) -> Option<String> {
        service
            .decrypt_sync_data_workspace(data)
            .ok()
            .map(|ws| ws.name)
    }

    fn decrypt(
        &self,
        service: &CloudSyncService,
        data: &CloudSyncData,
    ) -> Result<Workspace, SyncError> {
        service.decrypt_sync_data_workspace(data)
    }

    fn encrypt(
        &self,
        service: &CloudSyncService,
        item: &Workspace,
        teams: &[Team],
    ) -> Result<CloudSyncData, SyncError> {
        service.prepare_workspace_sync_data_upload(item, item.team_id(), teams)
    }

    fn pending_deletion_entity_type(&self) -> &'static str {
        "workspace"
    }

    fn decide_pending_deletion(
        &self,
        engine: &SyncEngine,
        pending: &PendingCloudDeletion,
        current_cloud: Option<&CloudSyncData>,
    ) -> Result<PendingDeletionDecision, SyncError> {
        let Some(current_cloud) = current_cloud else {
            return Ok(PendingDeletionDecision::DeleteCloud);
        };

        if current_cloud.deleted_at.is_some() {
            return Ok(PendingDeletionDecision::DropPending);
        }

        let Some(base_last_synced_at) = pending.base_last_synced_at else {
            return Ok(PendingDeletionDecision::DeleteCloud);
        };

        let cloud_updated_at = current_cloud.updated_at / 1000;
        if cloud_updated_at > base_last_synced_at {
            self.restore_workspace_after_remote_update(engine, pending, current_cloud)?;
            return Ok(PendingDeletionDecision::DropPending);
        }

        Ok(PendingDeletionDecision::DeleteCloud)
    }
}
