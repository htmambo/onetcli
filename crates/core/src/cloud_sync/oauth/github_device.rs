//! GitHub OAuth Device Flow
//!
//! 实现 GitHub OAuth Device Authorization Grant (RFC 8628)
//!
//! ## 流程
//! 1. 请求 /device/code → 获取 device_code + user_code + verification_uri
//! 2. 打开浏览器到 verification_uri，让用户输入 user_code
//! 3. 轮询 /oauth/access_token?grant_type=urn:ietf:params:oauth:grant-type:device_code
//! 4. 直至获取 access_token 或错误

use crate::cloud_sync::client::CloudApiError;
use crate::cloud_sync::oauth::OAuthTokens;
use futures::AsyncReadExt;
use gpui::http_client::{AsyncBody, HttpClient, Method, Request, StatusCode};
use serde::{Deserialize, Serialize};
use smol::Timer;
use std::collections::HashMap;
use std::sync::Arc;

/// GitHub Device Flow 返回结果
pub struct DeviceFlowResponse {
    pub verification_uri: String,
    pub user_code: String,
    pub device_code: String,
}

/// GitHub Device Flow 错误响应
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct GithubDeviceError {
    error: String,
    error_description: Option<String>,
}

/// GitHub Device Flow code 响应
#[derive(Debug, Deserialize)]
struct GithubDeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    interval: u64,
    expires_in: u64,
}

/// GitHub Token 响应
#[derive(Debug, Deserialize)]
struct GithubTokenResponse {
    access_token: String,
    token_type: String,
    scope: String,
    #[serde(default)]
    refresh_token: Option<String>,
}

impl GithubTokenResponse {
    fn into_tokens(self) -> OAuthTokens {
        OAuthTokens {
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            token_type: self.token_type,
            expires_in: None,
            expires_at: None,
        }
    }
}

/// GitHub OAuth Device Flow 客户端
pub struct GithubOAuthClient {
    http: Arc<dyn HttpClient>,
    client_id: String,
}

impl GithubOAuthClient {
    pub fn new(http: Arc<dyn HttpClient>, client_id: String) -> Self {
        Self { http, client_id }
    }

    /// 开始 Device Flow，返回验证 URI、user_code 和 device_code
    pub async fn start_device_flow(&self) -> Result<DeviceFlowResponse, CloudApiError> {
        let body = format!("client_id={}&scope=gist", self.client_id);

        let req = Request::builder()
            .method(Method::POST)
            .uri("https://github.com/login/device/code")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "application/json")
            .body(AsyncBody::from(body.into_bytes()))
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        let response = self
            .http
            .send(req)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        if response.status() != StatusCode::OK {
            return Err(CloudApiError::ServerError(format!(
                "GitHub device code 请求失败: HTTP {}",
                response.status().as_u16()
            )));
        }

        let _status = response.status();
        let mut body_bytes = Vec::new();
        response
            .into_body()
            .read_to_end(&mut body_bytes)
            .await
            .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

        // 先尝试解析成功响应
        if let Ok(code_resp) = serde_json::from_slice::<GithubDeviceCodeResponse>(&body_bytes) {
            return Ok(DeviceFlowResponse {
                verification_uri: code_resp.verification_uri,
                user_code: code_resp.user_code,
                device_code: code_resp.device_code,
            });
        }

        // 再尝试解析错误
        if let Ok(err_resp) = serde_json::from_slice::<GithubDeviceError>(&body_bytes) {
            return Err(CloudApiError::ServerError(format!(
                "GitHub device code 错误: {} - {}",
                err_resp.error,
                err_resp.error_description.unwrap_or_default()
            )));
        }

        Err(CloudApiError::ParseError(format!(
            "无法解析 GitHub device code 响应: {}",
            String::from_utf8_lossy(&body_bytes)
        )))
    }

    /// 轮询获取 access_token
    ///
    /// `interval` 为轮询间隔（秒），`max_attempts` 为最大轮询次数。
    /// 返回 `Ok(OAuthTokens)` 表示成功，返回 `Err(CloudApiError::AuthenticationFailed)` 表示用户拒绝。
    pub async fn poll_for_token(
        &self,
        device_code: &str,
        interval_secs: u64,
        max_attempts: u32,
    ) -> Result<OAuthTokens, CloudApiError> {
        let body = format!(
            "client_id={}&device_code={}&grant_type=urn:ietf:params:oauth:grant-type:device_code",
            self.client_id, device_code
        );

        for _ in 0..max_attempts {
            Timer::after(std::time::Duration::from_secs(interval_secs)).await;

            let req = Request::builder()
                .method(Method::POST)
                .uri("https://github.com/login/oauth/access_token")
                .header("Content-Type", "application/x-www-form-urlencoded")
                .header("Accept", "application/json")
                .body(AsyncBody::from(body.as_bytes().to_vec()))
                .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

            let response = self
                .http
                .send(req)
                .await
                .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

            let _status = response.status();
            let mut body_bytes = Vec::new();
            response
                .into_body()
                .read_to_end(&mut body_bytes)
                .await
                .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

            // 解析 token 响应
            if let Ok(token_resp) = serde_json::from_slice::<GithubTokenResponse>(&body_bytes) {
                return Ok(token_resp.into_tokens());
            }

            // 解析错误
            if let Ok(err_resp) = serde_json::from_slice::<GithubDeviceError>(&body_bytes) {
                match err_resp.error.as_str() {
                    "authorization_pending" => continue,
                    "slow_down" => {
                        // 增加间隔
                    }
                    "access_denied" => {
                        return Err(CloudApiError::AuthenticationFailed(
                            "用户拒绝了授权请求".to_string(),
                        ));
                    }
                    "expired_token" => {
                        return Err(CloudApiError::AuthenticationFailed(
                            "device_code 已过期，请重新发起授权".to_string(),
                        ));
                    }
                    _ => {
                        return Err(CloudApiError::ServerError(format!(
                            "GitHub token 错误: {} - {}",
                            err_resp.error,
                            err_resp.error_description.unwrap_or_default()
                        )));
                    }
                }
            }

            return Err(CloudApiError::ParseError(format!(
                "无法解析 GitHub token 响应: {}",
                String::from_utf8_lossy(&body_bytes)
            )));
        }

        Err(CloudApiError::AuthenticationFailed(
            "授权超时，请重试".to_string(),
        ))
    }
}

// === Gist helper types ===

#[derive(Debug, Serialize)]
struct GistFile {
    filename: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct CreateGistRequest {
    description: String,
    #[serde(rename = "public")]
    is_public: bool,
    files: HashMap<String, GistFile>,
}

#[derive(Debug, Deserialize)]
struct GistResponse {
    id: String,
    #[serde(default)]
    files: HashMap<String, GistFileResponse>,
}

#[derive(Debug, Deserialize)]
struct GistFileResponse {
    content: Option<String>,
    filename: String,
}

/// 查找用户的 ONetCli-vault gist
pub async fn find_vault_gist(
    http: Arc<dyn HttpClient>,
    tokens: &OAuthTokens,
) -> Result<Option<String>, CloudApiError> {
    let req = Request::builder()
        .method(Method::GET)
        .uri("https://api.github.com/gists")
        .header("Authorization", format!("Bearer {}", tokens.access_token))
        .header("User-Agent", "onetcli")
        .header("Accept", "application/vnd.github+json")
        .body(AsyncBody::empty())
        .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

    let response = http
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
    response
        .into_body()
        .read_to_end(&mut bytes)
        .await
        .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

    let gists: Vec<GistResponse> =
        serde_json::from_slice(&bytes).map_err(|e| CloudApiError::ParseError(e.to_string()))?;

    for gist in gists {
        if gist.files.contains_key("ONetCli-vault.json") {
            return Ok(Some(gist.id));
        }
    }
    Ok(None)
}

/// 创建 ONetCli-vault gist
pub async fn create_vault_gist(
    http: Arc<dyn HttpClient>,
    tokens: &OAuthTokens,
) -> Result<String, CloudApiError> {
    let mut files = HashMap::new();
    files.insert(
        "ONetCli-vault.json".to_string(),
        GistFile {
            filename: "ONetCli-vault.json".to_string(),
            content: "{}".to_string(),
        },
    );

    let body = serde_json::to_vec(&CreateGistRequest {
        description: "ONetCli sync vault".to_string(),
        is_public: false,
        files,
    })
    .map_err(|e| CloudApiError::DataFormatError(e.to_string()))?;

    let req = Request::builder()
        .method(Method::POST)
        .uri("https://api.github.com/gists")
        .header("Authorization", format!("Bearer {}", tokens.access_token))
        .header("User-Agent", "ONetCli")
        .header("Accept", "application/vnd.github+json")
        .header("Content-Type", "application/json")
        .body(AsyncBody::from(body))
        .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

    let response = http
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
    response
        .into_body()
        .read_to_end(&mut bytes)
        .await
        .map_err(|e| CloudApiError::NetworkError(e.to_string()))?;

    let gist: GistResponse =
        serde_json::from_slice(&bytes).map_err(|e| CloudApiError::ParseError(e.to_string()))?;

    Ok(gist.id)
}
