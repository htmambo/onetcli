//! 云同步服务

use crate::cloud_sync::models::*;
use crate::cloud_sync::queue::OperationQueue;
use crate::crypto::{self, CryptoError};
use crate::storage::{Certificate, CertificateKind, ConnectionType, StoredConnection};
use serde_json::Value;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

/// 云同步错误类型
#[derive(Debug)]
pub enum SyncError {
    /// 未解锁（未输入主密钥）
    NotUnlocked,
    /// 主密钥错误
    InvalidMasterKey,
    /// 云端主密钥与当前本地主密钥不一致
    CloudMasterKeyMismatch(String),
    /// 密钥版本不匹配
    KeyVersionMismatch,
    /// 网络错误
    NetworkError(String),
    /// 加解密错误
    CryptoError(CryptoError),
    /// 数据格式错误
    DataFormatError(String),
    /// 存储错误
    StorageError(String),
    /// 未登录
    NotLoggedIn,
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::NotUnlocked => write!(f, "请先输入主密钥解锁"),
            SyncError::InvalidMasterKey => write!(f, "主密钥错误"),
            SyncError::CloudMasterKeyMismatch(message) => write!(f, "{}", message),
            SyncError::KeyVersionMismatch => write!(f, "密钥版本不匹配，请重新同步"),
            SyncError::NetworkError(e) => write!(f, "网络错误: {}", e),
            SyncError::CryptoError(e) => write!(f, "加解密错误: {}", e),
            SyncError::DataFormatError(e) => write!(f, "数据格式错误: {}", e),
            SyncError::StorageError(e) => write!(f, "存储错误: {}", e),
            SyncError::NotLoggedIn => write!(f, "请先登录云端账户"),
        }
    }
}

impl std::error::Error for SyncError {}

impl From<CryptoError> for SyncError {
    fn from(e: CryptoError) -> Self {
        SyncError::CryptoError(e)
    }
}

/// 获取当前时间戳（毫秒）
fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 云同步服务
pub struct CloudSyncService {
    /// 主密钥（解锁后存储）
    master_key: Option<Zeroizing<String>>,
    /// 当前密钥版本
    key_version: u32,
    /// 是否已登录
    logged_in: bool,
    /// 用户 ID
    user_id: Option<String>,
    /// 同步操作队列（按类型分组，上限 64 个队列）
    operation_queues: HashMap<String, OperationQueue>,
}

const MAX_OPERATION_QUEUES: usize = 64;

impl CloudSyncService {
    /// 创建新的云同步服务
    pub fn new() -> Self {
        Self {
            master_key: None,
            key_version: 0,
            logged_in: false,
            user_id: None,
            operation_queues: HashMap::new(),
        }
    }

    /// 检查是否已解锁
    pub(crate) fn is_unlocked(&self) -> bool {
        self.master_key.is_some()
    }

    /// 获取当前密钥版本
    pub(crate) fn key_version(&self) -> u32 {
        self.key_version
    }

    /// 设置登录状态（由外部认证流程调用）
    pub fn set_logged_in(&mut self, user_id: String) {
        self.logged_in = true;
        self.user_id = Some(user_id);
    }

    /// 登出
    pub fn logout(&mut self) {
        self.logged_in = false;
        self.user_id = None;
        self.master_key = None;
        self.key_version = 0;
        self.operation_queues.clear();
    }

    pub(crate) fn take_operation_queue(&mut self, key: &str) -> OperationQueue {
        self.operation_queues.remove(key).unwrap_or_default()
    }

    pub(crate) fn store_operation_queue(&mut self, key: &str, queue: OperationQueue) {
        if queue.is_empty() {
            self.operation_queues.remove(key);
        } else {
            if self.operation_queues.len() >= MAX_OPERATION_QUEUES
                && !self.operation_queues.contains_key(key)
            {
                tracing::warn!(
                    "Operation queue limit ({MAX_OPERATION_QUEUES}) reached, dropping oldest"
                );
                if let Some(oldest_key) = self.operation_queues.keys().next().cloned() {
                    self.operation_queues.remove(&oldest_key);
                }
            }
            self.operation_queues.insert(key.to_string(), queue);
        }
    }

    /// 直接设置主密钥（不验证）
    ///
    /// 用于从本地 crypto 模块同步密钥状态，跳过云端验证。
    /// 调用者需确保密钥已通过本地验证。
    ///
    /// S2 后接受 `Zeroizing<String>`：调用方传 `Zeroizing::new(s)`，内部不再长期
    /// 持有普通 `String` 副本，drop 时 zeroize 缓冲区。
    pub(crate) fn set_master_key_directly(&mut self, master_key: Zeroizing<String>) {
        self.master_key = Some(master_key);
        // key_version 保持默认或之前的值，实际同步时会从云端更新
    }

    /// 解锁同步服务（验证主密钥）
    ///
    /// 需要先从云端获取 key_verification 数据进行验证
    pub(crate) fn unlock(
        &mut self,
        master_key: &str,
        cloud_config: &CloudUserConfig,
    ) -> Result<(), SyncError> {
        // 验证主密钥
        if !crypto::verify_master_key(master_key, &cloud_config.key_verification) {
            return Err(SyncError::InvalidMasterKey);
        }

        self.master_key = Some(Zeroizing::new(master_key.to_string()));
        self.key_version = cloud_config.key_version;
        Ok(())
    }

    /// 首次设置主密钥
    ///
    /// 返回需要上传到云端的配置数据
    pub(crate) fn setup_master_key(
        &mut self,
        master_key: &str,
    ) -> Result<CloudUserConfig, SyncError> {
        if !self.logged_in {
            return Err(SyncError::NotLoggedIn);
        }

        let verification = crypto::generate_key_verification_v1(master_key);
        let user_id = self.user_id.clone().unwrap_or_default();

        let config = CloudUserConfig {
            user_id,
            key_verification: verification,
            key_version: 1,
            updated_at: current_timestamp(),
        };

        self.master_key = Some(Zeroizing::new(master_key.to_string()));
        self.key_version = 1;

        Ok(config)
    }

    // ========================================================================
    // 统一 blob 加密/解密（新版 sync_data）
    // ========================================================================

    /// 选择加密密钥
    pub(crate) fn select_encrypt_key(&self) -> Result<&str, SyncError> {
        self.master_key
            .as_deref()
            .map(|s| s.as_str())
            .ok_or(SyncError::NotUnlocked)
    }

    /// 选择解密密钥（S2：返回 `Zeroizing<String>`，调用方短期持有，drop 时清零）
    pub(crate) fn select_decrypt_key(&self) -> Result<Zeroizing<String>, SyncError> {
        self.master_key.clone().ok_or(SyncError::NotUnlocked)
    }

    /// 加密整体明文 JSON 为 blob
    pub(crate) fn encrypt_blob(&self, plaintext: &str) -> Result<String, SyncError> {
        let key = self.select_encrypt_key()?;
        Ok(crypto::encrypt_with_key(plaintext, key))
    }

    /// 解密整体 blob 为明文 JSON
    pub(crate) fn decrypt_blob(&self, encrypted: &str) -> Result<String, SyncError> {
        let key = self.select_encrypt_key()?;
        crypto::decrypt_with_key(encrypted, key).map_err(SyncError::CryptoError)
    }

    /// 计算明文数据的 SHA-256 校验和
    pub(crate) fn calculate_blob_checksum(plaintext: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(plaintext.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// 准备上传连接到 sync_data（整体 blob 加密）
    pub(crate) fn prepare_sync_data_upload(
        &self,
        conn: &StoredConnection,
        workspace_cloud_id: Option<String>,
    ) -> Result<CloudSyncData, SyncError> {
        let plain_data = ConnectionPlainData {
            name: conn.name.clone(),
            connection_type: conn.connection_type.to_string(),
            sort_order: conn.sort_order,
            workspace_cloud_id,
            selected_databases: conn.selected_databases.clone(),
            remark: conn.remark.clone(),
            params: serde_json::from_str(&conn.params)
                .unwrap_or(Value::Object(serde_json::Map::new())),
            owner_id: conn.owner_id.clone(),
        };

        let plaintext = serde_json::to_string(&plain_data)
            .map_err(|e| SyncError::DataFormatError(e.to_string()))?;

        let checksum = Self::calculate_blob_checksum(&plaintext);
        let encrypted_data = self.encrypt_blob(&plaintext)?;
        let key_version = self.key_version();

        Ok(CloudSyncData {
            id: uuid::Uuid::new_v4().to_string(),
            owner_id: self.user_id.clone().unwrap_or_default(),
            data_type: data_type::CONNECTION.to_string(),
            name: conn.name.clone(),
            encrypted_data,
            key_version,
            checksum,
            version: 1,
            updated_at: current_timestamp(),
            deleted_at: None,
        })
    }

    /// 准备上传工作空间到 sync_data（整体 blob 加密）
    pub(crate) fn prepare_workspace_sync_data_upload(
        &self,
        ws: &crate::storage::Workspace,
    ) -> Result<CloudSyncData, SyncError> {
        let plain_data = WorkspacePlainData {
            name: ws.name.clone(),
            sort_order: ws.sort_order,
            color: ws.color.clone(),
            icon: ws.icon.clone(),
        };

        let plaintext = serde_json::to_string(&plain_data)
            .map_err(|e| SyncError::DataFormatError(e.to_string()))?;

        let checksum = Self::calculate_blob_checksum(&plaintext);
        let encrypted_data = self.encrypt_blob(&plaintext)?;
        let key_version = self.key_version();

        Ok(CloudSyncData {
            id: uuid::Uuid::new_v4().to_string(),
            owner_id: self.user_id.clone().unwrap_or_default(),
            data_type: data_type::WORKSPACE.to_string(),
            name: ws.name.clone(),
            encrypted_data,
            key_version,
            checksum,
            version: 1,
            updated_at: current_timestamp(),
            deleted_at: None,
        })
    }

    /// 准备上传证书到 sync_data（整体 blob 加密）
    pub(crate) fn prepare_certificate_sync_data_upload(
        &self,
        certificate: &Certificate,
    ) -> Result<CloudSyncData, SyncError> {
        // 对 params 进行加密 + base64 编码
        let params_json = serde_json::to_string(&certificate.params)
            .map_err(|e| SyncError::DataFormatError(e.to_string()))?;
        let key = self.select_encrypt_key()?;
        let encrypted_params = crypto::encrypt_with_key(&params_json, key);

        let plain_data = CertificatePlainData {
            name: certificate.name.clone(),
            kind: certificate.kind.to_string(),
            params: encrypted_params,
            remark: certificate.remark.clone(),
            owner_id: certificate.owner_id.clone(),
        };

        let plaintext = serde_json::to_string(&plain_data)
            .map_err(|e| SyncError::DataFormatError(e.to_string()))?;

        let checksum = Self::calculate_blob_checksum(&plaintext);
        let encrypted_data = self.encrypt_blob(&plaintext)?;
        let key_version = self.key_version();

        Ok(CloudSyncData {
            id: uuid::Uuid::new_v4().to_string(),
            owner_id: self.user_id.clone().unwrap_or_default(),
            data_type: data_type::CERTIFICATE.to_string(),
            name: certificate.name.clone(),
            encrypted_data,
            key_version,
            checksum,
            version: 1,
            updated_at: current_timestamp(),
            deleted_at: None,
        })
    }

    /// 解密 sync_data 中的连接数据
    pub(crate) fn decrypt_sync_data_connection(
        &self,
        cloud_data: &CloudSyncData,
    ) -> Result<StoredConnection, SyncError> {
        let plaintext = self.decrypt_blob(&cloud_data.encrypted_data)?;
        let plain_data: ConnectionPlainData = serde_json::from_str(&plaintext)
            .map_err(|e| SyncError::DataFormatError(e.to_string()))?;

        let connection_type = ConnectionType::from_str(&plain_data.connection_type);
        let params = serde_json::to_string(&plain_data.params).unwrap_or_else(|_| "{}".to_string());

        Ok(StoredConnection {
            id: None,
            name: plain_data.name,
            connection_type,
            sort_order: plain_data.sort_order,
            workspace_id: None, // 由调用者根据 workspace_cloud_id 解析
            params,
            selected_databases: plain_data.selected_databases,
            remark: plain_data.remark,
            sync_enabled: true,
            cloud_id: Some(cloud_data.id.clone()),
            last_synced_at: Some(cloud_data.updated_at / 1000),
            created_at: None,
            updated_at: Some(cloud_data.updated_at / 1000),
            owner_id: plain_data.owner_id,
        })
    }

    /// 解密 sync_data 中的工作空间数据
    pub(crate) fn decrypt_sync_data_workspace(
        &self,
        cloud_data: &CloudSyncData,
    ) -> Result<crate::storage::Workspace, SyncError> {
        let plaintext = self.decrypt_blob(&cloud_data.encrypted_data)?;
        let plain_data: WorkspacePlainData = serde_json::from_str(&plaintext)
            .map_err(|e| SyncError::DataFormatError(e.to_string()))?;

        Ok(crate::storage::Workspace {
            id: None,
            name: plain_data.name,
            sort_order: plain_data.sort_order,
            color: plain_data.color,
            icon: plain_data.icon,
            created_at: None,
            updated_at: Some(cloud_data.updated_at / 1000),
            cloud_id: Some(cloud_data.id.clone()),
            last_synced_at: Some(cloud_data.updated_at / 1000),
        })
    }

    /// 解密 sync_data 中的证书数据
    pub(crate) fn decrypt_sync_data_certificate(
        &self,
        cloud_data: &CloudSyncData,
    ) -> Result<Certificate, SyncError> {
        let plaintext = self.decrypt_blob(&cloud_data.encrypted_data)?;
        let plain_data: CertificatePlainData = serde_json::from_str(&plaintext)
            .map_err(|e| SyncError::DataFormatError(e.to_string()))?;

        // 解密 params
        let key = self.select_decrypt_key()?;
        let params_json = crypto::decrypt_with_key(&plain_data.params, key.as_ref())
            .map_err(|e| SyncError::DataFormatError(e.to_string()))?;
        let params: serde_json::Value = serde_json::from_str(&params_json)
            .map_err(|e| SyncError::DataFormatError(e.to_string()))?;

        Ok(Certificate {
            id: None,
            name: plain_data.name,
            kind: CertificateKind::from_str(&plain_data.kind),
            params,
            remark: plain_data.remark,
            sync_enabled: true,
            cloud_id: Some(cloud_data.id.clone()),
            last_synced_at: Some(cloud_data.updated_at / 1000),
            created_at: None,
            updated_at: Some(cloud_data.updated_at / 1000),
            owner_id: plain_data.owner_id,
        })
    }

    /// 准备上传 LLM 提供商配置到 sync_data（整体 blob 加密）
    pub(crate) fn prepare_llm_provider_sync_data_upload(
        &self,
        item: &crate::llm::types::ProviderConfig,
    ) -> Result<CloudSyncData, SyncError> {
        let plain_data = LlmProviderPlainData {
            name: item.name.clone(),
            provider_type: item.provider_type.as_str().to_string(),
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

        let plaintext = serde_json::to_string(&plain_data)
            .map_err(|e| SyncError::DataFormatError(e.to_string()))?;

        let checksum = Self::calculate_blob_checksum(&plaintext);
        let encrypted_data = self.encrypt_blob(&plaintext)?;
        let key_version = self.key_version();

        Ok(CloudSyncData {
            id: item
                .cloud_id
                .clone()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            owner_id: self.user_id.clone().unwrap_or_default(),
            data_type: data_type::LLM_PROVIDER.to_string(),
            name: item.name.clone(),
            encrypted_data,
            key_version,
            checksum,
            version: 1,
            updated_at: current_timestamp(),
            deleted_at: None,
        })
    }

    /// 解密 sync_data 中的 LLM 提供商配置数据
    pub(crate) fn decrypt_sync_data_llm_provider(
        &self,
        cloud_data: &CloudSyncData,
    ) -> Result<crate::llm::types::ProviderConfig, SyncError> {
        let plaintext = self.decrypt_blob(&cloud_data.encrypted_data)?;
        let plain_data: LlmProviderPlainData = serde_json::from_str(&plaintext)
            .map_err(|e| SyncError::DataFormatError(e.to_string()))?;

        let provider_type = crate::llm::types::ProviderType::from_str(&plain_data.provider_type)
            .unwrap_or(crate::llm::types::ProviderType::OpenAI);

        let models = if plain_data.models.is_empty() {
            vec![plain_data.model.clone()]
        } else {
            plain_data.models
        };

        Ok(crate::llm::types::ProviderConfig {
            id: 0, // 本地插入时重新分配
            name: plain_data.name,
            provider_type,
            api_key: plain_data.api_key,
            api_base: plain_data.api_base,
            api_version: plain_data.api_version,
            model: plain_data.model,
            models,
            max_tokens: plain_data.max_tokens,
            temperature: plain_data.temperature,
            thinking_budget: plain_data.thinking_budget,
            enabled: plain_data.enabled,
            is_default: plain_data.is_default,
            cloud_id: Some(cloud_data.id.clone()),
            last_synced_at: Some(cloud_data.updated_at / 1000),
            sync_enabled: true,
            created_at: cloud_data.updated_at / 1000,
            updated_at: cloud_data.updated_at / 1000,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blob_encrypt_decrypt() {
        let mut service = CloudSyncService::new();
        service.set_master_key_directly(Zeroizing::new("test_blob_key".to_string()));

        let plaintext = r#"{"name":"test","params":{"host":"localhost","password":"secret"}}"#;
        let encrypted = service.encrypt_blob(plaintext).unwrap();
        assert!(encrypted.starts_with("ENC:"));

        let decrypted = service.decrypt_blob(&encrypted).unwrap();
        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_blob_checksum() {
        let data = r#"{"name":"test","host":"localhost"}"#;
        let checksum1 = CloudSyncService::calculate_blob_checksum(data);
        let checksum2 = CloudSyncService::calculate_blob_checksum(data);
        assert_eq!(checksum1, checksum2);

        let different_data = r#"{"name":"test","host":"127.0.0.1"}"#;
        let checksum3 = CloudSyncService::calculate_blob_checksum(different_data);
        assert_ne!(checksum1, checksum3);
    }

    #[test]
    fn test_prepare_connection_sync_data_preserves_workspace_cloud_id() {
        let mut service = CloudSyncService::new();
        service.set_master_key_directly(Zeroizing::new("test_workspace_key".to_string()));

        let connection = StoredConnection::new_redis(
            "测试连接".to_string(),
            crate::storage::RedisParams {
                host: "127.0.0.1".to_string(),
                port: 6379,
                password: Some("secret".to_string()),
                username: Some("tester".to_string()),
                credential_ref: None,
                db_index: 0,
                mode: crate::storage::RedisMode::Standalone,
                use_tls: false,
                connect_timeout: None,
                sentinel: None,
                cluster: None,
                ssh_tunnel: None,
            },
            Some(7),
        );

        let cloud_data = service
            .prepare_sync_data_upload(&connection, Some("workspace-cloud-1".to_string()))
            .unwrap();

        let plaintext = service.decrypt_blob(&cloud_data.encrypted_data).unwrap();
        let plain_data: ConnectionPlainData = serde_json::from_str(&plaintext).unwrap();

        assert_eq!(
            plain_data.workspace_cloud_id.as_deref(),
            Some("workspace-cloud-1")
        );
        assert_eq!(plain_data.name, "测试连接");
    }
}
