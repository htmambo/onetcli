//! 云同步后端配置结构体。
//!
//! 抽取自 `setting_tab.rs`（轮 3 重构）。这些类型通过父模块以
//! `pub(crate) use` 重导出，维持 `crate::setting_tab::*Settings`
//! 引用路径不变（外部 `oauth_dialog.rs` 等无需改动）。

use one_core::cloud_sync::oauth::OAuthTokens;
use serde::{Deserialize, Serialize};

/// WebDAV 同步配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebDavSettings {
    pub endpoint: String,
    /// "basic" | "bearer"
    pub auth_type: String,
    pub username: String,
    pub password: String,
    pub bearer_token: String,
    pub vault_path: String,
}

impl Default for WebDavSettings {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            auth_type: "basic".to_string(),
            username: String::new(),
            password: String::new(),
            bearer_token: String::new(),
            vault_path: "ONetCli-vault".to_string(),
        }
    }
}

/// GitHub Gist 同步配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GistSettings {
    pub client_id: String,
    pub gist_id: Option<String>,
    pub tokens: Option<OAuthTokens>,
}

/// Google Drive 同步配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoogleDriveSettings {
    pub client_id: String,
    pub client_secret: String,
    pub folder_id: Option<String>,
    pub tokens: Option<OAuthTokens>,
}

/// OneDrive 同步配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OneDriveSettings {
    pub client_id: String,
    pub client_secret: String,
    pub root_id: Option<String>,
    pub tokens: Option<OAuthTokens>,
}
