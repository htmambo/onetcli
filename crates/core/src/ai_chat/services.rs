//! AI Chat Services - 共用服务层
//!
//! 提供 AI 聊天面板共用的服务功能：
//! - SessionService: 会话持久化服务

use crate::llm::chat_history::{ChatMessage, ChatSession, MessageRepository, SessionRepository};
use crate::llm::storage::ProviderRepository;
use crate::llm::{ChatRequest, GlobalProviderState, Message, Role};
use crate::storage::StorageManager;
use crate::storage::traits::Repository;
use rust_i18n::t;

// ============================================================================
// 错误类型
// ============================================================================

/// SessionService 错误类型
#[derive(Debug, Clone)]
pub enum SessionError {
    /// 仓库不可用
    RepositoryNotAvailable,
    /// 会话未找到
    SessionNotFound,
    /// 存储错误
    StorageError(String),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::RepositoryNotAvailable => {
                write!(f, "{}", t!("AiChat.session_repo_unavailable"))
            }
            SessionError::SessionNotFound => write!(f, "{}", t!("AiChat.session_not_found")),
            SessionError::StorageError(msg) => {
                write!(f, "{}", t!("AiChat.session_storage_error", error = msg))
            }
        }
    }
}

impl std::error::Error for SessionError {}

// ============================================================================
// 会话连接信息
// ============================================================================

/// 创建会话时的连接上下文信息
#[derive(Clone, Debug, Default)]
pub struct SessionConnectionInfo {
    pub connection_id: Option<String>,
    pub database_name: Option<String>,
    pub database_type: Option<String>,
}

// ============================================================================
// SessionService
// ============================================================================

/// 会话持久化服务
#[derive(Clone)]
pub struct SessionService {
    storage_manager: StorageManager,
}

impl SessionService {
    /// 创建新的 SessionService
    pub fn new(storage_manager: StorageManager) -> Self {
        Self { storage_manager }
    }

    /// 创建新会话
    pub fn create_session(
        &self,
        name: String,
        provider_id: String,
        connection_info: Option<SessionConnectionInfo>,
    ) -> Result<i64, SessionError> {
        let session_repo = self
            .storage_manager
            .get::<SessionRepository>()
            .ok_or(SessionError::RepositoryNotAvailable)?;

        let mut session = ChatSession::new(name, provider_id);
        if let Some(cx) = connection_info {
            session.connection_id = cx.connection_id;
            session.database_name = cx.database_name;
            session.database_type = cx.database_type;
        }
        session_repo
            .insert(&mut session)
            .map_err(|e| SessionError::StorageError(e.to_string()))
    }

    /// 获取会话
    pub fn get_session(&self, session_id: i64) -> Result<Option<ChatSession>, SessionError> {
        let session_repo = self
            .storage_manager
            .get::<SessionRepository>()
            .ok_or(SessionError::RepositoryNotAvailable)?;

        session_repo
            .get(session_id)
            .map_err(|e| SessionError::StorageError(e.to_string()))
    }

    /// 列出所有会话
    pub fn list_sessions(&self) -> Result<Vec<ChatSession>, SessionError> {
        let session_repo = self
            .storage_manager
            .get::<SessionRepository>()
            .ok_or(SessionError::RepositoryNotAvailable)?;

        session_repo
            .list()
            .map_err(|e| SessionError::StorageError(e.to_string()))
    }

    /// 删除会话
    pub fn delete_session(&self, session_id: i64) -> Result<(), SessionError> {
        let session_repo = self
            .storage_manager
            .get::<SessionRepository>()
            .ok_or(SessionError::RepositoryNotAvailable)?;

        session_repo
            .delete(session_id)
            .map_err(|e| SessionError::StorageError(e.to_string()))
    }

    /// 更新会话名称
    pub fn update_session_name(&self, session_id: i64, name: String) -> Result<(), SessionError> {
        let session_repo = self
            .storage_manager
            .get::<SessionRepository>()
            .ok_or(SessionError::RepositoryNotAvailable)?;

        let mut session = session_repo
            .get(session_id)
            .map_err(|e| SessionError::StorageError(e.to_string()))?
            .ok_or(SessionError::SessionNotFound)?;

        session.name = name;
        session_repo
            .update(&session)
            .map_err(|e| SessionError::StorageError(e.to_string()))
    }

    /// 添加用户消息
    pub fn add_user_message(&self, session_id: i64, content: String) -> Result<i64, SessionError> {
        let message_repo = self
            .storage_manager
            .get::<MessageRepository>()
            .ok_or(SessionError::RepositoryNotAvailable)?;

        let mut message = ChatMessage::user(session_id, content);
        message_repo
            .insert(&mut message)
            .map_err(|e| SessionError::StorageError(e.to_string()))
    }

    /// 添加助手消息
    pub fn add_assistant_message(
        &self,
        session_id: i64,
        content: String,
    ) -> Result<i64, SessionError> {
        let message_repo = self
            .storage_manager
            .get::<MessageRepository>()
            .ok_or(SessionError::RepositoryNotAvailable)?;

        let mut message = ChatMessage::assistant(session_id, content);
        message_repo
            .insert(&mut message)
            .map_err(|e| SessionError::StorageError(e.to_string()))
    }

    /// 获取会话的所有消息
    pub fn get_messages(&self, session_id: i64) -> Result<Vec<ChatMessage>, SessionError> {
        let message_repo = self
            .storage_manager
            .get::<MessageRepository>()
            .ok_or(SessionError::RepositoryNotAvailable)?;

        message_repo
            .list_by_session(session_id)
            .map_err(|e| SessionError::StorageError(e.to_string()))
    }

    /// 确保会话存在，不存在则创建
    pub fn ensure_session(
        &self,
        session_id: Option<i64>,
        provider_id: &str,
        default_name: &str,
        connection_info: Option<SessionConnectionInfo>,
    ) -> Result<i64, SessionError> {
        if let Some(id) = session_id {
            // 验证会话存在
            if self.get_session(id)?.is_some() {
                return Ok(id);
            }
        }

        // 创建新会话
        self.create_session(default_name.to_string(), provider_id.to_string(), connection_info)
    }

    /// 获取存储管理器的引用
    pub fn storage_manager(&self) -> &StorageManager {
        &self.storage_manager
    }
}

// ============================================================================
// AI 生成会话标题
// ============================================================================

/// 使用 AI 根据用户首条消息生成会话标题
///
/// 返回 `None` 表示生成失败，调用方应回退到 `extract_session_name`。
pub async fn generate_ai_session_title(
    provider_id_str: &str,
    user_message: &str,
    global_provider_state: &GlobalProviderState,
    storage_manager: &StorageManager,
) -> Option<String> {
    let provider_id: i64 = provider_id_str.parse().ok()?;

    let config = storage_manager
        .get::<ProviderRepository>()?
        .get(provider_id)
        .ok()
        .flatten()?;

    let provider = global_provider_state
        .manager()
        .get_provider(&config)
        .await
        .ok()?;

    let system_prompt = "请根据用户的第一条消息，生成一个简洁的会话标题（不超过15个字）。只返回标题文本，不要添加任何解释、引号或多余内容。";

    let request = ChatRequest {
        model: config.model.clone(),
        messages: vec![
            Message::text(Role::System, system_prompt),
            Message::text(Role::User, user_message),
        ],
        max_tokens: Some(30),
        temperature: Some(0.3),
        stream: Some(false),
        ..Default::default()
    };

    match provider.chat(&request).await {
        Ok(title) => {
            let title = title.trim().trim_matches('"').trim_matches('\'').to_string();
            if title.is_empty() {
                None
            } else {
                Some(title)
            }
        }
        Err(e) => {
            tracing::warn!("AI 生成会话标题失败: {}", e);
            None
        }
    }
}

// ============================================================================
// 会话标题工具函数
// ============================================================================

/// 从消息内容提取会话名称（过滤代码块和 @提及，智能截断）
pub fn extract_session_name(content: &str) -> String {
    // 1. 去掉 markdown 代码块
    let mut cleaned = String::new();
    let mut in_code_block = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if !in_code_block && !trimmed.is_empty() {
            cleaned.push_str(trimmed);
            cleaned.push(' ');
        }
    }

    // 2. 去掉 @表名 提及语法
    let re = regex::Regex::new(r"@\w+\b").ok();
    if let Some(re) = re {
        cleaned = re.replace_all(&cleaned, "").to_string();
    }

    // 3. 合并多余空白
    let clean_content = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let clean_content = clean_content.trim();

    if clean_content.is_empty() {
        return t!("AiChat.new_session_name").to_string();
    }

    if clean_content.chars().count() <= 20 {
        clean_content.to_string()
    } else {
        // 4. 智能截断：优先在词边界截断
        let mut result = String::new();
        let mut char_count = 0;
        for word in clean_content.split_whitespace() {
            let word_len = word.chars().count();
            if char_count + word_len + (!result.is_empty() as usize) > 17 {
                break;
            }
            if !result.is_empty() {
                result.push(' ');
                char_count += 1;
            }
            result.push_str(word);
            char_count += word_len;
        }
        if result.is_empty() {
            // fallback：单个词超过 17 字，硬截断
            result = clean_content.chars().take(17).collect();
        }
        format!("{}...", result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_session_name_short() {
        assert_eq!(extract_session_name("Hello"), "Hello");
        assert_eq!(extract_session_name("  Hello  "), "Hello");
    }

    #[test]
    fn test_extract_session_name_long() {
        let long_text = "这是一个非常长的会话标题，需要被截断，并且应该带有省略号";
        let result = extract_session_name(long_text);
        assert!(result.ends_with("..."));
        assert!(result.chars().count() <= 20);
    }

    #[test]
    fn test_extract_session_name_with_newlines() {
        assert_eq!(extract_session_name("Hello\nWorld"), "Hello World");
        assert_eq!(
            extract_session_name("Line1\n\nLine2\nLine3"),
            "Line1 Line2 Line3"
        );
        assert_eq!(extract_session_name("\n  Hello  \n"), "Hello");
    }
}
