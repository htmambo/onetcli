//! GitHub Gist Blob Vault
//!
//! 将加密数据存储到用户的 GitHub Gist。
//!
//! ## 存储策略
//! - 使用单个 gist 作为同步文件（文件名 `netcatty-vault.json`）
//! - initializeSync 时创建或定位该 gist
//! - Gist ID 存储在本地配置中

use crate::cloud_sync::blob_vault::{Blob, BlobMeta, BlobVault};
use crate::cloud_sync::client::CloudApiError;
use crate::cloud_sync::oauth::github_device::GithubOAuthClient;
use crate::cloud_sync::oauth::OAuthTokens;
use async_trait::async_trait;
use futures::AsyncReadExt;
use gpui::http_client::{AsyncBody, HttpClient, Method, Request, StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Gist 文件
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GistFile {
    filename: String,
    content: String,
}

/// Gist 描述
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GistDescription {
    #[serde(rename = "type")]
    gist_type: String,
}

/// 创建 Gist 请求
#[derive(Debug, Serialize)]
struct CreateGistRequest {
    description: String,
    #[serde(rename = "public")]
    is_public: bool,
    files: std::collections::HashMap<String, GistFile>,
}

/// 更新 Gist 请求
#[derive(Debug, Serialize)]
struct UpdateGistRequest {
    description: Option<String>,
    files: std::collections::HashMap<String, GistFile>,
}

/// Gist 响应
#[derive(Debug, Deserialize)]
struct GistResponse {
    id: String,
    #[serde(default)]
    files: std::collections::HashMap<String, GistFileResponse>,
}

/// Gist 文件响应
#[derive(Debug, Deserialize)]
struct GistFileResponse {
    content: Option<String>,
    filename: String,
}

/// GitHub Gist 同步配置（持久化到 AppSettings）
#[derive(Debug, Clone, Default)]
pub struct GithubGistSettings {
    pub client_id: String,
    pub gist_id: Option<String>,
}

/// GitHub Gist Blob Vault
pub struct GithubGistVault {
    http: Arc<dyn HttpClient>,
    client: GithubOAuthClient,
    gist_id: Option<String>,
    tokens: Option<OAuthTokens>,
}

use base64::Engine as _;

impl GithubGistVault {
    pub fn new(http: Arc<dyn HttpClient>, client_id: String) -> Self {
        let client = GithubOAuthClient::new(Arc::clone(&http), client_id);
        Self {
            http,
            client,
            gist_id: None,
            tokens: None,
        }
    }

    pub fn with_gist_id(mut self, gist_id: String) -> Self {
        self.gist_id = Some(gist_id);
        self
    }

    pub fn with_tokens(mut self, tokens: OAuthTokens) -> Self {
        self.tokens = Some(tokens);
        self
    }

    pub fn gist_id(&self) -> Option<&str> {
        self.gist_id.as_deref()
    }

    fn set_gist_id(&mut self, id: String) {
        self.gist_id = Some(id);
    }

    fn tokens(&self) -> Result<&OAuthTokens, CloudApiError> {
        self.tokens.as_ref().ok_or(CloudApiError::NotAuthenticated)
    }

    /// 设置 tokens 并自动查找或创建 vault gist
    pub async fn authenticate(&mut self) -> Result<String, CloudApiError> {
        // 1. 启动 Device Flow 获取 user_code
        let (verification_uri, user_code) = self.client.start_device_flow().await?;
        tracing::info!("GitHub Device Flow: 打开 {} 并输入代码 {}", verification_uri, user_code);

        // 2. 轮询 token（最大等待约 5 分钟）
        let interval = 5; // GitHub 默认 interval
        let max_attempts = 60;
        let tokens = self.client.poll_for_token(&user_code, interval, max_attempts).await?;
        self.tokens = Some(tokens.clone());

        // 3. 查找或创建 vault gist
        let gist_id = if let Some(id) = self.find_vault_gist(&tokens).await? {
            tracing::info!("找到现有 vault gist: {}", id);
            id
        } else {
            let id = self.create_vault_gist(&tokens).await?;
            tracing::info!("创建新 vault gist: {}", id);
            id
        };

        self.gist_id = Some(gist_id.clone());
        Ok(gist_id)
    }

    /// 查找 netcatty-vault gist
    async fn find_vault_gist(&self, tokens: &OAuthTokens) -> Result<Option<String>, CloudApiError> {
        let req = Request::builder()
            .method(Method::GET)
            .uri("https://api.github.com/gists")
            .header("Authorization", format!("Bearer {}", tokens.access_token))
            .header("User-Agent", "onetcli")
            .header("Accept", "application/vnd.github+json")
            .body(AsyncBody::empty())
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let response = self
            .http
            .send(req)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(CloudApiError::ServerError(format!(
                "列出 gists 失败: HTTP {}",
                response.status().as_u16()
            )));
        }

        let mut bytes = Vec::new();
        response.into_body().read_to_end(&mut bytes).await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        // 解析 gists 列表，查找 netcatty-vault.json
        let gists: Vec<GistResponse> = serde_json::from_slice(&bytes)
            .map_err(|e| CloudApiError::ParseError(e.to_string()))?;

        for gist in gists {
            if gist.files.contains_key("netcatty-vault.json") {
                return Ok(Some(gist.id));
            }
        }
        Ok(None)
    }

    /// 创建 vault gist
    async fn create_vault_gist(&self, tokens: &OAuthTokens) -> Result<String, CloudApiError> {
        let mut files = std::collections::HashMap::new();
        files.insert(
            "netcatty-vault.json".to_string(),
            GistFile { filename: "netcatty-vault.json".to_string(), content: "{}".to_string() },
        );

        let body = serde_json::to_vec(&CreateGistRequest {
            description: "onetcli sync vault".to_string(),
            is_public: false,
            files,
        }).map_err(|e| CloudApiError::DataFormatError(e.to_string()))?;

        let req = Request::builder()
            .method(Method::POST)
            .uri("https://api.github.com/gists")
            .header("Authorization", format!("Bearer {}", tokens.access_token))
            .header("User-Agent", "onetcli")
            .header("Accept", "application/vnd.github+json")
            .header("Content-Type", "application/json")
            .body(AsyncBody::from(body))
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let response = self
            .http
            .send(req)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        if !response.status().is_success() && response.status() != StatusCode::CREATED {
            return Err(CloudApiError::ServerError(format!(
                "创建 gist 失败: HTTP {}",
                response.status().as_u16()
            )));
        }

        let mut bytes = Vec::new();
        response.into_body().read_to_end(&mut bytes).await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let gist: GistResponse = serde_json::from_slice(&bytes)
            .map_err(|e| CloudApiError::ParseError(e.to_string()))?;

        Ok(gist.id)
    }

    /// 获取 gist 内容
    async fn get_gist(&self, gist_id: &str, tokens: &OAuthTokens) -> Result<Option<String>, CloudApiError> {
        let uri = format!("https://api.github.com/gists/{}", gist_id);
        let req = Request::builder()
            .method(Method::GET)
            .uri(&uri)
            .header("Authorization", format!("Bearer {}", tokens.access_token))
            .header("User-Agent", "onetcli")
            .header("Accept", "application/vnd.github+json")
            .body(AsyncBody::empty())
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let response = self
            .http
            .send(req)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(CloudApiError::ServerError(format!(
                "获取 gist 失败: HTTP {}",
                response.status().as_u16()
            )));
        }

        let mut bytes = Vec::new();
        response.into_body().read_to_end(&mut bytes).await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let gist: GistResponse = serde_json::from_slice(&bytes)
            .map_err(|e| CloudApiError::ParseError(e.to_string()))?;

        if let Some(file) = gist.files.get("netcatty-vault.json") {
            Ok(file.content.clone())
        } else {
            Ok(None)
        }
    }

    /// 更新 gist
    async fn update_gist(&self, gist_id: &str, content: &str, tokens: &OAuthTokens) -> Result<(), CloudApiError> {
        let mut files = std::collections::HashMap::new();
        files.insert(
            "netcatty-vault.json".to_string(),
            GistFile { filename: "netcatty-vault.json".to_string(), content: content.to_string() },
        );

        let body = serde_json::to_vec(&UpdateGistRequest {
            description: None,
            files,
        }).map_err(|e| CloudApiError::DataFormatError(e.to_string()))?;

        let uri = format!("https://api.github.com/gists/{}", gist_id);
        let req = Request::builder()
            .method(Method::PATCH)
            .uri(&uri)
            .header("Authorization", format!("Bearer {}", tokens.access_token))
            .header("User-Agent", "onetcli")
            .header("Accept", "application/vnd.github+json")
            .header("Content-Type", "application/json")
            .body(AsyncBody::from(body))
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let response = self
            .http
            .send(req)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(CloudApiError::ServerError(format!(
                "更新 gist 失败: HTTP {}",
                response.status().as_u16()
            )));
        }

        Ok(())
    }

    /// 删除 gist
    async fn delete_gist(&self, gist_id: &str, tokens: &OAuthTokens) -> Result<(), CloudApiError> {
        let uri = format!("https://api.github.com/gists/{}", gist_id);
        let req = Request::builder()
            .method(Method::DELETE)
            .uri(&uri)
            .header("Authorization", format!("Bearer {}", tokens.access_token))
            .header("User-Agent", "onetcli")
            .header("Accept", "application/vnd.github+json")
            .body(AsyncBody::empty())
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let response = self
            .http
            .send(req)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        if !response.status().is_success() && response.status() != StatusCode::NOT_FOUND {
            return Err(CloudApiError::ServerError(format!(
                "删除 gist 失败: HTTP {}",
                response.status().as_u16()
            )));
        }

        Ok(())
    }
}

#[async_trait]
impl BlobVault for GithubGistVault {
    fn backend_type(&self) -> &'static str {
        "github_gist"
    }

    /// 上传加密数据到 Gist（key 被忽略，所有数据存在同一 gist）
    async fn upload(&self, key: &str, data: Vec<u8>) -> Result<BlobMeta, CloudApiError> {
        let gist_id = self.gist_id.clone().ok_or_else(|| {
            CloudApiError::AuthenticationFailed("尚未初始化 Gist，请先完成 GitHub 授权".to_string())
        })?;
        let tokens = self.tokens()?;

        let encoded = base64::engine::general_purpose::STANDARD.encode(&data);
        self.update_gist(&gist_id, &encoded, tokens).await?;

        Ok(BlobMeta {
            key: key.to_string(),
            size: data.len() as u64,
            updated_at: chrono::Utc::now().timestamp_millis(),
        })
    }

    /// 下载 Gist 中的加密数据
    async fn download(&self, key: &str) -> Result<Blob, CloudApiError> {
        let gist_id = self.gist_id.clone().ok_or_else(|| {
            CloudApiError::AuthenticationFailed("尚未初始化 Gist，请先完成 GitHub 授权".to_string())
        })?;
        let tokens = self.tokens()?;

        let content = self.get_gist(&gist_id, tokens).await?
            .ok_or_else(|| CloudApiError::NotFound("vault gist 不存在".to_string()))?;

        let data = base64::engine::general_purpose::STANDARD.decode(&content).map_err(|e| {
            CloudApiError::DataFormatError(format!("base64 解码失败: {}", e))
        })?;

        Ok(Blob {
            key: key.to_string(),
            data,
            updated_at: chrono::Utc::now().timestamp_millis(),
        })
    }

    async fn delete(&self, _key: &str) -> Result<(), CloudApiError> {
        let gist_id = self.gist_id.clone().ok_or_else(|| {
            CloudApiError::AuthenticationFailed("尚未初始化 Gist，请先完成 GitHub 授权".to_string())
        })?;
        let tokens = self.tokens()?;
        self.delete_gist(&gist_id, tokens).await
    }

    async fn exists(&self, _key: &str) -> Result<bool, CloudApiError> {
        // 检查本地是否有 gist_id（初始化后缓存的值）
        if self.gist_id.is_some() {
            return Ok(true);
        }
        // 尝试查找现有的 vault gist
        if let Some(ref tokens) = self.tokens {
            if let Ok(Some(_)) = self.find_vault_gist(tokens).await {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn list(&self, _prefix: Option<&str>) -> Result<Vec<BlobMeta>, CloudApiError> {
        // Gist 是单文件存储，list 返回 vault 文件的元信息
        if self.gist_id.is_some() {
            Ok(vec![BlobMeta {
                key: "vault".to_string(),
                size: 0,
                updated_at: chrono::Utc::now().timestamp_millis(),
            }])
        } else {
            Ok(vec![])
        }
    }
}
