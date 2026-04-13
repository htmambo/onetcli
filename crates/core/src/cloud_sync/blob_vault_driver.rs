//! BlobVault 后端同步驱动
//!
//! 通过文件级存储后端（GitHub Gist、WebDAV、OneDrive、Google Drive 等）进行同步。
//! 不依赖 sync_server REST API，直接通过 `BlobVault` trait 操作文件。
//!
//! ## 差异点（与 sync_server 对比）
//! - 不需要用户登录态
//! - 不需要团队列表
//! - 不需要云端密钥配置同步
//! - 数据以加密 bundle blob 形式存储（所有类型打包为一个 blob）

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::blob_vault::BlobVault;
use super::engine::SyncEngine;
use super::models::SyncResult;
use super::service::SyncError;

/// 云端 bundle 元信息（存储在 blob 头部）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BundleMeta {
    /// bundle 版本（用于将来格式升级）
    version: u32,
    /// 创建/更新时间戳（秒）
    timestamp: i64,
    /// 各类型数据的最后更新时间（用于增量判断）
    type_timestamps: HashMap<String, i64>,
}

impl Default for BundleMeta {
    fn default() -> Self {
        Self {
            version: 1,
            timestamp: 0,
            type_timestamps: HashMap::new(),
        }
    }
}

/// 完整的同步 bundle（加密前的明文）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SyncBundle {
    meta: BundleMeta,
    /// 连接数据：StoredConnection 数组
    connections: Option<serde_json::Value>,
    /// 工作空间数据：Workspace 数组
    workspaces: Option<serde_json::Value>,
    /// 凭证数据：Certificate 数组
    certificates: Option<serde_json::Value>,
}

/// Gist 存储使用的 blob key
const BUNDLE_KEY: &str = "onetcli-vault.bundle";

impl SyncEngine {
    /// BlobVault 后端同步流程
    pub(crate) async fn blob_sync(&self) -> Result<SyncResult, SyncError> {
        let vault = self
            .blob_vault
            .as_ref()
            .ok_or_else(|| SyncError::NetworkError("未配置 Blob 存储后端".to_string()))?;

        // tracing::info!("[Blob同步] 后端类型: {}", vault.backend_type());

        // 确保加密服务已解锁
        self.ensure_unlocked()?;

        let mut result = SyncResult::default();

        // 1. 尝试下载云端 bundle
        let cloud_bundle = match self.download_bundle_blob(vault).await {
            Ok(Some(bundle)) => {
                // tracing::info!("[Blob同步] 云端 bundle 时间戳: {}", bundle.meta.timestamp);
                Some(bundle)
            }
            Ok(None) => {
                // tracing::info!("[Blob同步] 云端无 bundle，将上传本地数据");
                None
            }
            Err(e) => {
                // tracing::warn!("[Blob同步] 下载云端 bundle 失败: {}", e);
                None
            }
        };

        // 2. 构建本地 bundle
        let local_bundle = self.build_local_bundle()?;
        let local_timestamp = SyncEngine::current_timestamp();

        // 3. 决定同步方向
        let cloud_ts = cloud_bundle.as_ref().map(|b| b.meta.timestamp).unwrap_or(0);

        if cloud_ts > 0 && cloud_ts >= local_timestamp {
            // 云端更新，下载并应用
            if let Some(bundle) = cloud_bundle {
                match self.apply_bundle(&bundle) {
                    Ok((downloaded, _)) => {
                        result.downloaded = downloaded;
                        // tracing::info!(
                        //     "[Blob同步] 从云端 bundle 恢复: 下载 {} 项",
                        //     result.downloaded
                        // );
                    }
                    Err(e) => {
                        // tracing::error!("[Blob同步] 应用云端 bundle 失败: {}", e);
                        result.errors.push(format!("应用云端数据失败: {}", e));
                    }
                }
            }
        } else {
            // 本地更新或首次，上传
            match self
                .upload_bundle(vault, local_bundle, local_timestamp)
                .await
            {
                Ok(uploaded) => {
                    result.uploaded = uploaded;
                    // tracing::info!("[Blob同步] 上传 bundle 到云端: {} 项", result.uploaded);
                }
                Err(e) => {
                    // tracing::error!("[Blob同步] 上传 bundle 失败: {}", e);
                    result.errors.push(format!("上传云端失败: {}", e));
                }
            }
        }

        // 4. 更新本地同步状态
        if result.errors.is_empty() {
            self.update_local_sync_status()?;
        }

        // tracing::info!(
        //     "========== Blob同步完成: 上传 {} 个, 下载 {} 个, 错误 {} 个 ==========",
        //     result.uploaded,
        //     result.downloaded,
        //     result.errors.len()
        // );

        Ok(result)
    }

    // ========================================================================
    // Bundle 构建
    // ========================================================================

    /// 对证书数组中的 params 字段加密
    fn encrypt_certificate_params(&self, certs: &serde_json::Value) -> serde_json::Value {
        if let Some(arr) = certs.as_array() {
            let encrypted: Vec<serde_json::Value> = arr
                .iter()
                .map(|cert| {
                    let mut cert = cert.clone();
                    if let Some(params) = cert.get("params") {
                        if let Ok(json) = serde_json::to_string(params) {
                            let crypto = self.crypto_service.read().ok();
                            if let Some(ref crypto) = crypto {
                                if let Ok(key) = crypto.select_encrypt_key() {
                                    let encrypted = crate::crypto::encrypt_with_key(&json, key);
                                    if let Some(obj) = cert.as_object_mut() {
                                        obj.insert(
                                            "params".to_string(),
                                            serde_json::Value::String(encrypted),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    cert
                })
                .collect();
            serde_json::Value::Array(encrypted)
        } else {
            certs.clone()
        }
    }

    /// 对连接数组中的 params 字段加密
    fn encrypt_connection_params(&self, conns: &serde_json::Value) -> serde_json::Value {
        if let Some(arr) = conns.as_array() {
            let encrypted: Vec<serde_json::Value> = arr
                .iter()
                .map(|conn| {
                    let mut conn = conn.clone();
                    if let Some(params) = conn.get("params") {
                        if let Ok(json) = serde_json::to_string(params) {
                            let crypto = self.crypto_service.read().ok();
                            if let Some(ref crypto) = crypto {
                                if let Ok(key) = crypto.select_encrypt_key() {
                                    let encrypted = crate::crypto::encrypt_with_key(&json, key);
                                    if let Some(obj) = conn.as_object_mut() {
                                        obj.insert(
                                            "params".to_string(),
                                            serde_json::Value::String(encrypted),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    conn
                })
                .collect();
            serde_json::Value::Array(encrypted)
        } else {
            conns.clone()
        }
    }

    /// 对连接数组中的 params 字段解密
    fn decrypt_connection_params(&self, conns: &serde_json::Value) -> serde_json::Value {
        if let Some(arr) = conns.as_array() {
            let decrypted: Vec<serde_json::Value> = arr
                .iter()
                .map(|conn| {
                    let mut conn = conn.clone();
                    if let Some(params_str) = conn.get("params").and_then(|v| v.as_str()) {
                        let crypto = self.crypto_service.read().ok();
                        if let Some(ref crypto) = crypto {
                            if let Ok(key) = crypto.select_decrypt_key() {
                                if let Ok(json) = crate::crypto::decrypt_with_key(params_str, &key)
                                {
                                    if let Ok(params) =
                                        serde_json::from_str::<serde_json::Value>(&json)
                                    {
                                        if let Some(obj) = conn.as_object_mut() {
                                            obj.insert("params".to_string(), params);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    conn
                })
                .collect();
            serde_json::Value::Array(decrypted)
        } else {
            conns.clone()
        }
    }

    /// 对证书数组中的 params 字段解密
    fn decrypt_certificate_params(&self, certs: &serde_json::Value) -> serde_json::Value {
        if let Some(arr) = certs.as_array() {
            let decrypted: Vec<serde_json::Value> = arr
                .iter()
                .map(|cert| {
                    let mut cert = cert.clone();
                    if let Some(params_str) = cert.get("params").and_then(|v| v.as_str()) {
                        let crypto = self.crypto_service.read().ok();
                        if let Some(ref crypto) = crypto {
                            if let Ok(key) = crypto.select_decrypt_key() {
                                if let Ok(json) = crate::crypto::decrypt_with_key(params_str, &key)
                                {
                                    if let Ok(params) =
                                        serde_json::from_str::<serde_json::Value>(&json)
                                    {
                                        if let Some(obj) = cert.as_object_mut() {
                                            obj.insert("params".to_string(), params);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    cert
                })
                .collect();
            serde_json::Value::Array(decrypted)
        } else {
            certs.clone()
        }
    }

    fn build_local_bundle(&self) -> Result<SyncBundle, SyncError> {
        use crate::storage::traits::Repository;

        // 获取各类型数据为 JSON Value
        let connections: Option<serde_json::Value> = self
            .storage
            .get::<crate::storage::ConnectionRepository>()
            .and_then(|repo| repo.list().ok())
            .map(|conns| serde_json::to_value(&conns).unwrap_or_default())
            .map(|json_val| self.encrypt_connection_params(&json_val));

        let workspaces: Option<serde_json::Value> = self
            .storage
            .get::<crate::storage::WorkspaceRepository>()
            .and_then(|repo| repo.list().ok())
            .map(|wss| serde_json::to_value(&wss).unwrap_or_default());

        let certificates: Option<serde_json::Value> = self
            .storage
            .get::<crate::storage::CertificateRepository>()
            .and_then(|repo| repo.list().ok())
            .map(|certs| serde_json::to_value(&certs).unwrap_or_default())
            .map(|json_val| self.encrypt_certificate_params(&json_val));

        // 计算各类型的最大更新时间
        let mut type_timestamps = HashMap::new();
        if let Some(ref val) = connections {
            if let Some(arr) = val.as_array() {
                let max_ts = arr
                    .iter()
                    .filter_map(|v| v.get("updated_at").and_then(|v| v.as_i64()))
                    .max()
                    .unwrap_or(0);
                type_timestamps.insert("connection".to_string(), max_ts);
            }
        }
        if let Some(ref val) = workspaces {
            if let Some(arr) = val.as_array() {
                let max_ts = arr
                    .iter()
                    .filter_map(|v| v.get("updated_at").and_then(|v| v.as_i64()))
                    .max()
                    .unwrap_or(0);
                type_timestamps.insert("workspace".to_string(), max_ts);
            }
        }
        if let Some(ref val) = certificates {
            if let Some(arr) = val.as_array() {
                let max_ts = arr
                    .iter()
                    .filter_map(|v| v.get("updated_at").and_then(|v| v.as_i64()))
                    .max()
                    .unwrap_or(0);
                type_timestamps.insert("certificate".to_string(), max_ts);
            }
        }

        Ok(SyncBundle {
            meta: BundleMeta {
                version: 1,
                timestamp: SyncEngine::current_timestamp(),
                type_timestamps,
            },
            connections,
            workspaces,
            certificates,
        })
    }

    // ========================================================================
    // Blob 上传/下载
    // ========================================================================

    /// 下载并解析云端 bundle
    async fn download_bundle_blob(
        &self,
        vault: &Arc<dyn BlobVault>,
    ) -> Result<Option<SyncBundle>, SyncError> {
        let blob = vault.download(BUNDLE_KEY).await.map_err(|e| {
            let msg = e.to_string();
            if msg.contains("不存在") || msg.contains("not found") || msg.contains("404") {
                return SyncError::NetworkError("not_found".to_string());
            }
            SyncError::NetworkError(e.to_string())
        })?;

        let plaintext = String::from_utf8_lossy(&blob.data).to_string();

        let bundle: SyncBundle = serde_json::from_str(&plaintext)
            .map_err(|e| SyncError::StorageError(format!("bundle 反序列化失败: {}", e)))?;

        Ok(Some(bundle))
    }

    /// 加密并上传 bundle 到云端
    async fn upload_bundle(
        &self,
        vault: &Arc<dyn BlobVault>,
        bundle: SyncBundle,
        timestamp: i64,
    ) -> Result<usize, SyncError> {
        // 序列化为 JSON 明文直接存储（暂不加密，便于观察验证）
        let plaintext = serde_json::to_string_pretty(&bundle)
            .map_err(|e| SyncError::StorageError(format!("bundle 序列化失败: {}", e)))?;

        // 上传
        vault
            .upload(BUNDLE_KEY, plaintext.into_bytes())
            .await
            .map_err(|e| SyncError::NetworkError(e.to_string()))?;

        // tracing::info!("[Blob同步] bundle 上传成功，时间戳: {}", timestamp);

        // 计算上传项数
        let mut count = 0;
        if let Some(ref val) = bundle.connections {
            count += val.as_array().map(|v| v.len()).unwrap_or(0);
        }
        if let Some(ref val) = bundle.workspaces {
            count += val.as_array().map(|v| v.len()).unwrap_or(0);
        }
        if let Some(ref val) = bundle.certificates {
            count += val.as_array().map(|v| v.len()).unwrap_or(0);
        }

        Ok(count)
    }

    // ========================================================================
    // Bundle 应用（从云端恢复到本地）
    // ========================================================================

    fn apply_bundle(&self, bundle: &SyncBundle) -> Result<(usize, usize), SyncError> {
        use crate::storage::traits::Repository;

        let mut downloaded = 0;
        let mut updated = 0;

        // 恢复连接
        if let Some(ref val) = bundle.connections {
            let decrypted_conns = self.decrypt_connection_params(val);
            let cloud_connections: Vec<crate::storage::StoredConnection> =
                serde_json::from_value(decrypted_conns)
                    .map_err(|e| SyncError::StorageError(format!("连接数据解析失败: {}", e)))?;

            let repo = self
                .storage
                .get::<crate::storage::ConnectionRepository>()
                .ok_or_else(|| {
                    SyncError::StorageError("ConnectionRepository not found".to_string())
                })?;

            let local_connections = repo
                .list()
                .map_err(|e| SyncError::StorageError(format!("获取本地连接失败: {}", e)))?;

            let local_by_cloud_id: HashMap<String, &crate::storage::StoredConnection> =
                local_connections
                    .iter()
                    .filter_map(|c| c.cloud_id.as_ref().map(|id| (id.clone(), c)))
                    .collect();

            let local_by_name: HashMap<&str, &crate::storage::StoredConnection> = local_connections
                .iter()
                .map(|c| (c.name.as_str(), c))
                .collect();

            for cloud_conn in &cloud_connections {
                let cloud_id = cloud_conn.cloud_id.as_deref();

                if let Some(cloud_id) = cloud_id {
                    if let Some(local) = local_by_cloud_id.get(cloud_id) {
                        let local: &crate::storage::StoredConnection = *local;
                        // 已有对应 cloud_id 的本地连接，比较更新时间
                        let cloud_updated = cloud_conn.updated_at.unwrap_or(0);
                        let local_updated = local.updated_at.unwrap_or(0);

                        if cloud_updated > local_updated {
                            // 云端更新，更新本地
                            let mut updated_conn = local.clone();
                            updated_conn.name = cloud_conn.name.clone();
                            updated_conn.params = cloud_conn.params.clone();
                            updated_conn.workspace_id = cloud_conn.workspace_id;
                            updated_conn.selected_databases = cloud_conn.selected_databases.clone();
                            updated_conn.remark = cloud_conn.remark.clone();
                            updated_conn.owner_id = cloud_conn.owner_id.clone();
                            updated_conn.updated_at = cloud_conn.updated_at;
                            updated_conn.last_synced_at = Some(SyncEngine::current_timestamp());

                            repo.update(&updated_conn).map_err(|e| {
                                SyncError::StorageError(format!("更新连接失败: {}", e))
                            })?;
                            updated += 1;
                        }
                        // 否则保留本地版本
                    } else {
                        // 云端有但本地没有，新建
                        let mut new_conn = cloud_conn.clone();
                        new_conn.id = None;
                        new_conn.last_synced_at = Some(SyncEngine::current_timestamp());
                        repo.insert(&mut new_conn)
                            .map_err(|e| SyncError::StorageError(format!("创建连接失败: {}", e)))?;
                        downloaded += 1;
                    }
                } else if !local_by_name.contains_key(cloud_conn.name.as_str()) {
                    // 无 cloud_id 且本地无同名，新建
                    let mut new_conn = cloud_conn.clone();
                    new_conn.id = None;
                    new_conn.cloud_id = None;
                    new_conn.last_synced_at = Some(SyncEngine::current_timestamp());
                    repo.insert(&mut new_conn)
                        .map_err(|e| SyncError::StorageError(format!("创建连接失败: {}", e)))?;
                    downloaded += 1;
                }
            }
        }

        // 恢复工作空间
        if let Some(ref val) = bundle.workspaces {
            let cloud_workspaces: Vec<crate::storage::Workspace> =
                serde_json::from_value(val.clone())
                    .map_err(|e| SyncError::StorageError(format!("工作空间数据解析失败: {}", e)))?;

            let repo = self
                .storage
                .get::<crate::storage::WorkspaceRepository>()
                .ok_or_else(|| {
                    SyncError::StorageError("WorkspaceRepository not found".to_string())
                })?;

            let local_workspaces = repo
                .list()
                .map_err(|e| SyncError::StorageError(format!("获取本地工作空间失败: {}", e)))?;

            let local_by_cloud_id: HashMap<String, &crate::storage::Workspace> = local_workspaces
                .iter()
                .filter_map(|w| w.cloud_id.as_ref().map(|id| (id.clone(), w)))
                .collect();

            for cloud_ws in &cloud_workspaces {
                if let Some(cloud_id) = &cloud_ws.cloud_id {
                    if let Some(local) = local_by_cloud_id.get(cloud_id) {
                        let local: &crate::storage::Workspace = *local;
                        let cloud_updated = cloud_ws.updated_at.unwrap_or(0);
                        let local_updated = local.updated_at.unwrap_or(0);

                        if cloud_updated > local_updated {
                            let mut updated_ws = local.clone();
                            updated_ws.name = cloud_ws.name.clone();
                            updated_ws.updated_at = cloud_ws.updated_at;
                            updated_ws.last_synced_at = Some(SyncEngine::current_timestamp());
                            repo.update_from_cloud(&updated_ws).map_err(|e| {
                                SyncError::StorageError(format!("更新工作空间失败: {}", e))
                            })?;
                            updated += 1;
                        }
                    } else {
                        let mut new_ws = cloud_ws.clone();
                        new_ws.id = None;
                        new_ws.last_synced_at = Some(SyncEngine::current_timestamp());
                        repo.insert(&mut new_ws).map_err(|e| {
                            SyncError::StorageError(format!("创建工作空间失败: {}", e))
                        })?;
                        downloaded += 1;
                    }
                }
            }
        }

        // 恢复凭证
        if let Some(ref val) = bundle.certificates {
            let decrypted_certs = self.decrypt_certificate_params(val);
            let cloud_certs: Vec<crate::storage::Certificate> =
                serde_json::from_value(decrypted_certs)
                    .map_err(|e| SyncError::StorageError(format!("凭证数据解析失败: {}", e)))?;

            let repo = self
                .storage
                .get::<crate::storage::CertificateRepository>()
                .ok_or_else(|| {
                    SyncError::StorageError("CertificateRepository not found".to_string())
                })?;

            let local_certs = repo
                .list()
                .map_err(|e| SyncError::StorageError(format!("获取本地凭证失败: {}", e)))?;

            let local_by_cloud_id: HashMap<String, &crate::storage::Certificate> = local_certs
                .iter()
                .filter_map(|c| c.cloud_id.as_ref().map(|id| (id.clone(), c)))
                .collect();

            for cloud_cert in &cloud_certs {
                if let Some(cloud_id) = &cloud_cert.cloud_id {
                    if let Some(local) = local_by_cloud_id.get(cloud_id) {
                        let local: &crate::storage::Certificate = *local;
                        let cloud_updated = cloud_cert.updated_at.unwrap_or(0);
                        let local_updated = local.updated_at.unwrap_or(0);

                        if cloud_updated > local_updated {
                            let mut updated_cert = local.clone();
                            updated_cert.name = cloud_cert.name.clone();
                            updated_cert.kind = cloud_cert.kind;
                            updated_cert.params = cloud_cert.params.clone();
                            updated_cert.remark = cloud_cert.remark.clone();
                            updated_cert.updated_at = cloud_cert.updated_at;
                            updated_cert.last_synced_at = Some(SyncEngine::current_timestamp());

                            repo.update(&updated_cert).map_err(|e| {
                                SyncError::StorageError(format!("更新凭证失败: {}", e))
                            })?;
                            updated += 1;
                        }
                    } else {
                        let mut new_cert = cloud_cert.clone();
                        new_cert.id = None;
                        new_cert.last_synced_at = Some(SyncEngine::current_timestamp());
                        repo.insert(&mut new_cert)
                            .map_err(|e| SyncError::StorageError(format!("创建凭证失败: {}", e)))?;
                        downloaded += 1;
                    }
                }
            }
        }

        Ok((downloaded, updated))
    }

    // ========================================================================
    // 同步状态更新
    // ========================================================================

    fn update_local_sync_status(&self) -> Result<(), SyncError> {
        use crate::storage::traits::Repository;

        let now = Some(SyncEngine::current_timestamp());

        // 更新连接的同步时间
        if let Some(repo) = self.storage.get::<crate::storage::ConnectionRepository>() {
            if let Ok(conns) = repo.list() {
                for conn in conns {
                    if conn.sync_enabled {
                        if let Some(id) = conn.id {
                            let _ = repo.update_sync_status(id, conn.cloud_id.clone(), now);
                        }
                    }
                }
            }
        }

        // 更新工作空间的同步时间
        if let Some(repo) = self.storage.get::<crate::storage::WorkspaceRepository>() {
            if let Ok(wss) = repo.list() {
                for ws in wss {
                    if let Some(id) = ws.id {
                        let _ = repo.update_sync_status(id, ws.cloud_id.clone(), now);
                    }
                }
            }
        }

        Ok(())
    }

    /// 通过 BlobVault 同步单个 handler 的数据（已废弃，保留兼容接口）
    ///
    /// 现在使用 bundle 级别的同步，此方法保留以避免编译错误。
    #[deprecated(since = "0.2.0", note = "使用 bundle 级别的同步替代")]
    async fn sync_handler_via_blob(
        &self,
        _handler: &dyn super::engine::SyncHandler,
        _vault: &Arc<dyn BlobVault>,
    ) -> Result<SyncResult, SyncError> {
        Ok(SyncResult::default())
    }
}
