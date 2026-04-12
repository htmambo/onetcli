//! 云端 API 客户端抽象层
//!
//! 定义云端 API 的通用接口，当前主要对接 `sync_server`。

use crate::cloud_sync::models::*;
use crate::llm::ChatStream;
use async_trait::async_trait;
use llm_connector::ChatRequest;
use std::fmt;
use std::sync::Arc;

use super::blob_vault::BlobVault;

/// 云端 API 错误类型
#[derive(Debug, Clone)]
pub enum CloudApiError {
    /// 未登录
    NotAuthenticated,
    /// 认证失败
    AuthenticationFailed(String),
    /// 需要邮箱确认（注册成功但需验证邮箱）
    EmailConfirmationRequired(String),
    /// 网络错误
    NetworkError(String),
    /// 服务端错误
    ServerError(String),
    /// 数据解析错误
    ParseError(String),
    /// 资源未找到
    NotFound(String),
    /// 冲突
    Conflict(String),
    /// 不支持的操作
    NotSupported(String),
    /// 数据格式错误
    DataFormatError(String),
    /// 未知错误
    Unknown(String),
}

impl fmt::Display for CloudApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CloudApiError::NotAuthenticated => write!(f, "未登录"),
            CloudApiError::AuthenticationFailed(msg) => write!(f, "认证失败: {}", msg),
            CloudApiError::EmailConfirmationRequired(msg) => write!(f, "需要邮箱确认: {}", msg),
            CloudApiError::NetworkError(msg) => write!(f, "网络错误: {}", msg),
            CloudApiError::ServerError(msg) => write!(f, "服务端错误: {}", msg),
            CloudApiError::ParseError(msg) => write!(f, "数据解析错误: {}", msg),
            CloudApiError::NotFound(msg) => write!(f, "资源未找到: {}", msg),
            CloudApiError::Conflict(msg) => write!(f, "冲突: {}", msg),
            CloudApiError::NotSupported(msg) => write!(f, "不支持: {}", msg),
            CloudApiError::DataFormatError(msg) => write!(f, "数据格式错误: {}", msg),
            CloudApiError::Unknown(msg) => write!(f, "未知错误: {}", msg),
        }
    }
}

impl CloudApiError {
    /// 判断是否为认证相关错误（应触发清除认证状态）
    pub fn is_auth_error(&self) -> bool {
        matches!(
            self,
            CloudApiError::NotAuthenticated | CloudApiError::AuthenticationFailed(_)
        )
    }
}

impl std::error::Error for CloudApiError {}

/// 会话过期事件回调类型
pub type SessionExpiredCallback = Arc<dyn Fn() + Send + Sync>;
/// 自动刷新成功回调类型
pub type TokenRefreshedCallback = Arc<dyn Fn(AuthResponse) + Send + Sync>;

/// 云端 API 客户端 trait
///
/// 定义与云端服务交互的通用接口。
#[async_trait]
pub trait CloudApiClient: Send + Sync {
    // ========================================================================
    // 认证相关
    // ========================================================================

    /// 使用邮箱密码登录
    async fn sign_in_with_password(
        &self,
        email: &str,
        password: &str,
    ) -> Result<AuthResponse, CloudApiError>;

    /// 使用 OAuth 登录（如 GitHub、Google）
    async fn sign_in_with_oauth(
        &self,
        provider: &str,
        redirect_url: &str,
    ) -> Result<OAuthResponse, CloudApiError>;

    /// 注册新用户
    async fn sign_up(&self, email: &str, password: &str) -> Result<AuthResponse, CloudApiError>;

    /// 登出
    async fn sign_out(&self) -> Result<(), CloudApiError>;

    /// 获取当前登录用户信息
    async fn get_current_user(&self) -> Result<Option<UserInfo>, CloudApiError>;

    /// 刷新访问令牌
    async fn refresh_token(&self, refresh_token: &str) -> Result<AuthResponse, CloudApiError>;

    // ========================================================================
    // 用户配置相关（密钥验证数据）
    // ========================================================================

    /// 获取用户的加密配置
    async fn get_user_config(&self) -> Result<Option<CloudUserConfig>, CloudApiError>;

    /// 保存用户的加密配置
    async fn save_user_config(&self, config: &CloudUserConfig) -> Result<(), CloudApiError>;

    // ========================================================================
    // OnetCli 模型列表
    // ========================================================================

    /// 获取当前可用模型列表
    async fn list_models(&self) -> Result<Vec<String>, CloudApiError>;

    // ========================================================================
    // 统一同步数据（sync_data 表）
    // ========================================================================

    /// 获取同步数据列表
    ///
    /// 可选过滤：data_type, since_timestamp
    async fn list_sync_data(
        &self,
        data_type: Option<&str>,
        since: Option<i64>,
    ) -> Result<Vec<CloudSyncData>, CloudApiError>;

    /// 创建同步数据
    async fn create_sync_data(&self, data: &CloudSyncData) -> Result<CloudSyncData, CloudApiError>;

    /// 更新同步数据（乐观并发控制）
    async fn update_sync_data(&self, data: &CloudSyncData) -> Result<CloudSyncData, CloudApiError>;

    /// 软删除同步数据
    async fn delete_sync_data(&self, id: &str) -> Result<(), CloudApiError>;

    // ========================================================================
    // 团队管理（暂不支持）
    // ========================================================================

    /// 获取当前用户所在的所有团队（暂不支持）
    async fn list_teams(&self) -> Result<Vec<()>, CloudApiError> {
        Err(CloudApiError::NotSupported("团队功能暂不支持".to_string()))
    }

    /// 创建团队（暂不支持）
    async fn create_team(&self, _team: &str) -> Result<(), CloudApiError> {
        Err(CloudApiError::NotSupported("团队功能暂不支持".to_string()))
    }

    /// 更新团队信息（暂不支持）
    async fn update_team(&self, _team: &str) -> Result<(), CloudApiError> {
        Err(CloudApiError::NotSupported("团队功能暂不支持".to_string()))
    }

    /// 删除团队（暂不支持）
    async fn delete_team(&self, _id: &str) -> Result<(), CloudApiError> {
        Err(CloudApiError::NotSupported("团队功能暂不支持".to_string()))
    }

    /// 获取团队成员列表（暂不支持）
    async fn list_team_members(&self, _team_id: &str) -> Result<Vec<()>, CloudApiError> {
        Err(CloudApiError::NotSupported("团队功能暂不支持".to_string()))
    }

    /// 添加团队成员（暂不支持）
    async fn add_team_member(&self, _member: &str) -> Result<(), CloudApiError> {
        Err(CloudApiError::NotSupported("团队功能暂不支持".to_string()))
    }

    /// 通过邮箱添加团队成员（暂不支持）
    async fn add_team_member_by_email(
        &self,
        _team_id: &str,
        _email: &str,
    ) -> Result<(), CloudApiError> {
        Err(CloudApiError::NotSupported("团队功能暂不支持".to_string()))
    }

    /// 移除团队成员（暂不支持）
    async fn remove_team_member(&self, _member_id: &str) -> Result<(), CloudApiError> {
        Err(CloudApiError::NotSupported("团队功能暂不支持".to_string()))
    }

    // ========================================================================
    // AI 聊天
    // ========================================================================

    /// 聊天
    async fn chat(&self, request: &ChatRequest) -> Result<String, CloudApiError>;

    /// 聊天流
    async fn chat_stream(&self, request: &ChatRequest) -> Result<ChatStream, CloudApiError>;

    // ========================================================================
    // Blob 存储（可选实现）
    // ========================================================================

    /// 将客户端作为 BlobVault 使用
    ///
    /// 返回 `Some(&dyn BlobVault)` 表示该客户端支持直接 blob 存储操作（如 WebDAV、S3）。
    /// 返回 `None` 表示该客户端不支持 blob 操作（如 sync_server REST API）。
    fn as_blob_vault(&self) -> Option<&dyn BlobVault> {
        None
    }
}

/// 认证响应
#[derive(Debug, Clone)]
pub struct AuthResponse {
    /// 用户 ID
    pub user_id: String,
    /// 用户邮箱
    pub email: String,
    /// 访问令牌
    pub access_token: String,
    /// 刷新令牌
    pub refresh_token: String,
    /// 令牌过期时间（Unix 时间戳）
    pub expires_at: i64,
}

/// OAuth 响应
#[derive(Debug, Clone)]
pub struct OAuthResponse {
    /// 授权 URL
    pub auth_url: String,
}

/// 用户信息
#[derive(Debug, Clone)]
pub struct UserInfo {
    /// 用户 ID
    pub id: String,
    /// 用户邮箱
    pub email: String,
    /// 用户昵称（可选）
    pub nickname: Option<String>,
    /// 用户名（可选）
    pub username: Option<String>,
    /// 头像 URL（可选）
    pub avatar_url: Option<String>,
    /// 创建时间
    pub created_at: i64,
}

impl UserInfo {
    /// 返回适合界面展示的主身份文案，优先使用昵称，其次用户名，最后回退到邮箱前缀。
    pub fn display_name(&self) -> String {
        self.nickname
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| {
                self.username
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| {
                self.email
                    .split('@')
                    .next()
                    .unwrap_or(&self.email)
                    .to_string()
            })
    }

    /// 返回适合界面展示的副身份文案。
    ///
    /// 当主文案已经等于邮箱时，不再重复显示邮箱。
    pub fn secondary_identity(&self) -> Option<String> {
        let display_name = self.display_name();
        if display_name == self.email {
            None
        } else {
            Some(self.email.clone())
        }
    }
}
