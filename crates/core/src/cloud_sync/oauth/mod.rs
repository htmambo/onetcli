//! OAuth 认证模块
//!
//! 提供 GitHub Gist、Google Drive、Microsoft OneDrive 等 OAuth 存储后端的支持。
//!
//! ## 架构
//!
//! - `pkce.rs` — PKCE 辅助函数（code_verifier、code_challenge 生成）
//! - `callback_server.rs` — 本地 HTTP 回调服务器（用于 PKCE 授权码流程）
//! - `github_device.rs` — GitHub OAuth Device Flow
//! - `github_gist.rs` — GitHub Gist Blob Vault
//! - `google_drive.rs` — Google Drive Blob Vault
//! - `onedrive.rs` — Microsoft OneDrive Blob Vault

pub mod callback_server;
pub mod github_device;
pub mod github_gist;
pub mod google_drive;
pub mod onedrive;
pub mod pkce;

pub use github_device::{create_vault_gist, find_vault_gist};

use serde::{Deserialize, Serialize};

/// OAuth 令牌
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub expires_at: Option<i64>,
}
