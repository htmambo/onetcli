//! Microsoft OneDrive Blob Vault
//!
//! 通过 Microsoft Graph API 存储加密同步数据。
//! 授权流程由 `main/src/settings/oauth_dialog.rs` 负责。

use crate::cloud_sync::blob_vault::{Blob, BlobMeta, BlobVault};
use crate::cloud_sync::client::CloudApiError;
use crate::cloud_sync::oauth::OAuthTokens;
use async_trait::async_trait;
use futures::AsyncReadExt;
use gpui::http_client::{AsyncBody, HttpClient, Method, Request, StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// OneDrive 文件响应
#[derive(Debug, Deserialize)]
struct DriveItem {
    id: String,
    name: String,
    #[serde(default)]
    size: Option<i64>,
}

/// OneDrive 列出响应
#[derive(Debug, Deserialize)]
struct DriveChildrenResponse {
    value: Vec<DriveItem>,
}

/// OneDrive Blob Vault
pub struct OneDriveVault {
    http: Arc<dyn HttpClient>,
    tokens: Arc<Mutex<Option<OAuthTokens>>>,
    root_id: Arc<Mutex<Option<String>>>,
}

impl OneDriveVault {
    pub fn new(http: Arc<dyn HttpClient>) -> Self {
        Self {
            http,
            tokens: Arc::new(Mutex::new(None)),
            root_id: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_tokens(&self, tokens: OAuthTokens) -> Arc<Self> {
        *self.tokens.lock().unwrap() = Some(tokens);
        Arc::new(Self {
            http: Arc::clone(&self.http),
            tokens: Arc::clone(&self.tokens),
            root_id: Arc::clone(&self.root_id),
        })
    }

    fn tokens(&self) -> Result<OAuthTokens, CloudApiError> {
        self.tokens
            .lock()
            .unwrap()
            .clone()
            .ok_or(CloudApiError::NotAuthenticated)
    }

    /// 获取 vault 文件夹 ID
    async fn get_or_create_vault_folder(&self) -> Result<String, CloudApiError> {
        if let Some(ref id) = *self.root_id.lock().unwrap() {
            return Ok(id.clone());
        }

        let tokens = self.tokens()?;

        // 查询 ONetCli-vault 文件夹
        let query = urlencoding::encode("name='ONetCli-vault' and folder");
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
        response
            .into_body()
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        if status.is_success() {
            let children: DriveChildrenResponse = serde_json::from_slice(&bytes)
                .map_err(|e| CloudApiError::ParseError(e.to_string()))?;
            if let Some(folder) = children
                .value
                .into_iter()
                .find(|i| i.name == "ONetCli-vault")
            {
                *self.root_id.lock().unwrap() = Some(folder.id.clone());
                return Ok(folder.id);
            }
        }

        // 创建文件夹
        let folder_id = self.create_folder("ONetCli-vault", None).await?;
        *self.root_id.lock().unwrap() = Some(folder_id.clone());
        Ok(folder_id)
    }

    /// 创建文件夹
    async fn create_folder(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> Result<String, CloudApiError> {
        let tokens = self.tokens()?;

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
        })
        .map_err(|e| CloudApiError::DataFormatError(e.to_string()))?;

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
        response
            .into_body()
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let item: DriveItem =
            serde_json::from_slice(&bytes).map_err(|e| CloudApiError::ParseError(e.to_string()))?;

        Ok(item.id)
    }

    /// 上传文件
    async fn upload_file(
        &self,
        name: &str,
        content: &[u8],
        folder_id: &str,
    ) -> Result<BlobMeta, CloudApiError> {
        let tokens = self.tokens()?;
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
        response
            .into_body()
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let item: DriveItem =
            serde_json::from_slice(&bytes).map_err(|e| CloudApiError::ParseError(e.to_string()))?;

        Ok(BlobMeta {
            key: item.id,
            size: content.len() as u64,
            updated_at: now,
        })
    }

    /// 下载文件
    async fn download_file(&self, item_id: &str) -> Result<Blob, CloudApiError> {
        let tokens = self.tokens()?;
        let now = chrono::Utc::now().timestamp_millis();

        let uri = format!(
            "https://graph.microsoft.com/v1.0/me/drive/items/{}/content",
            item_id
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
            return Err(CloudApiError::NotFound(item_id.to_string()));
        }
        if !response.status().is_success() {
            return Err(CloudApiError::ServerError(format!(
                "下载 OneDrive 文件失败: HTTP {}",
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
            key: item_id.to_string(),
            data: bytes,
            updated_at: now,
        })
    }

    /// 删除文件
    async fn delete_file(&self, item_id: &str) -> Result<(), CloudApiError> {
        let tokens = self.tokens()?;

        let uri = format!(
            "https://graph.microsoft.com/v1.0/me/drive/items/{}",
            item_id
        );

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
    async fn find_vault_file(
        &self,
        folder_id: &str,
        name: &str,
    ) -> Result<Option<DriveItem>, CloudApiError> {
        let tokens = self.tokens()?;

        let uri = format!(
            "https://graph.microsoft.com/v1.0/me/drive/items/{}/children?$filter=name eq '{}'",
            folder_id, name
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
        response
            .into_body()
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let children: DriveChildrenResponse =
            serde_json::from_slice(&bytes).map_err(|e| CloudApiError::ParseError(e.to_string()))?;

        Ok(children.value.into_iter().find(|i| i.name == name))
    }

    /// 列出 vault 中的所有文件
    async fn list_vault_files(&self, folder_id: &str) -> Result<Vec<BlobMeta>, CloudApiError> {
        let tokens = self.tokens()?;
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
        response
            .into_body()
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let children: DriveChildrenResponse =
            serde_json::from_slice(&bytes).map_err(|e| CloudApiError::ParseError(e.to_string()))?;

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

    async fn upload(&self, key: &str, data: Vec<u8>) -> Result<BlobMeta, CloudApiError> {
        let folder_id = self.get_or_create_vault_folder().await?;
        self.upload_file(key, &data, &folder_id).await
    }

    async fn download(&self, key: &str) -> Result<Blob, CloudApiError> {
        let folder_id = self.get_or_create_vault_folder().await?;
        let file = self.find_vault_file(&folder_id, key).await?;
        let item_id = file
            .map(|f| f.id)
            .ok_or_else(|| CloudApiError::NotFound(format!("OneDrive 中未找到文件: {}", key)))?;
        self.download_file(&item_id).await
    }

    async fn delete(&self, key: &str) -> Result<(), CloudApiError> {
        let folder_id = self.get_or_create_vault_folder().await?;
        let file = self.find_vault_file(&folder_id, key).await?;
        if let Some(f) = file {
            self.delete_file(&f.id).await?;
        }
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, CloudApiError> {
        let folder_id = self.get_or_create_vault_folder().await?;
        let file = self.find_vault_file(&folder_id, key).await?;
        Ok(file.is_some())
    }

    async fn list(&self, _prefix: Option<&str>) -> Result<Vec<BlobMeta>, CloudApiError> {
        let folder_id = self.get_or_create_vault_folder().await?;
        self.list_vault_files(&folder_id).await
    }
}
