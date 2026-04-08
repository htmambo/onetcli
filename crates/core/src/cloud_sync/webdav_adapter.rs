//! WebDAV Blob Vault 实现
//!
//! 基于 HTTP/WebDAV 协议，将加密同步数据存储到任意 WebDAV 服务器。
//! 支持 Basic/Bearer Token 认证。

use crate::cloud_sync::blob_vault::{Blob, BlobMeta, BlobVault};
use crate::cloud_sync::client::CloudApiError;
use futures::AsyncReadExt;
use gpui::http_client::{AsyncBody, HttpClient, Method, Request, StatusCode};
use std::sync::Arc;

/// WebDAV 认证方式
#[derive(Debug, Clone)]
pub enum WebDavAuth {
    /// Basic 认证
    Basic { username: String, password: String },
    /// Bearer Token
    Bearer { token: String },
}

/// WebDAV Blob Vault 配置
#[derive(Debug, Clone)]
pub struct WebDavConfig {
    /// 服务器地址（末尾不带斜杠）
    pub endpoint: String,
    /// 认证信息
    pub auth: WebDavAuth,
    /// 同步文件存放的路径前缀（末尾不带斜杠），默认 "netcatty-vault"
    pub vault_path: String,
}

impl WebDavConfig {
    /// 构建完整的文件 URL
    fn build_url(&self, key: &str) -> String {
        format!(
            "{}/{}/{}",
            self.endpoint.trim_end_matches('/'),
            self.vault_path,
            key
        )
    }

    /// 构建 auth header
    fn auth_header(&self) -> String {
        match &self.auth {
            WebDavAuth::Basic { username, password } => {
                use base64::Engine;
                let credentials =
                    base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
                format!("Basic {credentials}")
            }
            WebDavAuth::Bearer { token } => format!("Bearer {token}"),
        }
    }
}

/// WebDAV Blob Vault
pub struct WebDavVault {
    config: WebDavConfig,
    http: Arc<dyn HttpClient>,
}

impl WebDavVault {
    pub fn new(config: WebDavConfig, http: Arc<dyn HttpClient>) -> Self {
        Self { config, http }
    }

    /// 发送请求并读取响应体
    async fn send_and_read(&self, req: Request<AsyncBody>) -> Result<(StatusCode, Vec<u8>), CloudApiError> {
        let response = self
            .http
            .send(req)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;
        let status = response.status();
        let mut body = response.into_body();
        let mut bytes = Vec::new();
        body.read_to_end(&mut bytes)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;
        Ok((status, bytes))
    }
}

#[async_trait::async_trait]
impl BlobVault for WebDavVault {
    fn backend_type(&self) -> &'static str {
        "webdav"
    }

    async fn upload(&self, key: &str, data: Vec<u8>) -> Result<BlobMeta, CloudApiError> {
        let url = self.config.build_url(key);
        let now = chrono::Utc::now().timestamp_millis();
        let size = data.len() as u64;

        let req = Request::builder()
            .method("PUT")
            .uri(&url)
            .header("Authorization", self.config.auth_header())
            .header("Content-Type", "application/octet-stream")
            .body(AsyncBody::from(data))
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let (status, _) = self.send_and_read(req).await?;

        if status.is_success() || status == StatusCode::CREATED {
            Ok(BlobMeta { key: key.to_string(), size, updated_at: now })
        } else {
            Err(CloudApiError::ServerError(format!(
                "WebDAV 上传失败: HTTP {}",
                status.as_u16()
            )))
        }
    }

    async fn download(&self, key: &str) -> Result<Blob, CloudApiError> {
        let url = self.config.build_url(key);
        let now = chrono::Utc::now().timestamp_millis();

        let req = Request::builder()
            .method(Method::GET)
            .uri(&url)
            .header("Authorization", self.config.auth_header())
            .body(AsyncBody::empty())
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let (status, bytes) = self.send_and_read(req).await?;

        if status == StatusCode::NOT_FOUND {
            return Err(CloudApiError::NotFound(key.to_string()));
        }
        if !status.is_success() {
            return Err(CloudApiError::ServerError(format!(
                "WebDAV 下载失败: HTTP {}",
                status.as_u16()
            )));
        }

        Ok(Blob { key: key.to_string(), data: bytes, updated_at: now })
    }

    async fn delete(&self, key: &str) -> Result<(), CloudApiError> {
        let url = self.config.build_url(key);

        let req = Request::builder()
            .method(Method::DELETE)
            .uri(&url)
            .header("Authorization", self.config.auth_header())
            .body(AsyncBody::empty())
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let (status, _) = self.send_and_read(req).await?;

        if status.is_success() || status == StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(CloudApiError::ServerError(format!(
                "WebDAV 删除失败: HTTP {}",
                status.as_u16()
            )))
        }
    }

    async fn exists(&self, key: &str) -> Result<bool, CloudApiError> {
        let url = self.config.build_url(key);

        let req = Request::builder()
            .method(Method::HEAD)
            .uri(&url)
            .header("Authorization", self.config.auth_header())
            .body(AsyncBody::empty())
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let (status, _) = self.send_and_read(req).await?;
        Ok(status.is_success())
    }

    async fn list(&self, prefix: Option<&str>) -> Result<Vec<BlobMeta>, CloudApiError> {
        let propfind_url = format!(
            "{}/{}",
            self.config.endpoint.trim_end_matches('/'),
            self.config.vault_path.trim_start_matches('/')
        );
        let search_prefix = prefix.unwrap_or("");
        let depth = if search_prefix.is_empty() { "1" } else { "infinity" };

        let body = r#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:"><d:allprop/></d:propfind>"#;

        let req = Request::builder()
            .method("PROPFIND")
            .uri(&propfind_url)
            .header("Authorization", self.config.auth_header())
            .header("Depth", depth)
            .header("Content-Type", "application/xml")
            .body(AsyncBody::from(body.as_bytes().to_vec()))
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let (status, bytes) = self.send_and_read(req).await?;

        if !status.is_success() {
            return Err(CloudApiError::ServerError(format!(
                "WebDAV 列举失败: HTTP {}",
                status.as_u16()
            )));
        }

        let text = String::from_utf8_lossy(&bytes);
        let vault_prefix = format!("/{}", self.config.vault_path.trim_matches('/'));

        let mut results = Vec::new();
        for line in text.lines() {
            if let Some(start) = line.find("<d:href>") {
                if let Some(end) = line[start + 9..].find("</d:href>") {
                    let href = &line[start + 9..start + 9 + end];
                    if href.starts_with(&vault_prefix) && href.contains(search_prefix) {
                        results.push(BlobMeta { key: href.to_string(), size: 0, updated_at: 0 });
                    }
                }
            }
        }
        Ok(results)
    }
}
