use crate::DatabasePlugin;
use crate::executor::{ExecOptions, SqlResult, SqlSource};
use async_trait::async_trait;
use one_core::storage::DbConnectionConfig;
use thiserror::Error;
use tokio::sync::mpsc;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("connection error: {message}")]
    Connection {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("query error: {message}")]
    Query {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("transaction error: {message}")]
    Transaction {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("not connected to database")]
    NotConnected,

    #[error("operation not supported: {0}")]
    NotSupported(String),

    /// 外部驱动 manifest 校验/加载失败（协议不兼容、字段非法等）。
    /// 与 `Internal` 区分：这是用户可预期的配置问题，非程序内部错误。
    #[error("invalid driver manifest: {0}")]
    InvalidManifest(String),

    /// session 不存在（结构性错误，**单一事实来源**）
    ///
    /// 与 `Internal` 区分：这是**结构性**区分，编译器保证穷尽匹配，
    /// 无需关心消息格式。生产者是 ConnectionManager 的三个 NotFound 路径
    /// （get_session_connection / release_session_internal / close_session）。
    /// 消费端通过 `is_session_not_found(&err)` 判定，**取代字符串前缀匹配**。
    ///
    /// 单一事实来源：本变体的 Display 与分类器共享同一字符串字面量
    /// `"session not found: "`，避免双源漂移。
    #[error("session not found: {0}")]
    SessionNotFound(String),

    /// **Deprecated 用法**：`DbError::Internal(String)` **禁止**用于表达
    /// "session 不存在"（必须使用 `DbError::SessionNotFound(String)` 变体）。
    /// 否则 `is_session_not_found` 分类器无法识别，NotFound 错误会泄露到调用方。
    ///
    /// 历史背景：Round 14 前 IPC 路径假设会通过 Display 序列化产生
    /// `Internal("session not found: ...")`，现已删除该兜底分支。
    /// 如未来需重新支持字符串序列化，应通过序列化层显式转换 + 新增 typed error code。
    #[error("internal error: {0}")]
    Internal(String),
}

impl DbError {
    /// 编译期强制同步所有变体的穷尽枚举（无 `_ =>` 通配）。
    ///
    /// 用途：`no_other_variant_collides_with_session_not_found` 等变体级
    /// 分类测试通过调用本函数**强制列出**每个变体；新增 `DbError` 变体时
    /// 编译失败提醒开发者同步更新碰撞测试，避免未来变体静默失覆盖。
    pub fn variant_tag(&self) -> &'static str {
        match self {
            DbError::Connection { .. } => "Connection",
            DbError::Query { .. } => "Query",
            DbError::Transaction { .. } => "Transaction",
            DbError::NotConnected => "NotConnected",
            DbError::NotSupported(_) => "NotSupported",
            DbError::InvalidManifest(_) => "InvalidManifest",
            DbError::SessionNotFound(_) => "SessionNotFound",
            DbError::Internal(_) => "Internal",
        }
    }
}

impl DbError {
    pub fn connection(message: impl Into<String>) -> Self {
        Self::Connection {
            message: message.into(),
            source: None,
        }
    }

    pub fn connection_with_source<E>(message: impl Into<String>, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        let message = format!("{}: {}", message.into(), source);
        Self::Connection {
            message,
            source: Some(Box::new(source)),
        }
    }

    pub fn query(message: impl Into<String>) -> Self {
        Self::Query {
            message: message.into(),
            source: None,
        }
    }

    pub fn query_with_source<E>(message: impl Into<String>, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        let message = format!("{}: {}", message.into(), source);
        Self::Query {
            message,
            source: Some(Box::new(source)),
        }
    }

    pub fn transaction(message: impl Into<String>) -> Self {
        Self::Transaction {
            message: message.into(),
            source: None,
        }
    }

    pub fn transaction_with_source<E>(message: impl Into<String>, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        let message = format!("{}: {}", message.into(), source);
        Self::Transaction {
            message,
            source: Some(Box::new(source)),
        }
    }
}

/// 流式执行进度信息
#[derive(Clone, Debug)]
pub struct StreamingProgress {
    pub current: usize,
    pub total: usize,
    pub result: SqlResult,
    /// 已读取字节数（文件流式模式）
    pub bytes_read: u64,
    /// 文件总大小（文件流式模式，0 表示非文件模式）
    pub file_size: u64,
}

impl StreamingProgress {
    pub fn new(current: usize, total: usize, result: SqlResult) -> Self {
        Self {
            current,
            total,
            result,
            bytes_read: 0,
            file_size: 0,
        }
    }

    pub fn with_file_progress(
        current: usize,
        result: SqlResult,
        bytes_read: u64,
        file_size: u64,
    ) -> Self {
        Self {
            current,
            total: 0,
            result,
            bytes_read,
            file_size,
        }
    }

    /// 计算进度百分比
    /// 文件模式使用字节比例，脚本模式使用语句比例
    pub fn progress_percent(&self) -> f32 {
        if self.file_size > 0 {
            (self.bytes_read as f64 / self.file_size as f64 * 100.0) as f32
        } else if self.total > 0 {
            (self.current as f64 / self.total as f64 * 100.0) as f32
        } else {
            0.0
        }
    }
}

#[async_trait]
pub trait DbConnection: Sync + Send {
    fn config(&self) -> &DbConnectionConfig;

    /// Update database field in config (used when connection's actual database changes)
    fn set_config_database(&mut self, database: Option<String>);

    /// Whether this database type supports switching database within a connection
    fn supports_database_switch(&self) -> bool {
        true
    }

    /// Whether an idle release should close the physical connection.
    ///
    /// 默认对所有内置驱动复用连接池。
    fn close_on_release(&self) -> bool {
        false
    }

    async fn connect(&mut self) -> Result<(), DbError>;
    async fn disconnect(&mut self) -> Result<(), DbError>;
    async fn execute(
        &self,
        plugin: &dyn DatabasePlugin,
        script: &str,
        options: ExecOptions,
    ) -> Result<Vec<SqlResult>, DbError>;
    async fn query(&self, query: &str) -> Result<SqlResult, DbError>;

    fn ping_query(&self) -> &'static str {
        "SELECT 1"
    }

    async fn ping(&self) -> Result<(), DbError> {
        match self.query(self.ping_query()).await? {
            SqlResult::Error(error) => Err(DbError::connection(format!(
                "connection ping failed: {}",
                error.message
            ))),
            _ => Ok(()),
        }
    }

    /// Get current database/schema name from the connection
    async fn current_database(&self) -> Result<Option<String>, DbError>;

    /// Switch to a different database
    async fn switch_database(&self, database: &str) -> Result<(), DbError>;

    /// Switch to a different schema within the current database
    /// For PostgreSQL: SET search_path TO schema
    /// For Oracle: Uses switch_database instead (schema = user)
    /// Other databases: No-op (use schema table in SQL)
    async fn switch_schema(&self, _schema: &str) -> Result<(), DbError> {
        Ok(())
    }

    async fn execute_streaming(
        &self,
        plugin: &dyn DatabasePlugin,
        source: SqlSource,
        options: ExecOptions,
        sender: mpsc::Sender<StreamingProgress>,
    ) -> Result<(), DbError>;

    /// Best-effort rollback of any active transaction.
    ///
    /// 默认实现：对连接执行 `ROLLBACK` 语句。MySQL/PostgreSQL/SQLite/Oracle
    /// 均支持 ROLLBACK；非事务上下文下通常无副作用或仅发 warning。
    ///
    /// **用途**：在 release_session / close_session 前调用，防止未提交事务
    /// 状态泄漏到下一个使用者（脏连接复用）。
    ///
    /// **错误处理**：失败仅记录 warn，不阻塞 release 流程（连接可能本身已
    /// 损坏或不支持 ROLLBACK）。
    async fn rollback_if_active(&self) -> Result<(), DbError> {
        match self.query("ROLLBACK").await {
            Ok(_) => Ok(()),
            Err(e) => {
                tracing::warn!(
                    target: "db::connection",
                    error = ?e,
                    "rollback_if_active failed — proceeding with release anyway"
                );
                Err(e)
            }
        }
    }
}
