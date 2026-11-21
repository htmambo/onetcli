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

    /// 机器可读错误码（稳定字符串，**wire 格式**——可用于 IPC、metric、监控告警键）。
    ///
    /// 与 [`variant_tag`](Self::variant_tag) 的区别：
    /// - `variant_tag`：Rust 标识符风格（PascalCase），**仅供日志/调试**
    /// - `code`：snake_case 稳定字符串，**面向外部系统**
    ///
    /// **稳定性保证**：已发布的 code 字符串**永不变更**——可作为 IPC 协议字段、
    /// metric 标签键、Sentry/告警路由。**新增**变体时可自由分配新 code；
    /// **重命名**已有 code 视为 breaking change，需 major version bump。
    ///
    /// 调用方注意：code 仅区分错误**类别**，不携带错误**详情**（message/source
    /// 仍走 Display 序列化，但**禁止**用于自动化决策——含 DSN/凭据等敏感信息）。
    pub fn code(&self) -> &'static str {
        match self {
            DbError::Connection { .. } => "db.connection",
            DbError::Query { .. } => "db.query",
            DbError::Transaction { .. } => "db.transaction",
            DbError::NotConnected => "db.not_connected",
            DbError::NotSupported(_) => "db.not_supported",
            DbError::InvalidManifest(_) => "db.invalid_manifest",
            DbError::SessionNotFound(_) => "db.session_not_found",
            DbError::Internal(_) => "db.internal",
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

    /// 连接上是否存在活动事务。
    ///
    /// 默认返回 `true`（保守策略：仍然发送 ROLLBACK）。
    /// 能可靠判断事务状态的实现可覆写为 `false`，
    /// 省掉一次无谓的 ROLLBACK 往返（远程库上这是一次完整网络往返）。
    async fn has_active_transaction(&self) -> bool {
        true
    }

    /// Best-effort rollback of any active transaction.
    ///
    /// 默认实现：连接上没有活动事务时直接返回，否则对连接执行 `ROLLBACK` 语句。
    /// MySQL/PostgreSQL/SQLite/Oracle 均支持 ROLLBACK；
    /// 非事务上下文下通常无副作用或仅发 warning。
    ///
    /// **用途**：在 release_session / close_session 前调用，防止未提交事务
    /// 状态泄漏到下一个使用者（脏连接复用）。
    ///
    /// **错误处理**：失败仅记录 warn，不阻塞 release 流程（连接可能本身已
    /// 损坏或不支持 ROLLBACK）。
    async fn rollback_if_active(&self) -> Result<(), DbError> {
        if !self.has_active_transaction().await {
            return Ok(());
        }
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

/// 去掉语句开头的注释，便于判断语句类型。
///
/// 只处理行注释（`--`）与块注释（`/* */`），够用于"这条语句是不是
/// 事务控制语句 / 写语句"的判断。
fn strip_leading_comments(statement: &str) -> &str {
    let mut rest = statement.trim_start();
    loop {
        if let Some(after) = rest.strip_prefix("--") {
            // 行注释到行尾结束
            match after.find('\n') {
                Some(idx) => rest = after[idx + 1..].trim_start(),
                None => return "",
            }
        } else if let Some(after) = rest.strip_prefix("/*") {
            match after.find("*/") {
                Some(idx) => rest = after[idx + 2..].trim_start(),
                None => return "",
            }
        } else {
            return rest;
        }
    }
}

/// 推断脚本执行完毕后是否仍有活动事务。
///
/// 输入必须是**已按 SQL 方言切分好的语句**（驱动解析后的结果），
/// 不能直接对原始脚本文本做 `split(';')`——字符串字面量里的分号会把
/// 一条语句切成两半，`INSERT ... VALUES ('BEGIN; ROLLBACK;')` 这类内容
/// 会被误判成事务控制语句，进而跳过必要的 ROLLBACK。
///
/// 只认显式的 `BEGIN`/`START TRANSACTION` 与 `COMMIT`/`ROLLBACK`；
/// 以最后一条事务控制语句为准。判断不出来时返回 `None`，
/// 调用方按"可能有事务"的保守策略处理。
pub fn infer_transaction_state(statements: &[String]) -> Option<bool> {
    let mut state = None;
    for statement in statements {
        let trimmed = strip_leading_comments(statement).to_ascii_uppercase();
        if trimmed.starts_with("BEGIN") || trimmed.starts_with("START TRANSACTION") {
            state = Some(true);
        } else if trimmed.starts_with("COMMIT") || trimmed.starts_with("ROLLBACK") {
            state = Some(false);
        }
    }
    state
}

#[cfg(test)]
mod tests {
    use super::infer_transaction_state;

    #[test]
    fn infer_transaction_state_识别显式事务控制语句() {
        let stmts =
            |items: &[&str]| -> Vec<String> { items.iter().map(|s| s.to_string()).collect() };

        // 没有事务控制语句时判断不出来
        assert_eq!(infer_transaction_state(&stmts(&["SELECT 1"])), None);
        // 显式开启事务
        assert_eq!(infer_transaction_state(&stmts(&["BEGIN"])), Some(true));
        assert_eq!(
            infer_transaction_state(&stmts(&["START TRANSACTION"])),
            Some(true)
        );
        assert_eq!(
            infer_transaction_state(&stmts(&["BEGIN", "INSERT INTO t VALUES (1)"])),
            Some(true)
        );
        // 以最后一条事务控制语句为准
        assert_eq!(
            infer_transaction_state(&stmts(&["BEGIN", "INSERT INTO t VALUES (1)", "COMMIT"])),
            Some(false)
        );
        assert_eq!(infer_transaction_state(&stmts(&["ROLLBACK"])), Some(false));
        // 注释不影响判断
        assert_eq!(
            infer_transaction_state(&stmts(&["-- 说明\nBEGIN", "COMMIT"])),
            Some(false)
        );
    }

    #[test]
    fn infer_transaction_state_不受字符串字面量干扰() {
        // 语句已由方言解析器切分，字面量里的 BEGIN/COMMIT 不应被当成事务控制语句
        let statements = vec!["INSERT INTO t VALUES ('BEGIN; ROLLBACK;')".to_string()];
        assert_eq!(infer_transaction_state(&statements), None);
    }
}
