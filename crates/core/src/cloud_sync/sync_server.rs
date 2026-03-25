//! sync_server 云端 API 客户端实现
//!
//! 面向独立部署的 `sync_server` REST 接口，
//! 提供账号密码登录、用户配置同步和同步项读写能力。

use crate::cloud_sync::client::*;
use crate::cloud_sync::models::*;
use crate::llm::ChatStream;
use async_trait::async_trait;
use futures::AsyncReadExt;
use gpui::http_client::{AsyncBody, HttpClient, Method, Request, StatusCode};
use llm_connector::ChatRequest;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex as AsyncMutex;
use tracing::{debug, error, info, warn};
use url::Url;

/// sync_server 客户端配置
#[derive(Debug, Clone)]
pub struct SyncServerConfig {
    /// 服务根地址，例如 http://127.0.0.1:8787
    pub base_url: String,
}

#[derive(Debug, Clone)]
struct AuthState {
    access_token: Option<String>,
    refresh_token: Option<String>,
    user_id: Option<String>,
    expires_at: i64,
}

#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    data: T,
}

#[derive(Debug, Deserialize)]
struct ApiErrorEnvelope {
    error: ApiErrorBody,
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncServerPublicUser {
    id: String,
    email: String,
    nickname: Option<String>,
    role: String,
    status: String,
    created_at: String,
    updated_at: String,
    last_login_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncServerSessionPayload {
    token: String,
    refresh_token: String,
    expires_at: String,
    user: SyncServerPublicUser,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncServerConfigPayload {
    key_verification: String,
    key_version: u32,
    updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncServerSyncItemPayload {
    id: String,
    owner_id: String,
    data_type: String,
    encrypted_data: String,
    key_version: u32,
    checksum: String,
    version: u32,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PasswordAuthRequest<'a> {
    email: &'a str,
    password: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RefreshRequest<'a> {
    refresh_token: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveConfigRequest<'a> {
    key_verification: &'a str,
    key_version: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateSyncItemRequest<'a> {
    id: Option<&'a str>,
    data_type: &'a str,
    encrypted_data: &'a str,
    key_version: u32,
    checksum: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateSyncItemRequest<'a> {
    encrypted_data: &'a str,
    key_version: u32,
    checksum: &'a str,
    version: u32,
    deleted_at: Option<String>,
}

/// sync_server 客户端
pub struct SyncServerClient {
    base_url: RwLock<String>,
    http: Arc<dyn HttpClient>,
    auth_state: RwLock<AuthState>,
    refresh_lock: AsyncMutex<()>,
    on_session_expired: RwLock<Option<SessionExpiredCallback>>,
    on_token_refreshed: RwLock<Option<TokenRefreshedCallback>>,
}

impl SyncServerClient {
    /// 创建新的 sync_server 客户端
    pub fn new(config: SyncServerConfig, http: Arc<dyn HttpClient>) -> Self {
        Self {
            base_url: RwLock::new(Self::normalize_base_url(&config.base_url)),
            http,
            auth_state: RwLock::new(AuthState {
                access_token: None,
                refresh_token: None,
                user_id: None,
                expires_at: 0,
            }),
            refresh_lock: AsyncMutex::new(()),
            on_session_expired: RwLock::new(None),
            on_token_refreshed: RwLock::new(None),
        }
    }

    /// 规范化 sync_server 根地址。
    pub fn normalize_base_url(value: &str) -> String {
        value.trim().trim_end_matches('/').to_string()
    }

    /// 检查 sync_server 根地址是否为有效的 HTTP/HTTPS URL。
    pub fn is_valid_base_url(value: &str) -> bool {
        let normalized = Self::normalize_base_url(value);
        if normalized.is_empty() {
            return false;
        }

        match Url::parse(&normalized) {
            Ok(url) => matches!(url.scheme(), "http" | "https") && url.has_host(),
            Err(_) => false,
        }
    }

    /// 返回当前根地址。
    pub fn base_url(&self) -> String {
        self.base_url
            .read()
            .map(|value| value.clone())
            .unwrap_or_default()
    }

    /// 检查当前根地址是否有效。
    pub fn has_valid_base_url(&self) -> bool {
        Self::is_valid_base_url(&self.base_url())
    }

    /// 更新当前根地址，返回值表示是否发生变化。
    pub fn set_base_url(&self, value: impl AsRef<str>) -> bool {
        let normalized = Self::normalize_base_url(value.as_ref());
        if let Ok(mut base_url) = self.base_url.write() {
            if *base_url == normalized {
                return false;
            }
            *base_url = normalized;
            return true;
        }

        false
    }

    /// 设置会话过期回调
    pub fn set_session_expired_callback(&self, callback: SessionExpiredCallback) {
        if let Ok(mut cb) = self.on_session_expired.write() {
            *cb = Some(callback);
        }
    }

    /// 设置自动刷新成功回调
    pub fn set_token_refreshed_callback(&self, callback: TokenRefreshedCallback) {
        if let Ok(mut cb) = self.on_token_refreshed.write() {
            *cb = Some(callback);
        }
    }

    /// 设置认证状态（包含过期时间）
    pub fn set_auth_with_expiry(
        &self,
        access_token: String,
        refresh_token: String,
        user_id: String,
        expires_at: i64,
    ) {
        if let Ok(mut state) = self.auth_state.write() {
            state.access_token = Some(access_token);
            state.refresh_token = Some(refresh_token);
            state.user_id = Some(user_id);
            state.expires_at = expires_at;
        }
    }

    /// 清除认证状态
    pub fn clear_auth(&self) {
        if let Ok(mut state) = self.auth_state.write() {
            state.access_token = None;
            state.refresh_token = None;
            state.user_id = None;
            state.expires_at = 0;
        }
    }

    fn notify_session_expired(&self) {
        if let Ok(cb) = self.on_session_expired.read() {
            if let Some(callback) = cb.as_ref() {
                callback();
            }
        }
    }

    fn notify_token_refreshed(&self, auth: &AuthResponse) {
        if let Ok(cb) = self.on_token_refreshed.read() {
            if let Some(callback) = cb.as_ref() {
                callback(auth.clone());
            }
        }
    }

    fn get_access_token(&self) -> Option<String> {
        self.auth_state
            .read()
            .ok()
            .and_then(|state| state.access_token.clone())
    }

    fn get_refresh_token(&self) -> Option<String> {
        self.auth_state
            .read()
            .ok()
            .and_then(|state| state.refresh_token.clone())
    }

    fn get_user_id(&self) -> Option<String> {
        self.auth_state
            .read()
            .ok()
            .and_then(|state| state.user_id.clone())
    }

    fn api_url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base_url().trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    fn common_headers(&self) -> Vec<(&'static str, String)> {
        vec![("Content-Type", "application/json".to_string())]
    }

    fn auth_headers(&self) -> Result<Vec<(&'static str, String)>, CloudApiError> {
        let token = self
            .get_access_token()
            .ok_or(CloudApiError::NotAuthenticated)?;

        let mut headers = self.common_headers();
        headers.push(("Authorization", format!("Bearer {}", token)));
        Ok(headers)
    }

    fn build_request(
        &self,
        method: Method,
        url: &str,
        headers: Vec<(&'static str, String)>,
        body: Option<Vec<u8>>,
    ) -> Result<Request<AsyncBody>, CloudApiError> {
        let mut builder = Request::builder().method(method).uri(url);

        for (key, value) in headers {
            builder = builder.header(key, value);
        }

        let body = body.map(AsyncBody::from).unwrap_or_else(AsyncBody::empty);
        builder
            .body(body)
            .map_err(|error| CloudApiError::NetworkError(error.to_string()))
    }

    async fn send_request(
        &self,
        request: Request<AsyncBody>,
    ) -> Result<(StatusCode, Vec<u8>), CloudApiError> {
        let method = request.method().clone();
        let uri = request.uri().to_string();

        debug!("[sync_server] request start: {} {}", method, uri);

        let response = self.http.send(request).await.map_err(|error| {
            error!(
                "[sync_server] request failed: {} {} - {}",
                method, uri, error
            );
            CloudApiError::NetworkError(error.to_string())
        })?;

        let status = response.status();
        let mut body = response.into_body();
        let mut bytes = Vec::new();
        body.read_to_end(&mut bytes)
            .await
            .map_err(|error| CloudApiError::NetworkError(format!("读取响应失败: {}", error)))?;

        debug!(
            "[sync_server] request done: {} {} -> {}",
            method, uri, status
        );
        Ok((status, bytes))
    }

    async fn send_json<T: serde::de::DeserializeOwned>(
        &self,
        method: Method,
        url: &str,
        headers: Vec<(&'static str, String)>,
        body: Option<Vec<u8>>,
    ) -> Result<(StatusCode, Result<T, String>), CloudApiError> {
        let request = self.build_request(method, url, headers, body)?;
        let (status, response_bytes) = self.send_request(request).await?;
        let result = serde_json::from_slice(&response_bytes)
            .map_err(|_| String::from_utf8_lossy(&response_bytes).to_string());
        Ok((status, result))
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        headers: Vec<(&'static str, String)>,
    ) -> Result<(StatusCode, Result<T, String>), CloudApiError> {
        self.send_json(Method::GET, url, headers, None).await
    }

    async fn post_json<T: serde::de::DeserializeOwned, B: serde::Serialize>(
        &self,
        url: &str,
        headers: Vec<(&'static str, String)>,
        body: &B,
    ) -> Result<(StatusCode, Result<T, String>), CloudApiError> {
        let body_bytes = serde_json::to_vec(body)
            .map_err(|error| CloudApiError::ParseError(error.to_string()))?;
        self.send_json(Method::POST, url, headers, Some(body_bytes))
            .await
    }

    async fn put_json<T: serde::de::DeserializeOwned, B: serde::Serialize>(
        &self,
        url: &str,
        headers: Vec<(&'static str, String)>,
        body: &B,
    ) -> Result<(StatusCode, Result<T, String>), CloudApiError> {
        let body_bytes = serde_json::to_vec(body)
            .map_err(|error| CloudApiError::ParseError(error.to_string()))?;
        self.send_json(Method::PUT, url, headers, Some(body_bytes))
            .await
    }

    async fn delete_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        headers: Vec<(&'static str, String)>,
    ) -> Result<(StatusCode, Result<T, String>), CloudApiError> {
        self.send_json(Method::DELETE, url, headers, None).await
    }

    async fn get_json_with_retry<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<(StatusCode, Result<T, String>), CloudApiError> {
        if self.is_token_expiring_soon() {
            self.ensure_token_valid().await?;
        }

        let headers = self.auth_headers()?;
        let (status, result) = self.get_json(url, headers).await?;
        if status == StatusCode::UNAUTHORIZED {
            self.refresh_current_session().await?;
            let retry_headers = self.auth_headers()?;
            let (retry_status, retry_result) = self.get_json(url, retry_headers).await?;
            if retry_status == StatusCode::UNAUTHORIZED {
                self.clear_auth();
                self.notify_session_expired();
                return Err(CloudApiError::NotAuthenticated);
            }
            return Ok((retry_status, retry_result));
        }

        Ok((status, result))
    }

    async fn post_json_with_retry<T: serde::de::DeserializeOwned, B: serde::Serialize>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<(StatusCode, Result<T, String>), CloudApiError> {
        if self.is_token_expiring_soon() {
            self.ensure_token_valid().await?;
        }

        let headers = self.auth_headers()?;
        let (status, result) = self.post_json(url, headers, body).await?;
        if status == StatusCode::UNAUTHORIZED {
            self.refresh_current_session().await?;
            let retry_headers = self.auth_headers()?;
            let (retry_status, retry_result) = self.post_json(url, retry_headers, body).await?;
            if retry_status == StatusCode::UNAUTHORIZED {
                self.clear_auth();
                self.notify_session_expired();
                return Err(CloudApiError::NotAuthenticated);
            }
            return Ok((retry_status, retry_result));
        }

        Ok((status, result))
    }

    async fn put_json_with_retry<T: serde::de::DeserializeOwned, B: serde::Serialize>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<(StatusCode, Result<T, String>), CloudApiError> {
        if self.is_token_expiring_soon() {
            self.ensure_token_valid().await?;
        }

        let headers = self.auth_headers()?;
        let (status, result) = self.put_json(url, headers, body).await?;
        if status == StatusCode::UNAUTHORIZED {
            self.refresh_current_session().await?;
            let retry_headers = self.auth_headers()?;
            let (retry_status, retry_result) = self.put_json(url, retry_headers, body).await?;
            if retry_status == StatusCode::UNAUTHORIZED {
                self.clear_auth();
                self.notify_session_expired();
                return Err(CloudApiError::NotAuthenticated);
            }
            return Ok((retry_status, retry_result));
        }

        Ok((status, result))
    }

    async fn delete_json_with_retry<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<(StatusCode, Result<T, String>), CloudApiError> {
        if self.is_token_expiring_soon() {
            self.ensure_token_valid().await?;
        }

        let headers = self.auth_headers()?;
        let (status, result) = self.delete_json(url, headers).await?;
        if status == StatusCode::UNAUTHORIZED {
            self.refresh_current_session().await?;
            let retry_headers = self.auth_headers()?;
            let (retry_status, retry_result) = self.delete_json(url, retry_headers).await?;
            if retry_status == StatusCode::UNAUTHORIZED {
                self.clear_auth();
                self.notify_session_expired();
                return Err(CloudApiError::NotAuthenticated);
            }
            return Ok((retry_status, retry_result));
        }

        Ok((status, result))
    }

    fn is_token_expiring_soon(&self) -> bool {
        if let Ok(state) = self.auth_state.read() {
            if state.expires_at == 0 {
                return false;
            }

            let now = chrono::Utc::now().timestamp();
            return state.expires_at <= now + 60;
        }

        false
    }

    async fn ensure_token_valid(&self) -> Result<(), CloudApiError> {
        if self.is_token_expiring_soon() {
            self.refresh_current_session().await?;
        }

        Ok(())
    }

    async fn refresh_current_session(&self) -> Result<AuthResponse, CloudApiError> {
        let _guard = self.refresh_lock.lock().await;
        let refresh_token = self
            .get_refresh_token()
            .ok_or(CloudApiError::NotAuthenticated)?;

        match self.refresh_with_token(&refresh_token).await {
            Ok(auth) => {
                self.notify_token_refreshed(&auth);
                Ok(auth)
            }
            Err(error) => {
                if error.is_auth_error() {
                    self.clear_auth();
                    self.notify_session_expired();
                }
                Err(error)
            }
        }
    }

    async fn refresh_with_token(&self, refresh_token: &str) -> Result<AuthResponse, CloudApiError> {
        let url = self.api_url("/api/v1/auth/refresh");
        let headers = self.common_headers();
        let body = RefreshRequest { refresh_token };
        let (status, result) = self
            .post_json::<ApiEnvelope<SyncServerSessionPayload>, _>(&url, headers, &body)
            .await?;

        if status.is_success() {
            let payload = result.map_err(CloudApiError::ParseError)?.data;
            let auth = self.map_auth_response(payload);
            self.set_auth_with_expiry(
                auth.access_token.clone(),
                auth.refresh_token.clone(),
                auth.user_id.clone(),
                auth.expires_at,
            );
            return Ok(auth);
        }

        Err(CloudApiError::AuthenticationFailed(
            Self::extract_error_message(result.err(), "刷新会话失败"),
        ))
    }

    fn map_auth_response(&self, payload: SyncServerSessionPayload) -> AuthResponse {
        let expires_at = parse_rfc3339_to_seconds(&payload.expires_at);
        let SyncServerSessionPayload {
            token,
            refresh_token,
            user,
            ..
        } = payload;

        let _ = (
            &user.role,
            &user.status,
            &user.updated_at,
            &user.last_login_at,
        );

        AuthResponse {
            user_id: user.id,
            email: user.email,
            access_token: token,
            refresh_token,
            expires_at,
        }
    }

    fn map_user_info(user: SyncServerPublicUser) -> UserInfo {
        let _ = (
            &user.role,
            &user.status,
            &user.updated_at,
            &user.last_login_at,
        );
        UserInfo {
            id: user.id,
            email: user.email,
            nickname: user.nickname.filter(|nickname| !nickname.trim().is_empty()),
            username: None,
            avatar_url: None,
            created_at: parse_rfc3339_to_seconds(&user.created_at),
        }
    }

    fn map_user_config(payload: SyncServerConfigPayload, user_id: String) -> CloudUserConfig {
        CloudUserConfig {
            user_id,
            key_verification: payload.key_verification,
            key_version: payload.key_version,
            updated_at: parse_rfc3339_to_millis(&payload.updated_at),
        }
    }

    fn map_sync_item(payload: SyncServerSyncItemPayload) -> CloudSyncData {
        let SyncServerSyncItemPayload {
            id,
            owner_id,
            data_type,
            encrypted_data,
            key_version,
            checksum,
            version,
            created_at: _created_at,
            updated_at,
            deleted_at,
        } = payload;

        CloudSyncData {
            id,
            owner_id,
            team_id: None,
            data_type,
            encrypted_data,
            key_version,
            checksum,
            version,
            updated_at: parse_rfc3339_to_millis(&updated_at),
            deleted_at: deleted_at.as_deref().map(parse_rfc3339_to_millis),
        }
    }

    fn unsupported(message: &str) -> CloudApiError {
        CloudApiError::ServerError(message.to_string())
    }

    fn extract_error_message(raw: Option<String>, default: &str) -> String {
        let Some(raw) = raw else {
            return default.to_string();
        };

        if let Ok(payload) = serde_json::from_str::<ApiErrorEnvelope>(&raw) {
            return payload.error.message;
        }

        if raw.trim().is_empty() {
            default.to_string()
        } else {
            raw
        }
    }
}

#[async_trait]
impl CloudApiClient for SyncServerClient {
    async fn sign_in_with_password(
        &self,
        email: &str,
        password: &str,
    ) -> Result<AuthResponse, CloudApiError> {
        let url = self.api_url("/api/v1/auth/login");
        let headers = self.common_headers();
        let body = PasswordAuthRequest { email, password };
        let (status, result) = self
            .post_json::<ApiEnvelope<SyncServerSessionPayload>, _>(&url, headers, &body)
            .await?;

        if status.is_success() {
            let payload = result.map_err(CloudApiError::ParseError)?.data;
            let auth = self.map_auth_response(payload);
            self.set_auth_with_expiry(
                auth.access_token.clone(),
                auth.refresh_token.clone(),
                auth.user_id.clone(),
                auth.expires_at,
            );
            info!("[sync_server] 密码登录成功: user_id={}", auth.user_id);
            return Ok(auth);
        }

        Err(CloudApiError::AuthenticationFailed(
            Self::extract_error_message(result.err(), "登录失败"),
        ))
    }

    async fn sign_in_with_oauth(
        &self,
        _provider: &str,
        _redirect_url: &str,
    ) -> Result<OAuthResponse, CloudApiError> {
        Err(Self::unsupported("当前 sync_server 不支持 OAuth 登录"))
    }

    async fn sign_up(&self, email: &str, password: &str) -> Result<AuthResponse, CloudApiError> {
        let url = self.api_url("/api/v1/auth/register");
        let headers = self.common_headers();
        let body = PasswordAuthRequest { email, password };
        let (status, result) = self
            .post_json::<ApiEnvelope<SyncServerSessionPayload>, _>(&url, headers, &body)
            .await?;

        if status.is_success() {
            let payload = result.map_err(CloudApiError::ParseError)?.data;
            let auth = self.map_auth_response(payload);
            self.set_auth_with_expiry(
                auth.access_token.clone(),
                auth.refresh_token.clone(),
                auth.user_id.clone(),
                auth.expires_at,
            );
            info!("[sync_server] 注册并登录成功: user_id={}", auth.user_id);
            return Ok(auth);
        }

        Err(CloudApiError::AuthenticationFailed(
            Self::extract_error_message(result.err(), "注册失败"),
        ))
    }

    async fn sign_out(&self) -> Result<(), CloudApiError> {
        let Some(_) = self.get_access_token() else {
            self.clear_auth();
            return Ok(());
        };

        let url = self.api_url("/api/v1/auth/logout");
        let result = self
            .post_json_with_retry::<ApiEnvelope<serde_json::Value>, _>(&url, &serde_json::json!({}))
            .await;

        self.clear_auth();
        result.map(|_| ())
    }

    async fn get_current_user(&self) -> Result<Option<UserInfo>, CloudApiError> {
        if self.get_access_token().is_none() {
            return Ok(None);
        }

        let url = self.api_url("/api/v1/auth/me");
        match self
            .get_json_with_retry::<ApiEnvelope<SyncServerPublicUser>>(&url)
            .await
        {
            Ok((status, result)) if status.is_success() => {
                let payload = result.map_err(CloudApiError::ParseError)?.data;
                Ok(Some(Self::map_user_info(payload)))
            }
            Ok((_status, result)) => Err(CloudApiError::ServerError(Self::extract_error_message(
                result.err(),
                "获取当前用户失败",
            ))),
            Err(CloudApiError::NotAuthenticated) => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn refresh_token(&self, refresh_token: &str) -> Result<AuthResponse, CloudApiError> {
        let auth = self.refresh_with_token(refresh_token).await?;
        self.notify_token_refreshed(&auth);
        Ok(auth)
    }

    async fn get_user_config(&self) -> Result<Option<CloudUserConfig>, CloudApiError> {
        let url = self.api_url("/api/v1/sync/config");
        let (status, result) = self
            .get_json_with_retry::<ApiEnvelope<Option<SyncServerConfigPayload>>>(&url)
            .await?;

        if status.is_success() {
            let payload = result.map_err(CloudApiError::ParseError)?.data;
            let user_id = self.get_user_id().unwrap_or_default();
            return Ok(payload.map(|config| Self::map_user_config(config, user_id)));
        }

        Err(CloudApiError::ServerError(Self::extract_error_message(
            result.err(),
            "获取用户配置失败",
        )))
    }

    async fn save_user_config(&self, config: &CloudUserConfig) -> Result<(), CloudApiError> {
        let url = self.api_url("/api/v1/sync/config");
        let body = SaveConfigRequest {
            key_verification: &config.key_verification,
            key_version: config.key_version,
        };
        let (status, result) = self
            .put_json_with_retry::<ApiEnvelope<SyncServerConfigPayload>, _>(&url, &body)
            .await?;

        if status.is_success() {
            return Ok(());
        }

        Err(CloudApiError::ServerError(Self::extract_error_message(
            result.err(),
            "保存用户配置失败",
        )))
    }

    async fn list_models(&self) -> Result<Vec<String>, CloudApiError> {
        Ok(Vec::new())
    }

    async fn list_sync_data(
        &self,
        data_type: Option<&str>,
        team_id: Option<&str>,
        since: Option<i64>,
    ) -> Result<Vec<CloudSyncData>, CloudApiError> {
        if let Some(team_id) = team_id {
            warn!(
                "[sync_server] 当前后端不支持团队同步过滤，忽略 team_id={}",
                team_id
            );
            return Ok(Vec::new());
        }

        let mut url = self.api_url("/api/v1/sync/items");
        let mut params = Vec::new();
        if let Some(data_type) = data_type {
            params.push(format!("dataType={}", data_type));
        }
        if let Some(since) = since {
            params.push(format!("since={}", since));
        }
        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }

        let (status, result) = self
            .get_json_with_retry::<ApiEnvelope<Vec<SyncServerSyncItemPayload>>>(&url)
            .await?;

        if status.is_success() {
            let items = result.map_err(CloudApiError::ParseError)?.data;
            return Ok(items.into_iter().map(Self::map_sync_item).collect());
        }

        Err(CloudApiError::ServerError(Self::extract_error_message(
            result.err(),
            "获取同步数据失败",
        )))
    }

    async fn create_sync_data(&self, data: &CloudSyncData) -> Result<CloudSyncData, CloudApiError> {
        if data.team_id.is_some() {
            return Err(Self::unsupported("当前 sync_server 不支持团队同步数据"));
        }

        let url = self.api_url("/api/v1/sync/items");
        let body = CreateSyncItemRequest {
            id: (!data.id.is_empty()).then_some(data.id.as_str()),
            data_type: &data.data_type,
            encrypted_data: &data.encrypted_data,
            key_version: data.key_version,
            checksum: &data.checksum,
        };
        let (status, result) = self
            .post_json_with_retry::<ApiEnvelope<SyncServerSyncItemPayload>, _>(&url, &body)
            .await?;

        if status.is_success() || status == StatusCode::CREATED {
            let payload = result.map_err(CloudApiError::ParseError)?.data;
            return Ok(Self::map_sync_item(payload));
        }

        Err(CloudApiError::ServerError(Self::extract_error_message(
            result.err(),
            "创建同步数据失败",
        )))
    }

    async fn update_sync_data(&self, data: &CloudSyncData) -> Result<CloudSyncData, CloudApiError> {
        if data.team_id.is_some() {
            return Err(Self::unsupported("当前 sync_server 不支持团队同步数据"));
        }

        let url = self.api_url(&format!("/api/v1/sync/items/{}", data.id));
        let body = UpdateSyncItemRequest {
            encrypted_data: &data.encrypted_data,
            key_version: data.key_version,
            checksum: &data.checksum,
            version: data.version,
            deleted_at: data.deleted_at.map(format_millis_to_rfc3339),
        };
        let (status, result) = self
            .put_json_with_retry::<ApiEnvelope<SyncServerSyncItemPayload>, _>(&url, &body)
            .await?;

        if status.is_success() {
            let payload = result.map_err(CloudApiError::ParseError)?.data;
            return Ok(Self::map_sync_item(payload));
        }

        if status == StatusCode::CONFLICT {
            return Err(CloudApiError::Conflict(Self::extract_error_message(
                result.err(),
                "同步数据版本冲突",
            )));
        }

        Err(CloudApiError::ServerError(Self::extract_error_message(
            result.err(),
            "更新同步数据失败",
        )))
    }

    async fn delete_sync_data(&self, id: &str) -> Result<(), CloudApiError> {
        let url = self.api_url(&format!("/api/v1/sync/items/{}", id));
        let (status, result) = self
            .delete_json_with_retry::<ApiEnvelope<serde_json::Value>>(&url)
            .await?;

        if status.is_success() {
            return Ok(());
        }

        if status == StatusCode::CONFLICT {
            return Err(CloudApiError::Conflict(Self::extract_error_message(
                result.err(),
                "删除同步数据失败",
            )));
        }

        Err(CloudApiError::ServerError(Self::extract_error_message(
            result.err(),
            "删除同步数据失败",
        )))
    }

    async fn list_teams(&self) -> Result<Vec<Team>, CloudApiError> {
        Ok(Vec::new())
    }

    async fn create_team(&self, _team: &Team) -> Result<Team, CloudApiError> {
        Err(Self::unsupported("当前 sync_server 不支持团队功能"))
    }

    async fn update_team(&self, _team: &Team) -> Result<Team, CloudApiError> {
        Err(Self::unsupported("当前 sync_server 不支持团队功能"))
    }

    async fn delete_team(&self, _id: &str) -> Result<(), CloudApiError> {
        Err(Self::unsupported("当前 sync_server 不支持团队功能"))
    }

    async fn list_team_members(&self, _team_id: &str) -> Result<Vec<TeamMember>, CloudApiError> {
        Ok(Vec::new())
    }

    async fn add_team_member(&self, _member: &TeamMember) -> Result<TeamMember, CloudApiError> {
        Err(Self::unsupported("当前 sync_server 不支持团队功能"))
    }

    async fn add_team_member_by_email(
        &self,
        _team_id: &str,
        _email: &str,
    ) -> Result<TeamMember, CloudApiError> {
        Err(Self::unsupported("当前 sync_server 不支持团队功能"))
    }

    async fn remove_team_member(&self, _member_id: &str) -> Result<(), CloudApiError> {
        Err(Self::unsupported("当前 sync_server 不支持团队功能"))
    }

    async fn chat(&self, _request: &ChatRequest) -> Result<String, CloudApiError> {
        Err(Self::unsupported("当前 sync_server 不支持云端 AI 聊天"))
    }

    async fn chat_stream(&self, _request: &ChatRequest) -> Result<ChatStream, CloudApiError> {
        Err(Self::unsupported("当前 sync_server 不支持云端 AI 聊天"))
    }
}

fn parse_rfc3339_to_seconds(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.timestamp())
        .unwrap_or(0)
}

fn parse_rfc3339_to_millis(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0)
}

fn format_millis_to_rfc3339(value: i64) -> String {
    chrono::DateTime::from_timestamp_millis(value)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339())
}

#[cfg(test)]
mod tests {
    use super::SyncServerClient;

    #[test]
    fn normalize_base_url_trims_spaces_and_trailing_slashes() {
        assert_eq!(
            SyncServerClient::normalize_base_url(" https://example.com/api/ "),
            "https://example.com/api"
        );
    }

    #[test]
    fn is_valid_base_url_requires_http_or_https() {
        assert!(SyncServerClient::is_valid_base_url("http://127.0.0.1:8787"));
        assert!(SyncServerClient::is_valid_base_url(
            "https://example.com/api"
        ));
        assert!(!SyncServerClient::is_valid_base_url(""));
        assert!(!SyncServerClient::is_valid_base_url("127.0.0.1:8787"));
        assert!(!SyncServerClient::is_valid_base_url("ftp://example.com"));
    }
}
