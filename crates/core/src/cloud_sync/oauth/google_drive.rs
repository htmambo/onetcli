//! Google Drive Blob Vault
//!
//! 通过 Google Drive API 存储加密同步数据。
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
use std::sync::{Arc, Mutex};

/// Google OAuth Token 响应
#[derive(Debug, Deserialize)]
struct GoogleTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
    token_type: String,
    #[serde(default)]
    id_token: Option<String>,
}

impl GoogleTokenResponse {
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

/// Google Drive 文件响应
#[derive(Debug, Deserialize)]
struct DriveFile {
    id: String,
    name: String,
    mime_type: String,
    #[serde(default)]
    size: Option<String>,
}

/// Google Drive 列出响应
#[derive(Debug, Deserialize)]
struct DriveListResponse {
    files: Vec<DriveFile>,
    #[serde(default)]
    next_page_token: Option<String>,
}

/// Google Drive Blob Vault
pub struct GoogleDriveVault {
    http: Arc<dyn HttpClient>,
    client_id: String,
    client_secret: String,
    tokens: Arc<Mutex<Option<OAuthTokens>>>,
    folder_id: Arc<Mutex<Option<String>>>,
}

impl GoogleDriveVault {
    pub fn new(http: Arc<dyn HttpClient>, client_id: String, client_secret: String) -> Self {
        Self {
            http,
            client_id,
            client_secret,
            tokens: Arc::new(Mutex::new(None)),
            folder_id: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_tokens(&self, tokens: OAuthTokens) -> Arc<Self> {
        *self.tokens.lock().unwrap() = Some(tokens);
        Arc::new(Self {
            http: Arc::clone(&self.http),
            client_id: self.client_id.clone(),
            client_secret: self.client_secret.clone(),
            tokens: Arc::clone(&self.tokens),
            folder_id: Arc::clone(&self.folder_id),
        })
    }

    pub fn with_folder_id(&self, folder_id: String) -> Arc<Self> {
        *self.folder_id.lock().unwrap() = Some(folder_id);
        Arc::new(Self {
            http: Arc::clone(&self.http),
            client_id: self.client_id.clone(),
            client_secret: self.client_secret.clone(),
            tokens: Arc::clone(&self.tokens),
            folder_id: Arc::clone(&self.folder_id),
        })
    }

    /// 开始 PKCE OAuth 流程
    pub async fn authenticate(&mut self, config: &OAuthConfig) -> Result<OAuthTokens, CloudApiError> {
        let code_verifier = generate_code_verifier();
        let code_challenge = generate_code_challenge(&code_verifier);
        let state = format!("ONetCli_{}", chrono::Utc::now().timestamp_millis());

        let auth_url = format!(
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
            start_callback_server(redirect_port, 300)
                .map_err(|e| CloudApiError::AuthenticationFailed(e.to_string()))?;

        if !callback.is_success() {
            return Err(CloudApiError::AuthenticationFailed(
                callback
                    .error_description
                    .unwrap_or_else(|| "授权失败".to_string()),
            ));
        }

        if callback.state.as_deref() != Some(&state) {
            return Err(CloudApiError::AuthenticationFailed(
                "State 不匹配".to_string(),
            ));
        }

        let token_url = config.provider.token_url();
        let code = callback.code;

        let mut body_parts = vec![
            "grant_type=authorization_code".to_string(),
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
        response
            .into_body()
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        if !status.is_success() {
            return Err(CloudApiError::ServerError(format!(
                "Google token 请求失败: {}",
                String::from_utf8_lossy(&bytes)
            )));
        }

        let token_resp: GoogleTokenResponse =
            serde_json::from_slice(&bytes).map_err(|e| CloudApiError::ParseError(e.to_string()))?;

        let tokens = token_resp.into_tokens();
        *self.tokens.lock().unwrap() = Some(tokens.clone());
        // 获取或创建 vault 文件夹
        let folder_id = self.get_or_create_app_folder().await?;
        *self.folder_id.lock().unwrap() = Some(folder_id);
        Ok(tokens)
    }

    fn tokens(&self) -> Result<OAuthTokens, CloudApiError> {
        self.tokens.lock().unwrap().clone().ok_or(CloudApiError::NotAuthenticated)
    }

    /// 获取或创建应用专属文件夹
    async fn get_or_create_app_folder(&self) -> Result<String, CloudApiError> {
        if let Some(ref id) = *self.folder_id.lock().unwrap() {
            return Ok(id.clone());
        }

        let tokens = self.tokens()?;

        // 查询应用文件夹
        let query = "name='ONetCli-vault' and mimeType='application/vnd.google-apps.folder' and trashed=false";
        let uri = format!(
            "https://www.googleapis.com/drive/v3/files?q={}",
            urlencoding::encode(query)
        );

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

        let mut bytes = Vec::new();
        response
            .into_body()
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let list: DriveListResponse =
            serde_json::from_slice(&bytes).map_err(|e| CloudApiError::ParseError(e.to_string()))?;

        if let Some(folder) = list.files.into_iter().next() {
            *self.folder_id.lock().unwrap() = Some(folder.id.clone());
            return Ok(folder.id);
        }

        // 创建文件夹
        let folder_id = self.create_folder("ONetCli-vault", None).await?;
        *self.folder_id.lock().unwrap() = Some(folder_id.clone());
        Ok(folder_id)
    }

    /// 创建文件夹
    async fn create_folder(&self, name: &str, parent_id: Option<&str>) -> Result<String, CloudApiError> {
        let tokens = self.tokens()?;

        #[derive(Serialize)]
        struct CreateFolderRequest<'a> {
            name: &'a str,
            mime_type: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            parents: Option<Vec<&'a str>>,
        }

        let body = serde_json::to_vec(&CreateFolderRequest {
            name,
            mime_type: "application/vnd.google-apps.folder",
            parents: parent_id.map(|p| vec![p]),
        })
        .map_err(|e| CloudApiError::DataFormatError(e.to_string()))?;

        let req = Request::builder()
            .method(Method::POST)
            .uri("https://www.googleapis.com/drive/v3/files")
            .header("Authorization", format!("Bearer {}", tokens.access_token))
            .header("Content-Type", "application/json")
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
                "创建 Google Drive 文件夹失败: {}",
                String::from_utf8_lossy(&bytes)
            )));
        }

        let mut bytes = Vec::new();
        response
            .into_body()
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let file: DriveFile =
            serde_json::from_slice(&bytes).map_err(|e| CloudApiError::ParseError(e.to_string()))?;

        Ok(file.id)
    }

    /// 上传或更新文件
    async fn upload_file(&self, name: &str, content: &[u8], parent_id: &str) -> Result<BlobMeta, CloudApiError> {
        let tokens = self.tokens()?;
        let now = chrono::Utc::now().timestamp_millis();

        // multipart/form-data 上传
        let boundary = "boundary1234567890";
        let mut body = Vec::new();

        // 元数据部分
        body.extend_from_slice(
            format!(
                "--{}\r\n\
             Content-Type: application/json; charset=UTF-8\r\n\r\n\
             {{\"name\":\"{}\",\"parents\":[\"{}\"]}}\r\n",
                boundary, name, parent_id
            )
            .as_bytes(),
        );

        // 文件内容部分
        body.extend_from_slice(
            format!(
                "--{}\r\n\
             Content-Type: application/octet-stream\r\n\r\n",
                boundary
            )
            .as_bytes(),
        );
        body.extend_from_slice(content);
        body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());

        let req = Request::builder()
            .method(Method::POST)
            .uri("https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart")
            .header("Authorization", format!("Bearer {}", tokens.access_token))
            .header(
                "Content-Type",
                format!("multipart/related; boundary={}", boundary),
            )
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
                "上传 Google Drive 文件失败: {}",
                String::from_utf8_lossy(&bytes)
            )));
        }

        let mut bytes = Vec::new();
        response
            .into_body()
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let file: DriveFile =
            serde_json::from_slice(&bytes).map_err(|e| CloudApiError::ParseError(e.to_string()))?;

        Ok(BlobMeta {
            key: file.id,
            size: content.len() as u64,
            updated_at: now,
        })
    }

    /// 下载文件
    async fn download_file(&self, file_id: &str) -> Result<Blob, CloudApiError> {
        let tokens = self.tokens()?;
        let now = chrono::Utc::now().timestamp_millis();

        let uri = format!(
            "https://www.googleapis.com/drive/v3/files/{}?alt=media",
            file_id
        );
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
            return Err(CloudApiError::NotFound(file_id.to_string()));
        }
        if !response.status().is_success() {
            return Err(CloudApiError::ServerError(format!(
                "下载 Google Drive 文件失败: HTTP {}",
                response.status().as_u16()
            )));
        }

        let mut bytes = Vec::new();
        response
            .into_body()
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        Ok(Blob {
            key: file_id.to_string(),
            data: bytes,
            updated_at: now,
        })
    }

    /// 删除文件
    async fn delete_file(&self, file_id: &str) -> Result<(), CloudApiError> {
        let tokens = self.tokens()?;

        let uri = format!("https://www.googleapis.com/drive/v3/files/{}", file_id);
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
                "删除 Google Drive 文件失败: HTTP {}",
                response.status().as_u16()
            )));
        }

        Ok(())
    }

    /// 列出 vault 中的所有文件
    pub(crate) async fn list_vault_files(&self, folder_id: &str) -> Result<Vec<BlobMeta>, CloudApiError> {
        let tokens = self.tokens()?;
        let now = chrono::Utc::now().timestamp_millis();

        let query = format!("'{}' in parents and trashed=false", folder_id);
        let uri = format!(
            "https://www.googleapis.com/drive/v3/files?q={}&fields=files(id,name,size,mimeType)",
            urlencoding::encode(&query)
        );

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

        let mut bytes = Vec::new();
        response
            .into_body()
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        #[derive(Deserialize)]
        struct FileListResponse { files: Vec<DriveFile> }
        let list: FileListResponse =
            serde_json::from_slice(&bytes).map_err(|e| CloudApiError::ParseError(e.to_string()))?;

        Ok(list
            .files
            .into_iter()
            .map(|f| BlobMeta {
                key: f.id,
                size: f.size.as_ref().and_then(|s| s.parse().ok()).unwrap_or(0),
                updated_at: now,
            })
            .collect())
    }

    /// 查找 vault 中指定名称的文件
    pub(crate) async fn find_vault_file(&self, folder_id: &str, name: &str) -> Result<Option<DriveFile>, CloudApiError> {
        let tokens = self.tokens()?;

        let query = format!(
            "name='{}' and '{}' in parents and trashed=false",
            name, folder_id
        );
        let uri = format!(
            "https://www.googleapis.com/drive/v3/files?q={}",
            urlencoding::encode(&query)
        );

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

        let mut bytes = Vec::new();
        response
            .into_body()
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let list: DriveListResponse =
            serde_json::from_slice(&bytes).map_err(|e| CloudApiError::ParseError(e.to_string()))?;

        Ok(list.files.into_iter().next())
    }
}

#[async_trait]
impl BlobVault for GoogleDriveVault {
    fn backend_type(&self) -> &'static str {
        "google_drive"
    }

    async fn upload(&self, key: &str, data: Vec<u8>) -> Result<BlobMeta, CloudApiError> {
        let folder_id = self.get_or_create_app_folder().await?;
        self.upload_file(key, &data, &folder_id).await
    }

    async fn download(&self, key: &str) -> Result<Blob, CloudApiError> {
        let folder_id = self.get_or_create_app_folder().await?;
        let file = self.find_vault_file(&folder_id, key).await?;
        let file_id = file.map(|f| f.id).ok_or_else(|| {
            CloudApiError::NotFound(format!("Google Drive 中未找到文件: {}", key))
        })?;
        self.download_file(&file_id).await
    }

    async fn delete(&self, key: &str) -> Result<(), CloudApiError> {
        let folder_id = self.get_or_create_app_folder().await?;
        let file = self.find_vault_file(&folder_id, key).await?;
        if let Some(f) = file {
            self.delete_file(&f.id).await?;
        }
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, CloudApiError> {
        let folder_id = self.get_or_create_app_folder().await?;
        let file = self.find_vault_file(&folder_id, key).await?;
        Ok(file.is_some())
    }

    async fn list(&self, _prefix: Option<&str>) -> Result<Vec<BlobMeta>, CloudApiError> {
        let folder_id = self.get_or_create_app_folder().await?;
        self.list_vault_files(&folder_id).await
    }
}
