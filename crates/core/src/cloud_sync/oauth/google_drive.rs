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
use crate::cloud_sync::oauth::OAuthTokens;
use async_trait::async_trait;
use futures::AsyncReadExt;
use gpui::http_client::{AsyncBody, HttpClient, Method, Request, StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
    tokens: Option<OAuthTokens>,
    folder_id: Option<String>,
}

impl GoogleDriveVault {
    pub fn new(http: Arc<dyn HttpClient>, client_id: String, client_secret: String) -> Self {
        Self {
            http,
            client_id,
            client_secret,
            tokens: None,
            folder_id: None,
        }
    }

    pub fn with_tokens(mut self, tokens: OAuthTokens) -> Self {
        self.tokens = Some(tokens);
        self
    }

    pub fn with_folder_id(mut self, folder_id: String) -> Self {
        self.folder_id = Some(folder_id);
        self
    }

    fn tokens(&self) -> Result<&OAuthTokens, CloudApiError> {
        self.tokens.as_ref().ok_or(CloudApiError::NotAuthenticated)
    }

    /// 获取或创建应用专属文件夹
    async fn get_or_create_app_folder(&mut self) -> Result<String, CloudApiError> {
        if let Some(ref id) = self.folder_id {
            return Ok(id.clone());
        }

        let tokens = self.tokens.as_ref().ok_or(CloudApiError::NotAuthenticated)?;

        // 查询应用文件夹
        let query = "name='netcatty-vault' and mimeType='application/vnd.google-apps.folder' and trashed=false";
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
        response.into_body().read_to_end(&mut bytes).await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let list: DriveListResponse = serde_json::from_slice(&bytes)
            .map_err(|e| CloudApiError::ParseError(e.to_string()))?;

        if let Some(folder) = list.files.into_iter().next() {
            self.folder_id = Some(folder.id.clone());
            return Ok(folder.id);
        }

        // 创建文件夹
        let folder_id = self.create_folder("netcatty-vault", None).await?;
        self.folder_id = Some(folder_id.clone());
        Ok(folder_id)
    }

    /// 创建文件夹
    async fn create_folder(&self, name: &str, parent_id: Option<&str>) -> Result<String, CloudApiError> {
        let tokens = self.tokens.as_ref().ok_or(CloudApiError::NotAuthenticated)?;

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
        }).map_err(|e| CloudApiError::DataFormatError(e.to_string()))?;

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
        response.into_body().read_to_end(&mut bytes).await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let file: DriveFile = serde_json::from_slice(&bytes)
            .map_err(|e| CloudApiError::ParseError(e.to_string()))?;

        Ok(file.id)
    }

    /// 上传或更新文件
    async fn upload_file(&self, name: &str, content: &[u8], parent_id: &str) -> Result<BlobMeta, CloudApiError> {
        let tokens = self.tokens.as_ref().ok_or(CloudApiError::NotAuthenticated)?;
        let now = chrono::Utc::now().timestamp_millis();

        // multipart/form-data 上传
        let boundary = "boundary1234567890";
        let mut body = Vec::new();

        // 元数据部分
        body.extend_from_slice(format!(
            "--{}\r\n\
             Content-Type: application/json; charset=UTF-8\r\n\r\n\
             {{\"name\":\"{}\",\"parents\":[\"{}\"]}}\r\n",
            boundary, name, parent_id
        ).as_bytes());

        // 文件内容部分
        body.extend_from_slice(format!(
            "--{}\r\n\
             Content-Type: application/octet-stream\r\n\r\n",
            boundary
        ).as_bytes());
        body.extend_from_slice(content);
        body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());

        let req = Request::builder()
            .method(Method::POST)
            .uri("https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart")
            .header("Authorization", format!("Bearer {}", tokens.access_token))
            .header("Content-Type", format!("multipart/related; boundary={}", boundary))
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
        response.into_body().read_to_end(&mut bytes).await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let file: DriveFile = serde_json::from_slice(&bytes)
            .map_err(|e| CloudApiError::ParseError(e.to_string()))?;

        Ok(BlobMeta {
            key: file.id,
            size: content.len() as u64,
            updated_at: now,
        })
    }

    /// 下载文件
    async fn download_file(&self, file_id: &str) -> Result<Blob, CloudApiError> {
        let tokens = self.tokens.as_ref().ok_or(CloudApiError::NotAuthenticated)?;
        let now = chrono::Utc::now().timestamp_millis();

        let uri = format!("https://www.googleapis.com/drive/v3/files/{}?alt=media", file_id);
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
        response.into_body().read_to_end(&mut bytes).await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        Ok(Blob { key: file_id.to_string(), data: bytes, updated_at: now })
    }

    /// 删除文件
    async fn delete_file(&self, file_id: &str) -> Result<(), CloudApiError> {
        let tokens = self.tokens.as_ref().ok_or(CloudApiError::NotAuthenticated)?;

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

    /// 查找 vault 文件
    async fn find_vault_file(&self, folder_id: &str) -> Result<Option<DriveFile>, CloudApiError> {
        let tokens = self.tokens.as_ref().ok_or(CloudApiError::NotAuthenticated)?;

        let query = format!(
            "name='netcatty-vault.json' and '{}' in parents and trashed=false",
            folder_id
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
        response.into_body().read_to_end(&mut bytes).await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let list: DriveListResponse = serde_json::from_slice(&bytes)
            .map_err(|e| CloudApiError::ParseError(e.to_string()))?;

        Ok(list.files.into_iter().next())
    }
}

#[async_trait]
impl BlobVault for GoogleDriveVault {
    fn backend_type(&self) -> &'static str {
        "google_drive"
    }

    async fn upload(&self, _key: &str, _data: Vec<u8>) -> Result<BlobMeta, CloudApiError> {
        // key 作为文件名，存储到 app folder
        Err(CloudApiError::NotSupported("请使用 GoogleDriveVault 专用方法".to_string()))
    }

    async fn download(&self, _key: &str) -> Result<Blob, CloudApiError> {
        Err(CloudApiError::NotSupported("请使用 GoogleDriveVault 专用方法".to_string()))
    }

    async fn delete(&self, _key: &str) -> Result<(), CloudApiError> {
        Err(CloudApiError::NotSupported("请使用 GoogleDriveVault 专用方法".to_string()))
    }

    async fn exists(&self, _key: &str) -> Result<bool, CloudApiError> {
        Err(CloudApiError::NotSupported("请使用 GoogleDriveVault 专用方法".to_string()))
    }

    async fn list(&self, _prefix: Option<&str>) -> Result<Vec<BlobMeta>, CloudApiError> {
        Err(CloudApiError::NotSupported("请使用 GoogleDriveVault 专用方法".to_string()))
    }
}
