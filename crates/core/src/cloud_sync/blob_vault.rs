//! 云端文件存储抽象（Blob Vault）
//!
//! 抽象 WebDAV、S3 等文件存储的操作接口。
//! 与 REST 风格的 `CloudApiClient`（认证、团队管理）正交。
//!
//! ## 设计原则
//!
//! - 仅定义文件级操作：上传、下载、删除、列举
//! - WebDAV/S3 实现真正的文件操作
//! - 复用现有 AES-256-GCM 加密基础设施

use crate::cloud_sync::client::CloudApiError;
use async_trait::async_trait;

/// Blob 元数据
#[derive(Debug, Clone)]
pub struct BlobMeta {
    /// 文件名（含路径
    pub key: String,
    /// 文件大小（字节）
    pub size: u64,
    /// 最后修改时间戳（毫秒）
    pub updated_at: i64,
}

/// Blob 内容
#[derive(Debug, Clone)]
pub struct Blob {
    pub key: String,
    pub data: Vec<u8>,
    pub updated_at: i64,
}

/// 云端文件存储 trait
///
/// 抽象 WebDAV、S3 等文件存储的操作。
/// 与 REST 风格的 `CloudApiClient`（账号、团队、配置管理）正交。
#[async_trait]
pub trait BlobVault: Send + Sync {
    /// 上传文件，返回文件元数据
    async fn upload(&self, key: &str, data: Vec<u8>) -> Result<BlobMeta, CloudApiError>;

    /// 下载文件
    async fn download(&self, key: &str) -> Result<Blob, CloudApiError>;

    /// 删除文件
    async fn delete(&self, key: &str) -> Result<(), CloudApiError>;

    /// 检查文件是否存在
    async fn exists(&self, key: &str) -> Result<bool, CloudApiError>;

    /// 列举目录下的所有文件
    async fn list(&self, prefix: Option<&str>) -> Result<Vec<BlobMeta>, CloudApiError>;

    /// 获取存储后端类型标识
    fn backend_type(&self) -> &'static str;
}
