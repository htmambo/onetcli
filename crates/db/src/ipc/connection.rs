use crate::connection::{DbConnection, DbError, StreamingProgress};
use crate::executor::{ExecOptions, QueryColumnMeta, QueryResult, SqlResult, SqlSource};
use crate::ipc::client::JsonRpcClient;
use crate::ipc::protocol::{
    connection_config_params_with_target, database_params, empty_params, schema_params, sql_params,
};
use crate::ipc::registry::IpcDriverManifest;
use crate::ssh_tunnel::resolve_connection_target;
use crate::{DatabasePlugin, SqlErrorInfo, truncate_str};
use async_trait::async_trait;
use one_core::storage::{DatabaseType, DbConnectionConfig};
use ssh::LocalPortForwardTunnel;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, error};

pub struct ExternalDbConnection {
    config: DbConnectionConfig,
    driver: IpcDriverManifest,
    /// 宿主建立的本地端口转发；driver 只看到 target host/port。
    tunnel: Option<LocalPortForwardTunnel>,
    /// `Arc` 让 `request` 能短锁拿 clone 后立刻释放,允许多 caller 并发调用
    /// `JsonRpcClient::request`。`Mutex<Option<...>>` 处理 connect/disconnect 的
    /// owner 切换。
    client: Mutex<Option<Arc<JsonRpcClient>>>,
}

impl ExternalDbConnection {
    pub fn new(config: DbConnectionConfig, driver: IpcDriverManifest) -> Self {
        Self {
            config,
            driver,
            tunnel: None,
            client: Mutex::new(None),
        }
    }

    async fn request<T>(&self, method: &str, params: serde_json::Value) -> Result<T, DbError>
    where
        T: serde::de::DeserializeOwned,
    {
        // 短锁:仅在拿 Arc clone 时持锁,之后释放,允许多 caller 并发调用 client。
        let client = {
            let guard = self.client.lock().await;
            guard.as_ref().cloned().ok_or(DbError::NotConnected)?
        };

        let result = client.request(method, params).await;

        // P0-4:transport 已 fatal(reader task 退出),evict 当前 broken client
        // 让下次 request 直接得到 NotConnected,触发上层重连逻辑。
        // 用 Arc::ptr_eq 防止误踩 — 别的 caller 可能已经 evict + reconnect。
        if client.is_closed() {
            let mut guard = self.client.lock().await;
            if let Some(current) = guard.as_ref() {
                if Arc::ptr_eq(current, &client) {
                    *guard = None;
                    // 旧 Arc 在最后一个 in-flight reference drop 后才真正释放,
                    // 由 JsonRpcClient::Drop 完成 reader_task abort + kill_on_drop child。
                }
            }
        }

        result
    }

    async fn exec_schema_switch_sql(&self, sql: &str) -> Result<(), DbError> {
        match self.query(sql).await? {
            SqlResult::Error(info) => Err(DbError::query(info.message)),
            _ => Ok(()),
        }
    }
}


#[derive(Clone, Copy)]
enum SchemaSwitchDialect {
    PostgreSql,
    Oracle,
    DuckDb,
}

impl SchemaSwitchDialect {
    fn sql(self, schema: &str) -> String {
        match self {
            Self::PostgreSql => {
                format!("SET search_path TO {}", quote_double_identifier(schema))
            }
            Self::Oracle => format!(
                "ALTER SESSION SET CURRENT_SCHEMA = {}",
                quote_double_identifier(schema)
            ),
            Self::DuckDb => format!("SET schema {}", quote_sql_string(schema)),
        }
    }
}

fn schema_switch_sql_for_driver(driver: &IpcDriverManifest, schema: &str) -> Option<String> {
    if schema.trim().is_empty() {
        return None;
    }
    schema_switch_dialect(driver).map(|dialect| dialect.sql(schema))
}

fn schema_switch_dialect(driver: &IpcDriverManifest) -> Option<SchemaSwitchDialect> {
    match driver.dialect.compatible_database_type.as_ref() {
        Some(DatabaseType::PostgreSQL) => Some(SchemaSwitchDialect::PostgreSql),
        Some(DatabaseType::Oracle) => Some(SchemaSwitchDialect::Oracle),
        Some(DatabaseType::DuckDB) => Some(SchemaSwitchDialect::DuckDb),
        _ => schema_switch_dialect_from_driver_id(&driver.id),
    }
}

fn schema_switch_dialect_from_driver_id(driver_id: &str) -> Option<SchemaSwitchDialect> {
    let id = driver_id.to_ascii_lowercase();
    if id.contains("postgres") || id.contains("kingbase") {
        return Some(SchemaSwitchDialect::PostgreSql);
    }
    if id.contains("oracle") || id == "dm" || id.contains("dameng") {
        return Some(SchemaSwitchDialect::Oracle);
    }
    if id.contains("duckdb") {
        return Some(SchemaSwitchDialect::DuckDb);
    }
    None
}

fn quote_double_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn quote_sql_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn is_method_not_found(error: &DbError) -> bool {
    matches!(error, DbError::NotSupported(_))
}

fn metadata_result(sql: &str, value: serde_json::Value) -> SqlResult {
    SqlResult::Query(QueryResult {
        sql: sql.to_string(),
        columns: vec!["json".to_string()],
        column_meta: vec![QueryColumnMeta::new("json", "JSON")],
        rows: vec![vec![Some(value.to_string())]],
        elapsed_ms: 0,
    })
}

#[async_trait]
impl DbConnection for ExternalDbConnection {
    fn config(&self) -> &DbConnectionConfig {
        &self.config
    }

    fn set_config_database(&mut self, database: Option<String>) {
        self.config.database = database;
    }

    fn close_on_release(&self) -> bool {
        self.driver.connection.close_on_release
    }

    async fn connect(&mut self) -> Result<(), DbError> {
        self.tunnel = None;
        let target = resolve_connection_target(&self.config).await?;
        let client =
            JsonRpcClient::start_with_connection_config(&self.driver, Some(&self.config)).await?;
        // initialize / connect 走 `&self`,这里直接用 owned client(尚未 Arc),
        // 任一步失败就把 client drop 掉 → reader_task abort + child kill_on_drop。
        let _: serde_json::Value = client.request("initialize", empty_params()).await?;
        let _: serde_json::Value = client
            .request(
                "connect",
                connection_config_params_with_target(&self.config, &target.host, target.port),
            )
            .await?;
        self.tunnel = target.tunnel;
        *self.client.lock().await = Some(Arc::new(client));
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), DbError> {
        let client_arc = self.client.lock().await.take();
        if let Some(client_arc) = client_arc {
            // 尽力发出 disconnect RPC(可能因连接已断而失败,允许)。
            let _: Result<serde_json::Value, DbError> =
                client_arc.request("disconnect", empty_params()).await;
            // 显式 abort reader + kill+wait child,确保返回前子进程已退出。
            // 即便仍有 in-flight Arc clone(并发 query 未返回),它们会因 reader 关闭
            // 而立即收到 disconnected 错误,然后 Arc 自然 drop。
            client_arc.shutdown().await;
        }
        self.tunnel = None;
        Ok(())
    }

    async fn execute(
        &self,
        plugin: &dyn DatabasePlugin,
        script: &str,
        options: ExecOptions,
    ) -> Result<Vec<SqlResult>, DbError> {
        let statements = plugin.split_sql_statements(script);
        let mut results = Vec::with_capacity(statements.len());
        for statement in statements {
            let result = self.query(&statement).await?;
            let should_stop = options.stop_on_error && result.is_error();
            results.push(result);
            if should_stop {
                break;
            }
        }
        Ok(results)
    }

    async fn query(&self, query: &str) -> Result<SqlResult, DbError> {
        if let Some(request) = query.strip_prefix("/*omnihub-ipc-metadata*/ ") {
            let value: serde_json::Value = serde_json::from_str(request)
                .map_err(|error| DbError::query_with_source("invalid metadata request", error))?;
            let method = value
                .get("method")
                .and_then(|value| value.as_str())
                .ok_or_else(|| DbError::query("metadata request method is required"))?;
            let params = value
                .get("params")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let result: serde_json::Value = self.request(method, params).await?;
            return Ok(metadata_result(query, result));
        }

        self.request("query", sql_params(query)).await
    }

    async fn ping(&self) -> Result<(), DbError> {
        let _: serde_json::Value = self.request("ping", empty_params()).await?;
        Ok(())
    }

    async fn current_database(&self) -> Result<Option<String>, DbError> {
        self.request("current_database", empty_params()).await
    }

    async fn switch_database(&self, database: &str) -> Result<(), DbError> {
        let _: serde_json::Value = self
            .request("switch_database", database_params(database))
            .await?;
        Ok(())
    }

    async fn switch_schema(&self, schema: &str) -> Result<(), DbError> {
        let switch_result: Result<serde_json::Value, DbError> = self
            .request("switch_schema", schema_params(schema))
            .await;
        let fallback_sql = schema_switch_sql_for_driver(&self.driver, schema);

        match (switch_result, fallback_sql.as_deref()) {
            (Ok(_), Some(sql)) => self.exec_schema_switch_sql(sql).await,
            (Ok(_), None) => Ok(()),
            (Err(error), Some(sql)) if is_method_not_found(&error) => {
                self.exec_schema_switch_sql(sql).await
            }
            (Err(error), _) => Err(error),
        }
    }

    async fn execute_streaming(
        &self,
        plugin: &dyn DatabasePlugin,
        source: SqlSource,
        options: ExecOptions,
        sender: mpsc::Sender<StreamingProgress>,
    ) -> Result<(), DbError> {
        debug!(
            "[MySQL] execute_streaming() called, transactional={}, streaming={}",
            options.transactional, options.streaming
        );

        let total_size = source.file_size().unwrap_or(0);
        let is_file_source = source.is_file();

        let mut parser = plugin
            .create_parser(source)
            .map_err(|e| DbError::query(format!("Failed to create parser: {}", e)))?;

        if options.streaming || is_file_source {
            let mut current = 0usize;

            while let Some(stmt_result) = parser.next() {
                let bytes_read = parser.bytes_read();
                let sql = match stmt_result {
                    Ok(s) if !s.trim().is_empty() => s,
                    Ok(_) => continue,
                    Err(e) => {
                        let progress = StreamingProgress::with_file_progress(
                            current,
                            SqlResult::Error(SqlErrorInfo {
                                sql: String::new(),
                                message: format!("Parse error: {}", e),
                            }),
                            bytes_read,
                            total_size,
                        );
                        let _ = sender.send(progress).await;
                        if options.stop_on_error {
                            break;
                        }
                        continue;
                    }
                };

                current += 1;
                debug!("[MySQL] Streaming statement {}", current);

                let result = match self.query(&sql).await {
                    Ok(r) => r,
                    Err(e) => {
                        let sql_preview = if sql.len() > 200 {
                            format!("{}...", truncate_str(&sql, 200))
                        } else {
                            sql.clone()
                        };
                        error!(
                            "[MySQL] Streaming statement {} failed: {}, SQL: {}",
                            current, e, sql_preview
                        );
                        SqlResult::Error(SqlErrorInfo {
                            sql: sql.clone(),
                            message: e.to_string(),
                        })
                    }
                };

                let is_error = result.is_error();
                let progress =
                    StreamingProgress::with_file_progress(current, result, bytes_read, total_size);
                if sender.send(progress).await.is_err() {
                    break;
                }

                if is_error && options.stop_on_error {
                    break;
                }
            }
        } else {
            let statements: Vec<String> = parser
                .filter_map(|r| r.ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let total = statements.len();
            debug!("[MySQL] Streaming {} statement(s)", total);

            if total == 0 {
                debug!("[MySQL] No statements to execute");
                return Ok(());
            }

            for (index, sql) in statements.into_iter().enumerate() {
                let current = index + 1;
                debug!("[MySQL] Streaming statement {}/{}", current, total);

                let result = match self.query(&sql).await {
                    Ok(r) => r,
                    Err(e) => {
                        let sql_preview = if sql.len() > 200 {
                            format!("{}...", truncate_str(&sql, 200))
                        } else {
                            sql.clone()
                        };
                        error!(
                            "[MySQL] Streaming statement {}/{} failed: {}, SQL: {}",
                            current, total, e, sql_preview
                        );
                        SqlResult::Error(SqlErrorInfo {
                            sql: sql.clone(),
                            message: e.to_string(),
                        })
                    }
                };

                let is_error = result.is_error();
                let progress = StreamingProgress::new(current, total, result);
                if sender.send(progress).await.is_err() {
                    break;
                }

                if is_error && options.stop_on_error {
                    break;
                }
            }
        }

        debug!("[MySQL] execute_streaming() completed");
        Ok(())
    }
}

#[cfg(test)]
mod schema_switch_tests {
    use super::*;
    use crate::ipc::registry::{IpcDriverEntry, IpcDriverManifest, IpcDriverTransport};
    use std::path::PathBuf;

    fn test_driver(driver_id: &str, compatible: Option<DatabaseType>) -> IpcDriverManifest {
        let mut driver = IpcDriverManifest {
            id: driver_id.into(),
            name: driver_id.into(),
            category: None,
            description: String::new(),
            version: String::new(),
            entry: IpcDriverEntry {
                command: "driver".into(),
                args: Vec::new(),
                working_dir: None,
                commands: Default::default(),
                env_from_config: Default::default(),
            },
            transport: IpcDriverTransport::local_socket("driver.sock"),
            dialect: Default::default(),
            capabilities: None,
            ui: Default::default(),
            connection: Default::default(),
            manifest_dir: PathBuf::from("/tmp"),
        };
        driver.dialect.compatible_database_type = compatible;
        driver
    }

    #[test]
    fn schema_switch_sql_uses_postgres_search_path_for_compatible_driver() {
        let driver = test_driver("custom", Some(DatabaseType::PostgreSQL));
        let sql = schema_switch_sql_for_driver(&driver, "tenant\"a");
        assert_eq!(sql.as_deref(), Some("SET search_path TO \"tenant\"\"a\""));
    }

    #[test]
    fn schema_switch_sql_uses_oracle_current_schema_for_compatible_driver() {
        let driver = test_driver("custom", Some(DatabaseType::Oracle));
        let sql = schema_switch_sql_for_driver(&driver, "APP");
        assert_eq!(
            sql.as_deref(),
            Some("ALTER SESSION SET CURRENT_SCHEMA = \"APP\"")
        );
    }

    #[test]
    fn schema_switch_sql_treats_dm_driver_as_oracle_compatible() {
        let driver = test_driver("dm", None);
        let sql = schema_switch_sql_for_driver(&driver, "APP");
        assert_eq!(
            sql.as_deref(),
            Some("ALTER SESSION SET CURRENT_SCHEMA = \"APP\"")
        );
    }

    #[test]
    fn schema_switch_sql_uses_duckdb_schema_setting() {
        let driver = test_driver("duckdb", Some(DatabaseType::DuckDB));
        let sql = schema_switch_sql_for_driver(&driver, "tenant'a");
        assert_eq!(sql.as_deref(), Some("SET schema 'tenant''a'"));
    }

    #[test]
    fn schema_switch_sql_skips_unknown_or_empty_schema() {
        let driver = test_driver("custom", None);
        assert!(schema_switch_sql_for_driver(&driver, "public").is_none());
        assert!(schema_switch_sql_for_driver(&driver, "  ").is_none());
    }

    #[test]
    fn is_method_not_found_matches_not_supported() {
        assert!(is_method_not_found(&DbError::NotSupported("missing".into())));
        assert!(!is_method_not_found(&DbError::query("boom")));
    }

    #[test]
    fn close_on_release_follows_manifest() {
        let mut driver = test_driver("sf", None);
        driver.connection.close_on_release = true;
        let config = DbConnectionConfig {
            id: "1".into(),
            database_type: DatabaseType::External,
            name: "sf".into(),
            host: "/tmp/a.db".into(),
            port: 0,
            username: String::new(),
            password: String::new(),
            database: None,
            service_name: None,
            sid: None,
            credential_ref: None,
            ssh_tunnel_credential_ref: None,
            workspace_id: None,
            extra_params: Default::default(),
        };
        let conn = ExternalDbConnection::new(config, driver);
        assert!(conn.close_on_release());
    }
}
