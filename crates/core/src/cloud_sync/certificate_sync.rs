//! 证书同步处理器

use crate::cloud_sync::engine::SyncEngine;
use crate::cloud_sync::models::CloudSyncData;
use crate::cloud_sync::service::{CloudSyncService, SyncError};
use crate::cloud_sync::sync_type::SyncTypeHandler;
use crate::storage::traits::Repository;
use crate::storage::{
    Certificate, CertificateRepository, detach_connections_for_certificate,
    sync_connections_for_certificate,
};

/// 证书同步类型处理器
pub struct CertificateSyncType;

impl SyncTypeHandler for CertificateSyncType {
    type Item = Certificate;

    fn data_type(&self) -> &'static str {
        "certificate"
    }

    fn display_name(&self) -> &'static str {
        "证书"
    }

    fn queue_key(&self) -> &'static str {
        "certificate"
    }

    fn list_local(&self, engine: &SyncEngine) -> Result<Vec<Certificate>, SyncError> {
        let repo = engine
            .storage
            .get::<CertificateRepository>()
            .ok_or_else(|| {
                SyncError::StorageError("CertificateRepository not found".to_string())
            })?;

        repo.list()
            .map_err(|e| SyncError::StorageError(e.to_string()))
    }

    fn insert_local(&self, engine: &SyncEngine, item: &mut Certificate) -> Result<(), SyncError> {
        let repo = engine
            .storage
            .get::<CertificateRepository>()
            .ok_or_else(|| {
                SyncError::StorageError("CertificateRepository not found".to_string())
            })?;

        repo.insert(item)
            .map_err(|e| SyncError::StorageError(e.to_string()))?;
        sync_connections_for_certificate(&engine.storage, item)
            .map_err(|e| SyncError::StorageError(e.to_string()))?;
        Ok(())
    }

    fn update_local_item(&self, engine: &SyncEngine, item: &Certificate) -> Result<(), SyncError> {
        let repo = engine
            .storage
            .get::<CertificateRepository>()
            .ok_or_else(|| {
                SyncError::StorageError("CertificateRepository not found".to_string())
            })?;

        repo.update_from_cloud(item)
            .map_err(|e| SyncError::StorageError(e.to_string()))?;
        sync_connections_for_certificate(&engine.storage, item)
            .map_err(|e| SyncError::StorageError(e.to_string()))?;
        Ok(())
    }

    fn delete_local(&self, engine: &SyncEngine, id: i64) -> Result<(), SyncError> {
        let repo = engine
            .storage
            .get::<CertificateRepository>()
            .ok_or_else(|| {
                SyncError::StorageError("CertificateRepository not found".to_string())
            })?;

        if let Some(certificate) = repo
            .get(id)
            .map_err(|e| SyncError::StorageError(e.to_string()))?
        {
            detach_connections_for_certificate(
                &engine.storage,
                certificate.id,
                certificate.cloud_id.as_deref(),
            )
            .map_err(|e| SyncError::StorageError(e.to_string()))?;
        }

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
            .get::<CertificateRepository>()
            .ok_or_else(|| {
                SyncError::StorageError("CertificateRepository not found".to_string())
            })?;

        repo.update_sync_status(
            local_id,
            Some(cloud_id.to_string()),
            Some(SyncEngine::current_timestamp()),
        )
        .map_err(|e| SyncError::StorageError(e.to_string()))?;

        if let Some(certificate) = repo
            .get(local_id)
            .map_err(|e| SyncError::StorageError(e.to_string()))?
        {
            sync_connections_for_certificate(&engine.storage, &certificate)
                .map_err(|e| SyncError::StorageError(e.to_string()))?;
        }

        Ok(())
    }

    fn decrypt_name(&self, service: &CloudSyncService, data: &CloudSyncData) -> Option<String> {
        service
            .decrypt_sync_data_certificate(data)
            .ok()
            .map(|certificate| certificate.name)
    }

    fn decrypt(
        &self,
        service: &CloudSyncService,
        data: &CloudSyncData,
    ) -> Result<Certificate, SyncError> {
        service.decrypt_sync_data_certificate(data)
    }

    fn encrypt(
        &self,
        service: &CloudSyncService,
        item: &Certificate,
    ) -> Result<CloudSyncData, SyncError> {
        service.prepare_certificate_sync_data_upload(item)
    }

    fn pending_deletion_entity_type(&self) -> &'static str {
        "certificate"
    }
}
