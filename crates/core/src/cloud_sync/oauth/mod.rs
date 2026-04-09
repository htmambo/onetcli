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

impl OAuthTokens {
    /// 是否已过期
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            let now = chrono::Utc::now().timestamp();
            return now >= expires_at as i64;
        }
        false
    }
}

/// OAuth 提供商
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthProvider {
    GitHub,
    Google,
    OneDrive,
}

impl OAuthProvider {
    pub fn auth_url(&self) -> &'static str {
        match self {
            Self::GitHub => "https://github.com/login/device/code",
            Self::Google => "https://accounts.google.com/o/oauth2/v2/auth",
            Self::OneDrive => "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
        }
    }

    pub fn token_url(&self) -> &'static str {
        match self {
            Self::GitHub => "https://github.com/login/oauth/access_token",
            Self::Google => "https://oauth2.googleapis.com/token",
            Self::OneDrive => "https://login.microsoftonline.com/common/oauth2/v2.0/token",
        }
    }

    pub fn scopes(&self) -> &'static str {
        match self {
            Self::GitHub => "gist",
            Self::Google => "https://www.googleapis.com/auth/drive.appdata",
            Self::OneDrive => "Files.ReadWrite offline_access",
        }
    }
}

/// OAuth 配置
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub provider: OAuthProvider,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub redirect_uri: String,
    pub scopes: String,
}

impl OAuthConfig {
    pub fn github(client_id: String) -> Self {
        Self {
            provider: OAuthProvider::GitHub,
            client_id,
            client_secret: None,
            redirect_uri: "http://localhost".to_string(),
            scopes: OAuthProvider::GitHub.scopes().to_string(),
        }
    }

    pub fn google(client_id: String, client_secret: String) -> Self {
        Self {
            provider: OAuthProvider::Google,
            client_id,
            client_secret: Some(client_secret),
            redirect_uri: "http://localhost:8787/oauth/callback".to_string(),
            scopes: OAuthProvider::Google.scopes().to_string(),
        }
    }

    pub fn onedrive(client_id: String, client_secret: String) -> Self {
        Self {
            provider: OAuthProvider::OneDrive,
            client_id,
            client_secret: Some(client_secret),
            redirect_uri: "http://localhost:8787/oauth/callback".to_string(),
            scopes: OAuthProvider::OneDrive.scopes().to_string(),
        }
    }
}
