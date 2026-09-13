use crate::types::FieldType;
use one_core::storage::DatabaseType;
use serde::{Deserialize, Serialize};
use sqlparser::dialect::{GenericDialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect};
use sqlparser::tokenizer::{Token, Tokenizer};
use std::borrow::Cow;
use std::path::PathBuf;

/// SQL 脚本来源
#[derive(Clone, Debug)]
pub enum SqlSource {
    /// 直接的 SQL 脚本字符串
    Script(String),
    /// SQL 文件路径
    File(PathBuf),
}

impl SqlSource {
    pub fn file_size(&self) -> Option<u64> {
        match self {
            SqlSource::Script(s) => Some(s.len() as u64),
            SqlSource::File(path) => std::fs::metadata(path).ok().map(|m| m.len()),
        }
    }

    pub fn is_file(&self) -> bool {
        matches!(self, SqlSource::File(_))
    }
}

/// Execution options for SQL script
#[derive(Debug, Clone)]
pub struct ExecOptions {
    /// Whether to stop execution when encountering an error
    pub stop_on_error: bool,
    /// Whether to wrap the entire script in a transaction
    pub transactional: bool,
    /// Maximum number of rows to return for query results
    pub max_rows: Option<usize>,
    /// 是否启用流式执行（逐条解析执行，适合大文件/大脚本）
    /// 默认 false，会先解析所有语句再执行
    pub streaming: bool,
}

impl Default for ExecOptions {
    fn default() -> Self {
        Self {
            stop_on_error: true,
            transactional: false,
            max_rows: Some(1000),
            streaming: false,
        }
    }
}

impl ExecOptions {
    /// 对查询结果集应用 max_rows 截断，避免 UI 层持有过多数据
    pub fn truncate_results(&self, results: &mut Vec<SqlResult>) {
        let Some(max_rows) = self.max_rows else {
            return;
        };
        for result in results.iter_mut() {
            if let SqlResult::Query(ref mut query) = result {
                if query.rows.len() > max_rows {
                    query.rows.truncate(max_rows);
                }
            }
        }
    }
}

/// 为 SELECT 查询按数据库方言注入行数限制（LIMIT / TOP / FETCH）。
/// 已有 LIMIT/TOP/FETCH 的语句保持不变；非查询语句原样返回。
pub(crate) fn apply_query_max_rows(
    db_type: DatabaseType,
    sql: &str,
    max_rows: Option<usize>,
    is_query: bool,
) -> Cow<'_, str> {
    let Some(max_rows) = max_rows else {
        return Cow::Borrowed(sql);
    };
    if max_rows == 0 {
        return Cow::Borrowed(sql);
    }
    let Some(tokens) = simple_select_tokens(&db_type, sql) else {
        return Cow::Borrowed(sql);
    };
    if !is_query || has_existing_row_limit(&tokens) {
        return Cow::Borrowed(sql);
    }

    let _ = db_type;
    append_query_clause(sql, &format!("LIMIT {max_rows}"))
}

fn append_query_clause<'a>(sql: &'a str, clause: &str) -> Cow<'a, str> {
    let trimmed_end = sql.trim_end();
    let trailing_ws = &sql[trimmed_end.len()..];
    let (body, terminator) = trimmed_end
        .strip_suffix(';')
        .map(|body| (body.trim_end(), ";"))
        .unwrap_or((trimmed_end, ""));
    Cow::Owned(format!("{body} {clause}{terminator}{trailing_ws}"))
}

fn simple_select_tokens(db_type: &DatabaseType, sql: &str) -> Option<Vec<SqlToken>> {
    let tokens = significant_tokens(db_type, sql)?;
    tokens
        .first()
        .is_some_and(|token| token.depth == 0 && word_eq(&token.token, "SELECT"))
        .then_some(tokens)
}

fn has_existing_row_limit(tokens: &[SqlToken]) -> bool {
    tokens.iter().any(|token| {
        token.depth == 0
            && (word_eq(&token.token, "LIMIT")
                || word_eq(&token.token, "FETCH")
                || word_eq(&token.token, "FORMAT")
                || word_eq(&token.token, "ROWNUM")
                || word_eq(&token.token, "TOP"))
    })
}

#[derive(Debug)]
struct SqlToken {
    token: Token,
    depth: usize,
}

fn significant_tokens(db_type: &DatabaseType, sql: &str) -> Option<Vec<SqlToken>> {
    let dialect = tokenizer_dialect(db_type);
    let mut tokenizer = Tokenizer::new(dialect.as_ref(), sql);
    let tokens = tokenizer.tokenize_with_location().ok()?;
    let mut depth = 0usize;
    let mut output = Vec::new();
    for token_with_span in tokens {
        match token_with_span.token {
            Token::Whitespace(_) | Token::EOF => {}
            Token::LParen => depth += 1,
            Token::RParen => depth = depth.saturating_sub(1),
            token => output.push(SqlToken { token, depth }),
        }
    }
    Some(output)
}

fn tokenizer_dialect(db_type: &DatabaseType) -> Box<dyn sqlparser::dialect::Dialect> {
    match db_type {
        DatabaseType::MySQL => Box::new(MySqlDialect {}),
        DatabaseType::PostgreSQL => Box::new(PostgreSqlDialect {}),
        DatabaseType::SQLite => Box::new(SQLiteDialect {}),
        DatabaseType::External => Box::new(GenericDialect {}),
    }
}

fn word_eq(token: &Token, expected: &str) -> bool {
    matches!(
        token,
        Token::Word(word)
            if word.quote_style.is_none() && word.value.eq_ignore_ascii_case(expected)
    )
}

/// Result of a single SQL statement execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SqlResult {
    /// Query result (SELECT, SHOW, etc.)
    Query(QueryResult),
    /// Execution result (INSERT, UPDATE, DELETE, DDL, etc.)
    Exec(ExecResult),
    /// Error result
    Error(SqlErrorInfo),
}

impl SqlResult {
    pub fn is_error(&self) -> bool {
        matches!(self, SqlResult::Error(_))
    }
}

/// Column metadata for query results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryColumnMeta {
    /// Column name
    pub name: String,
    /// Original database type (e.g., "VARCHAR(255)", "INT")
    pub db_type: String,
    /// Abstract field type for UI rendering
    pub field_type: FieldType,
    /// Whether the column is nullable
    pub nullable: bool,
}

impl QueryColumnMeta {
    pub fn new(name: impl Into<String>, db_type: impl Into<String>) -> Self {
        let db_type_str = db_type.into();
        let field_type = FieldType::from_db_type(&db_type_str);
        Self {
            name: name.into(),
            db_type: db_type_str,
            field_type,
            nullable: true,
        }
    }

    pub fn with_nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self
    }
}

/// Query result with data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Original SQL statement
    pub sql: String,
    /// Column names
    pub columns: Vec<String>,
    /// Column metadata with type information
    pub column_meta: Vec<QueryColumnMeta>,
    /// Row data (each row is a vector of optional strings)
    pub rows: Vec<Vec<Option<String>>>,
    /// Execution time in milliseconds
    #[serde(with = "elapsed_ms_serde")]
    pub elapsed_ms: u128,
}

/// Execution result for non-query statements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResult {
    /// Original SQL statement
    pub sql: String,
    /// Number of rows affected
    pub rows_affected: u64,
    /// Execution time in milliseconds
    #[serde(with = "elapsed_ms_serde")]
    pub elapsed_ms: u128,
    /// Optional message
    pub message: Option<String>,
}

/// Error information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlErrorInfo {
    /// Original SQL statement
    pub sql: String,
    /// Error message
    pub message: String,
}

mod elapsed_ms_serde {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &u128, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64((*value).try_into().unwrap_or(u64::MAX))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u128, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(u64::deserialize(deserializer)? as u128)
    }
}

pub fn format_message(sql: &str, rows_affected: u64) -> String {
    let trimmed = sql.trim().to_uppercase();

    if trimmed.starts_with("INSERT") {
        format!("Inserted {} row(s)", rows_affected)
    } else if trimmed.starts_with("UPDATE") {
        format!("Updated {} row(s)", rows_affected)
    } else if trimmed.starts_with("DELETE") {
        format!("Deleted {} row(s)", rows_affected)
    } else if trimmed.starts_with("REPLACE") {
        format!("Replaced {} row(s)", rows_affected)
    } else if trimmed.starts_with("CREATE") {
        "Object created successfully".to_string()
    } else if trimmed.starts_with("ALTER") {
        "Object altered successfully".to_string()
    } else if trimmed.starts_with("DROP") {
        "Object dropped successfully".to_string()
    } else if trimmed.starts_with("TRUNCATE") {
        "Table truncated successfully".to_string()
    } else if trimmed.starts_with("RENAME") {
        "Object renamed successfully".to_string()
    } else if trimmed.starts_with("USE") {
        "Database changed successfully".to_string()
    } else if trimmed.starts_with("SET") {
        "Variable set successfully".to_string()
    } else if trimmed.starts_with("BEGIN") || trimmed.starts_with("START TRANSACTION") {
        "Transaction started".to_string()
    } else if trimmed.starts_with("COMMIT") {
        "Transaction committed".to_string()
    } else if trimmed.starts_with("ROLLBACK") {
        "Transaction rolled back".to_string()
    } else {
        format!(
            "Query executed successfully, {} row(s) affected",
            rows_affected
        )
    }
}

/// Statement type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementType {
    /// Query statement (SELECT, SHOW, etc.)
    Query,
    /// Data manipulation (INSERT, UPDATE, DELETE)
    Dml,
    /// Data definition (CREATE, ALTER, DROP)
    Ddl,
    /// Transaction control (BEGIN, COMMIT, ROLLBACK)
    Transaction,
    /// Database commands (USE, SET)
    Command,
    /// Other execution statements
    Exec,
}

#[cfg(test)]
mod query_max_rows_tests {
    use super::*;
    use one_core::storage::DatabaseType;

    #[test]
    fn query_max_rows_adds_limit_for_limit_dialects() {
        let sql = apply_query_max_rows(DatabaseType::MySQL, "select * from users", Some(25), true);
        assert_eq!("select * from users LIMIT 25", sql);
    }

    #[test]
    fn query_max_rows_keeps_existing_limit() {
        let sql = apply_query_max_rows(
            DatabaseType::PostgreSQL,
            "select * from users limit 5",
            Some(25),
            true,
        );
        assert_eq!("select * from users limit 5", sql);
    }

    #[test]
    fn query_max_rows_ignores_non_queries_and_unbounded_options() {
        assert_eq!(
            "update users set name = 'a'",
            apply_query_max_rows(
                DatabaseType::MySQL,
                "update users set name = 'a'",
                Some(25),
                false,
            )
        );
        assert_eq!(
            "select * from users",
            apply_query_max_rows(DatabaseType::MySQL, "select * from users", None, true)
        );
        assert_eq!(
            "show tables",
            apply_query_max_rows(DatabaseType::MySQL, "show tables", Some(25), true)
        );
    }

    #[test]
    fn query_max_rows_ignores_limit_inside_string() {
        let sql = apply_query_max_rows(
            DatabaseType::SQLite,
            "select 'limit 1' as text from users",
            Some(25),
            true,
        );
        assert_eq!("select 'limit 1' as text from users LIMIT 25", sql);
    }
}
