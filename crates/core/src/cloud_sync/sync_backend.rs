//! 同步后端抽象
//!
//! 定义同步后端的通用能力接口，屏蔽不同存储后端的差异。
//! 目前已知的后端类型：
//! - `sync_server`：自建 REST 服务，需要登录认证，支持团队、密钥管理
//! - `github_gist`：GitHub Gist 存储，OAuth 认证，通过 BlobVault 文件操作
//! - `webdav`：WebDAV 存储，用户名/密码或 Token 认证，通过 BlobVault 文件操作
//! - 后续可扩展 OneDrive、Google Drive 等

use async_trait::async_trait;

use crate::cloud_sync::engine::SyncEngine;
use crate::cloud_sync::models::SyncResult;
use crate::cloud_sync::service::SyncError;
use std::sync::Arc;

/// 同步后端能力接口
///
/// 每种后端实现此 trait，告诉引擎它需要什么前置条件以及如何执行同步。
///
/// ## 扩展新后端（如 OneDrive / Google Drive）
///
/// 1. 在 `setting_tab.rs` 中添加下拉选项
/// 2. 实现此 trait
/// 3. 在 `home_tab.rs` 的 `trigger_sync` 中创建对应后端
#[async_trait]
pub trait SyncBackend: Send + Sync {
    /// 后端类型标识
    fn backend_type(&self) -> &'static str;

    /// 是否需要用户登录（sync_server 需要，blob 后端不需要）
    fn requires_login(&self) -> bool {
        false
    }

    /// 是否需要配置服务地址（sync_server 需要 URL，blob 后端不需要）
    fn requires_url(&self) -> bool {
        false
    }

    /// 初始化加密密钥配置
    async fn init_crypto(&self, engine: &SyncEngine) -> Result<(), SyncError>;

    /// 执行完整的同步流程
    async fn execute_sync(&self, engine: &SyncEngine) -> Result<SyncResult, SyncError>;
}

// ============================================================================
// sync_server 后端
// ============================================================================

/// sync_server REST API 后端
pub struct SyncServerBackend;

#[async_trait]
impl SyncBackend for SyncServerBackend {
    fn backend_type(&self) -> &'static str {
        "sync_server"
    }

    fn requires_login(&self) -> bool {
        true
    }

    fn requires_url(&self) -> bool {
        true
    }

    async fn init_crypto(&self, engine: &SyncEngine) -> Result<(), SyncError> {
        engine.ensure_personal_key_config().await
    }

    async fn execute_sync(&self, engine: &SyncEngine) -> Result<SyncResult, SyncError> {
        engine.sync_server_flow().await
    }
}

// ============================================================================
// BlobVault 后端（GitHub Gist / WebDAV 等）
// ============================================================================

/// BlobVault 存储后端
///
/// 适用于所有基于文件存储的后端：GitHub Gist、WebDAV、OneDrive、Google Drive 等。
/// 这些后端通过 `BlobVault` trait 进行文件级操作，不需要 sync_server API。
pub struct BlobVaultBackend;

#[async_trait]
impl SyncBackend for BlobVaultBackend {
    fn backend_type(&self) -> &'static str {
        "blob_vault"
    }

    fn requires_login(&self) -> bool {
        false
    }

    fn requires_url(&self) -> bool {
        false
    }

    async fn init_crypto(&self, engine: &SyncEngine) -> Result<(), SyncError> {
        engine.ensure_unlocked()
    }

    async fn execute_sync(&self, engine: &SyncEngine) -> Result<SyncResult, SyncError> {
        engine.blob_sync().await
    }
}

/// 按 `sync_backend_type` 字符串创建后端实例
///
/// 统一入口，避免 UI 层到处 match 字符串。
pub fn create_backend(sync_backend_type: &str) -> Arc<dyn SyncBackend> {
    match sync_backend_type {
        "github_gist" | "webdav" => Arc::new(BlobVaultBackend),
        _ => Arc::new(SyncServerBackend),
    }
}
