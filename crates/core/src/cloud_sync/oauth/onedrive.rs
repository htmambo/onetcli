//! Microsoft OneDrive Blob Vault
//!
//! 通过 Microsoft Graph API 存储加密同步数据。
//!
//! ## OAuth PKCE 流程
//! 1. 生成 code_verifier
//! 2. 计算 code_challenge = BASE64URL(SHA256(verifier))
//! 3. 打开浏览器到授权 URL
//! 4. 启动本地回调服务器
//! 5. 用 code 换 access_token

use crate::cloud_sync::blob_vault::{Blob, BlobMeta, BlobVault};
use crate::cloud_sync::client::CloudApiError;
use crate::cloud_sync::oauth::callback_server::start_callback_server;
use crate::cloud_sync::oauth::pkce::{generate_code_challenge, generate_code_verifier};
use crate::cloud_sync::oauth::{OAuthConfig, OAuthTokens};
use async_trait::async_trait;
use futures::AsyncReadExt;
use gpui::http_client::{AsyncBody, HttpClient, Method, Request, StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Microsoft OAuth Token 响应
#[derive(Debug, Deserialize)]
struct MsTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
    token_type: String,
    #[serde(default)]
    scope: String,
}

impl MsTokenResponse {
    fn into_tokens(self) -> OAuthTokens {
        let expires_at = chrono::Utc::now().timestamp() + self.expires_in as i64;
        OAuthTokens {
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            token_type: self.token_type,
            expires_in: Some(self.expires_in),
            expires_at: Some(expires_at),
        }
    }
}

/// OneDrive 文件响应
#[derive(Debug, Deserialize)]
struct DriveItem {
    id: String,
    name: String,
    #[serde(default)]
    size: Option<i64>,
    #[serde(default)]
    last_modified_date_time: Option<String>,
}

/// OneDrive 列出响应
#[derive(Debug, Deserialize)]
struct DriveChildrenResponse {
    value: Vec<DriveItem>,
    #[serde(default)]
    #[serde(rename = "@odata.nextLink")]
    next_link: Option<String>,
}

/// OneDrive 上传会话响应
#[derive(Debug, Deserialize)]
struct UploadSessionResponse {
    pub upload_url: String,
    pub expiration_date_time: String,
}

/// Microsoft Graph API 错误
#[derive(Debug, Deserialize)]
struct GraphError {
    error: GraphErrorDetail,
}

#[derive(Debug, Deserialize)]
struct GraphErrorDetail {
    code: String,
    message: String,
}

/// OneDrive Blob Vault
pub struct OneDriveVault {
    http: Arc<dyn HttpClient>,
    client_id: String,
    client_secret: Option<String>,
    tokens: Option<OAuthTokens>,
    root_id: Option<String>,
}

impl OneDriveVault {
    pub fn new(http: Arc<dyn HttpClient>, client_id: String) -> Self {
        Self {
            http,
            client_id,
            client_secret: None,
            tokens: None,
            root_id: None,
        }
    }

    pub fn with_tokens(mut self, tokens: OAuthTokens) -> Self {
        self.tokens = Some(tokens);
        self
    }

    pub fn with_secret(mut self, secret: String) -> Self {
        self.client_secret = Some(secret);
        self
    }

    fn tokens(&self) -> Result<&OAuthTokens, CloudApiError> {
        self.tokens.as_ref().ok_or(CloudApiError::NotAuthenticated)
    }

    /// 开始 PKCE OAuth 流程
    pub async fn authenticate(&mut self, config: &OAuthConfig) -> Result<OAuthTokens, CloudApiError> {
        let code_verifier = generate_code_verifier();
        let code_challenge = generate_code_challenge(&code_verifier);
        let state = format!("netcatty_{}", chrono::Utc::now().timestamp_millis());

        // 构建授权 URL
        let _auth_url = format!(
            "{}?{}",
            config.provider.auth_url(),
            [
                ("client_id", config.client_id.as_str()),
                ("response_type", "code"),
                ("redirect_uri", &config.redirect_uri),
                ("scope", &config.scopes),
                ("state", &state),
                ("code_challenge", &code_challenge),
                ("code_challenge_method", "S256"),
            ]
            .iter()
            .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&")
        );

        // 启动回调服务器
        let redirect_port = config
            .redirect_uri
            .strip_prefix("http://localhost:")
            .and_then(|s| s.split('/').next())
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(8787);

        let (_port, callback) =
            start_callback_server(redirect_port, 300).map_err(|e| CloudApiError::AuthenticationFailed(e.to_string()))?;

        if !callback.is_success() {
            return Err(CloudApiError::AuthenticationFailed(
                callback.error_description.unwrap_or_else(|| "授权失败".to_string()),
            ));
        }

        // 验证 state
        if callback.state.as_deref() != Some(&state) {
            return Err(CloudApiError::AuthenticationFailed("State 不匹配".to_string()));
        }

        // 用 code 换 token
        let token_url = config.provider.token_url();
        let code = callback.code;

        let mut body_parts = vec![
            format!("grant_type=authorization_code"),
            format!("client_id={}", urlencoding::encode(&config.client_id)),
            format!("code={}", urlencoding::encode(&code)),
            format!("redirect_uri={}", urlencoding::encode(&config.redirect_uri)),
            format!("code_verifier={}", urlencoding::encode(&code_verifier)),
        ];

        if let Some(ref secret) = config.client_secret {
            body_parts.push(format!("client_secret={}", urlencoding::encode(secret)));
        }

        let body = body_parts.join("&");

        let req = Request::builder()
            .method(Method::POST)
            .uri(token_url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "application/json")
            .body(AsyncBody::from(body.into_bytes()))
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let response = self
            .http
            .send(req)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let status = response.status();
        let mut bytes = Vec::new();
        response.into_body().read_to_end(&mut bytes).await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        if !status.is_success() {
            return Err(CloudApiError::ServerError(format!(
                "OneDrive token 请求失败: {}",
                String::from_utf8_lossy(&bytes)
            )));
        }

        let token_resp: MsTokenResponse = serde_json::from_slice(&bytes)
            .map_err(|e| CloudApiError::ParseError(e.to_string()))?;

        let tokens = token_resp.into_tokens();
        self.tokens = Some(tokens.clone());
        Ok(tokens)
    }

    /// 获取 vault 文件夹 ID
    async fn get_or_create_vault_folder(&mut self) -> Result<String, CloudApiError> {
        if let Some(ref id) = self.root_id {
            return Ok(id.clone());
        }

        let tokens = self.tokens.as_ref().ok_or(CloudApiError::NotAuthenticated)?;

        // 查询 netcatty-vault 文件夹
        let query = urlencoding::encode("name='netcatty-vault' and folder");
        let uri = format!(
            "https://graph.microsoft.com/v1.0/me/drive/root/children?q={}",
            query
        );

        let req = Request::builder()
            .method(Method::GET)
            .uri(&uri)
            .header("Authorization", format!("Bearer {}", tokens.access_token))
            .header("Accept", "application/json")
            .body(AsyncBody::empty())
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let response = self
            .http
            .send(req)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let status = response.status();
        let mut bytes = Vec::new();
        response.into_body().read_to_end(&mut bytes).await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        if status.is_success() {
            let children: DriveChildrenResponse = serde_json::from_slice(&bytes)
                .map_err(|e| CloudApiError::ParseError(e.to_string()))?;
            if let Some(folder) = children.value.into_iter().find(|i| i.name == "netcatty-vault") {
                self.root_id = Some(folder.id.clone());
                return Ok(folder.id);
            }
        }

        // 创建文件夹
        let folder_id = self.create_folder("netcatty-vault", None).await?;
        self.root_id = Some(folder_id.clone());
        Ok(folder_id)
    }

    /// 创建文件夹
    async fn create_folder(&self, name: &str, parent_id: Option<&str>) -> Result<String, CloudApiError> {
        let tokens = self.tokens.as_ref().ok_or(CloudApiError::NotAuthenticated)?;

        #[derive(Serialize)]
        struct CreateFolderRequest<'a> {
            name: &'a str,
            folder: FolderRef,
            #[serde(rename = "@microsoft.graph.conflictBehavior")]
            conflict_behavior: &'a str,
        }

        #[derive(Serialize)]
        struct FolderRef {}

        let parent_path = if let Some(pid) = parent_id {
            format!("me/drive/items/{}/children", pid)
        } else {
            "me/drive/root/children".to_string()
        };

        let uri = format!("https://graph.microsoft.com/v1.0/{}", parent_path);

        let body = serde_json::to_vec(&CreateFolderRequest {
            name,
            folder: FolderRef {},
            conflict_behavior: "rename",
        }).map_err(|e| CloudApiError::DataFormatError(e.to_string()))?;

        let req = Request::builder()
            .method(Method::POST)
            .uri(&uri)
            .header("Authorization", format!("Bearer {}", tokens.access_token))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(AsyncBody::from(body))
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let response = self
            .http
            .send(req)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            let mut bytes = Vec::new();
            response.into_body().read_to_end(&mut bytes).await.ok();
            return Err(CloudApiError::ServerError(format!(
                "创建 OneDrive 文件夹失败: {}",
                String::from_utf8_lossy(&bytes)
            )));
        }

        let mut bytes = Vec::new();
        response.into_body().read_to_end(&mut bytes).await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let item: DriveItem = serde_json::from_slice(&bytes)
            .map_err(|e| CloudApiError::ParseError(e.to_string()))?;

        Ok(item.id)
    }

    /// 上传文件
    async fn upload_file(&self, name: &str, content: &[u8], folder_id: &str) -> Result<BlobMeta, CloudApiError> {
        let tokens = self.tokens.as_ref().ok_or(CloudApiError::NotAuthenticated)?;
        let now = chrono::Utc::now().timestamp_millis();

        let uri = format!(
            "https://graph.microsoft.com/v1.0/me/drive/items/{}:/{}:/content",
            folder_id,
            urlencoding::encode(name)
        );

        let req = Request::builder()
            .method(Method::PUT)
            .uri(&uri)
            .header("Authorization", format!("Bearer {}", tokens.access_token))
            .header("Content-Type", "application/octet-stream")
            .header("Accept", "application/json")
            .body(AsyncBody::from(content.to_vec()))
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let response = self
            .http
            .send(req)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            let mut bytes = Vec::new();
            response.into_body().read_to_end(&mut bytes).await.ok();
            return Err(CloudApiError::ServerError(format!(
                "上传 OneDrive 文件失败: {}",
                String::from_utf8_lossy(&bytes)
            )));
        }

        let mut bytes = Vec::new();
        response.into_body().read_to_end(&mut bytes).await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let item: DriveItem = serde_json::from_slice(&bytes)
            .map_err(|e| CloudApiError::ParseError(e.to_string()))?;

        Ok(BlobMeta {
            key: item.id,
            size: content.len() as u64,
            updated_at: now,
        })
    }

    /// 下载文件
    async fn download_file(&self, item_id: &str) -> Result<Blob, CloudApiError> {
        let tokens = self.tokens.as_ref().ok_or(CloudApiError::NotAuthenticated)?;
        let now = chrono::Utc::now().timestamp_millis();

        let uri = format!("https://graph.microsoft.com/v1.0/me/drive/items/{}/content", item_id);

        let req = Request::builder()
            .method(Method::GET)
            .uri(&uri)
            .header("Authorization", format!("Bearer {}", tokens.access_token))
            .body(AsyncBody::empty())
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let response = self
            .http
            .send(req)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        if response.status() == StatusCode::NOT_FOUND {
            return Err(CloudApiError::NotFound(item_id.to_string()));
        }
        if !response.status().is_success() {
            return Err(CloudApiError::ServerError(format!(
                "下载 OneDrive 文件失败: HTTP {}",
                response.status().as_u16()
            )));
        }

        let mut bytes = Vec::new();
        response.into_body().read_to_end(&mut bytes).await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        Ok(Blob { key: item_id.to_string(), data: bytes, updated_at: now })
    }

    /// 删除文件
    async fn delete_file(&self, item_id: &str) -> Result<(), CloudApiError> {
        let tokens = self.tokens.as_ref().ok_or(CloudApiError::NotAuthenticated)?;

        let uri = format!("https://graph.microsoft.com/v1.0/me/drive/items/{}", item_id);

        let req = Request::builder()
            .method(Method::DELETE)
            .uri(&uri)
            .header("Authorization", format!("Bearer {}", tokens.access_token))
            .body(AsyncBody::empty())
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let response = self
            .http
            .send(req)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        if !response.status().is_success() && response.status() != StatusCode::NOT_FOUND {
            return Err(CloudApiError::ServerError(format!(
                "删除 OneDrive 文件失败: HTTP {}",
                response.status().as_u16()
            )));
        }

        Ok(())
    }

    /// 查找 vault 文件
    async fn find_vault_file(&self, folder_id: &str, name: &str) -> Result<Option<DriveItem>, CloudApiError> {
        let tokens = self.tokens.as_ref().ok_or(CloudApiError::NotAuthenticated)?;

        let uri = format!(
            "https://graph.microsoft.com/v1.0/me/drive/items/{}/children?$filter=name eq '{}'",
            folder_id,
            name
        );

        let req = Request::builder()
            .method(Method::GET)
            .uri(&uri)
            .header("Authorization", format!("Bearer {}", tokens.access_token))
            .header("Accept", "application/json")
            .body(AsyncBody::empty())
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let response = self
            .http
            .send(req)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let mut bytes = Vec::new();
        response.into_body().read_to_end(&mut bytes).await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let children: DriveChildrenResponse = serde_json::from_slice(&bytes)
            .map_err(|e| CloudApiError::ParseError(e.to_string()))?;

        Ok(children.value.into_iter().find(|i| i.name == name))
    }

    /// 列出 vault 中的所有文件
    async fn list_vault_files(&self, folder_id: &str) -> Result<Vec<BlobMeta>, CloudApiError> {
        let tokens = self.tokens.as_ref().ok_or(CloudApiError::NotAuthenticated)?;
        let now = chrono::Utc::now().timestamp_millis();

        let uri = format!(
            "https://graph.microsoft.com/v1.0/me/drive/items/{}/children",
            folder_id
        );

        let req = Request::builder()
            .method(Method::GET)
            .uri(&uri)
            .header("Authorization", format!("Bearer {}", tokens.access_token))
            .header("Accept", "application/json")
            .body(AsyncBody::empty())
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let response = self
            .http
            .send(req)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let mut bytes = Vec::new();
        response.into_body().read_to_end(&mut bytes).await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let children: DriveChildrenResponse = serde_json::from_slice(&bytes)
            .map_err(|e| CloudApiError::ParseError(e.to_string()))?;

        Ok(children
            .value
            .into_iter()
            .map(|item| BlobMeta {
                key: item.id,
                size: item.size.unwrap_or(0) as u64,
                updated_at: now,
            })
            .collect())
    }
}

#[async_trait]
impl BlobVault for OneDriveVault {
    fn backend_type(&self) -> &'static str {
        "onedrive"
    }

    async fn upload(&self, _key: &str, _data: Vec<u8>) -> Result<BlobMeta, CloudApiError> {
        Err(CloudApiError::NotSupported("请使用 OneDriveVault 专用方法".to_string()))
    }

    async fn download(&self, _key: &str) -> Result<Blob, CloudApiError> {
        Err(CloudApiError::NotSupported("请使用 OneDriveVault 专用方法".to_string()))
    }

    async fn delete(&self, _key: &str) -> Result<(), CloudApiError> {
        Err(CloudApiError::NotSupported("请使用 OneDriveVault 专用方法".to_string()))
    }

    async fn exists(&self, _key: &str) -> Result<bool, CloudApiError> {
        Err(CloudApiError::NotSupported("请使用 OneDriveVault 专用方法".to_string()))
    }

    async fn list(&self, _prefix: Option<&str>) -> Result<Vec<BlobMeta>, CloudApiError> {
        Err(CloudApiError::NotSupported("请使用 OneDriveVault 专用方法".to_string()))
    }
}
