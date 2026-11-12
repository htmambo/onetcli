use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;

use super::format_import_table_reference;
use crate::connection::DbConnection;
use crate::executor::{ExecOptions, SqlResult};
use crate::import_export::{
    ExportConfig, ExportProgressEvent, ExportProgressSender, ExportResult, FormatHandler,
    ImportConfig, ImportProgressEvent, ImportProgressSender, ImportResult,
};
use crate::{DatabasePlugin, DbError, SqlSource};

pub struct SqlFormatHandler;

/// 开启 DROP 时在首块输出的警示注释。
const DROP_WARNING: &str = "-- DROP TABLE enabled: replaying this file replaces existing tables; \
tables referenced by foreign keys may block replay.\n";

/// 结构块输出器：用"已输出的块"而非"遍历到的表"判定首块，
/// 保证首表结构为空时警示注释顺延到实际写出的第一块。
struct StructureBlockBuilder {
    emitted: bool,
}

impl StructureBlockBuilder {
    fn new() -> Self {
        Self { emitted: false }
    }

    fn next(
        &mut self,
        plugin: &dyn DatabasePlugin,
        config: &ExportConfig,
        table: &str,
        schema_sql: &str,
    ) -> String {
        let block = build_structure_block(plugin, config, table, schema_sql, !self.emitted);
        if !block.is_empty() {
            self.emitted = true;
        }
        block
    }
}

fn build_structure_block(
    plugin: &dyn DatabasePlugin,
    config: &ExportConfig,
    table: &str,
    schema_sql: &str,
    is_first_emitted: bool,
) -> String {
    if schema_sql.is_empty() {
        // 没有 CREATE 时不输出 DROP，避免"只删不建"。
        return String::new();
    }

    let include_drop = config.include_schema && config.drop_before_create;
    let mut output = String::new();
    if include_drop && is_first_emitted {
        output.push_str(DROP_WARNING);
    }
    output.push_str("-- Table structure for ");
    output.push_str(table);
    output.push('\n');

    if include_drop {
        // 与导出 CREATE 使用同一表名限定，避免跨库回放误删其他库的表。
        output.push_str("DROP TABLE IF EXISTS ");
        output.push_str(&plugin.format_export_table_reference(
            &config.database,
            config.schema.as_deref(),
            table,
        ));
        output.push_str(";\n");
    }

    output.push_str(schema_sql);
    output.push_str(";\n\n");
    output
}

#[async_trait]
impl FormatHandler for SqlFormatHandler {
    async fn import(
        &self,
        plugin: &dyn DatabasePlugin,
        connection: &dyn DbConnection,
        config: &ImportConfig,
        data: &str,
    ) -> Result<ImportResult> {
        self.import_with_progress(plugin, connection, config, data, "", None)
            .await
    }

    async fn import_with_progress(
        &self,
        plugin: &dyn DatabasePlugin,
        connection: &dyn DbConnection,
        config: &ImportConfig,
        data: &str,
        file_name: &str,
        progress_tx: Option<ImportProgressSender>,
    ) -> Result<ImportResult> {
        let start = Instant::now();
        let mut errors = Vec::new();
        let mut total_rows = 0u64;

        let send_progress = |event: ImportProgressEvent| {
            if let Some(tx) = &progress_tx {
                let _ = tx.send(event);
            }
        };

        send_progress(ImportProgressEvent::ParsingFile {
            file: file_name.to_string(),
        });

        let mut transactional_prefix = Vec::new();
        if config.truncate_before_import {
            if let Some(table) = &config.table {
                let table_ref = format_import_table_reference(plugin, config, table);
                let truncate_sql = format!("TRUNCATE TABLE {}", table_ref);
                if config.use_transaction {
                    transactional_prefix.push(truncate_sql);
                } else {
                    send_progress(ImportProgressEvent::ExecutingStatement {
                        file: file_name.to_string(),
                        statement_index: 0,
                        total_statements: 1,
                    });

                    let results = connection
                        .execute(plugin, &truncate_sql, ExecOptions::default())
                        .await
                        .map_err(|e| anyhow::anyhow!("Truncate failed: {}", e))?;

                    for result in results {
                        if let SqlResult::Error(err) = result {
                            let error_msg = format!("Truncate failed: {}", err.message);
                            errors.push(error_msg.clone());
                            send_progress(ImportProgressEvent::Error {
                                file: file_name.to_string(),
                                message: error_msg,
                            });
                            if config.stop_on_error {
                                send_progress(ImportProgressEvent::Finished {
                                    total_rows: 0,
                                    elapsed_ms: start.elapsed().as_millis(),
                                });
                                return Ok(ImportResult {
                                    success: false,
                                    rows_imported: 0,
                                    errors,
                                    elapsed_ms: start.elapsed().as_millis(),
                                });
                            }
                        }
                    }
                }
            }
        }

        let parser = plugin
            .create_parser(SqlSource::Script(data.to_string()))
            .map_err(|e| DbError::query(format!("Failed to create parser: {}", e)))?;
        let mut statements: Vec<String> = parser
            .filter_map(|r| r.ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !transactional_prefix.is_empty() {
            let mut prefixed = transactional_prefix;
            prefixed.extend(statements);
            statements = prefixed;
        }
        let total_statements = statements.len();

        if config.use_transaction {
            let exec_options = ExecOptions {
                stop_on_error: config.stop_on_error,
                transactional: true,
                max_rows: None,
                streaming: false,
            };
            let script = statements.join(";\n");

            match connection.execute(plugin, &script, exec_options).await {
                Ok(results) => {
                    for (idx, result) in results.into_iter().enumerate() {
                        send_progress(ImportProgressEvent::ExecutingStatement {
                            file: file_name.to_string(),
                            statement_index: idx,
                            total_statements,
                        });

                        match result {
                            SqlResult::Exec(exec_result) => {
                                total_rows += exec_result.rows_affected;
                                send_progress(ImportProgressEvent::StatementExecuted {
                                    file: file_name.to_string(),
                                    rows_affected: exec_result.rows_affected,
                                });
                            }
                            SqlResult::Error(err) => {
                                let error_msg = err.message.clone();
                                errors.push(error_msg.clone());
                                send_progress(ImportProgressEvent::Error {
                                    file: file_name.to_string(),
                                    message: error_msg,
                                });
                                if config.stop_on_error {
                                    send_progress(ImportProgressEvent::Finished {
                                        total_rows,
                                        elapsed_ms: start.elapsed().as_millis(),
                                    });
                                    return Ok(ImportResult {
                                        success: false,
                                        rows_imported: total_rows,
                                        errors,
                                        elapsed_ms: start.elapsed().as_millis(),
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    errors.push(error_msg.clone());
                    send_progress(ImportProgressEvent::Error {
                        file: file_name.to_string(),
                        message: error_msg,
                    });
                    if config.stop_on_error {
                        send_progress(ImportProgressEvent::Finished {
                            total_rows,
                            elapsed_ms: start.elapsed().as_millis(),
                        });
                        return Ok(ImportResult {
                            success: false,
                            rows_imported: total_rows,
                            errors,
                            elapsed_ms: start.elapsed().as_millis(),
                        });
                    }
                }
            }
        } else {
            for (idx, stmt) in statements.iter().enumerate() {
                let stmt = stmt.trim();
                if stmt.is_empty() {
                    continue;
                }

                send_progress(ImportProgressEvent::ExecutingStatement {
                    file: file_name.to_string(),
                    statement_index: idx,
                    total_statements,
                });

                let exec_options = ExecOptions {
                    stop_on_error: config.stop_on_error,
                    transactional: false,
                    max_rows: None,
                    streaming: false,
                };

                match connection.execute(plugin, stmt, exec_options).await {
                    Ok(results) => {
                        for result in results {
                            match result {
                                SqlResult::Exec(exec_result) => {
                                    total_rows += exec_result.rows_affected;
                                    send_progress(ImportProgressEvent::StatementExecuted {
                                        file: file_name.to_string(),
                                        rows_affected: exec_result.rows_affected,
                                    });
                                }
                                SqlResult::Error(err) => {
                                    let error_msg = err.message.clone();
                                    errors.push(error_msg.clone());
                                    send_progress(ImportProgressEvent::Error {
                                        file: file_name.to_string(),
                                        message: error_msg,
                                    });
                                    if config.stop_on_error {
                                        send_progress(ImportProgressEvent::Finished {
                                            total_rows,
                                            elapsed_ms: start.elapsed().as_millis(),
                                        });
                                        return Ok(ImportResult {
                                            success: false,
                                            rows_imported: total_rows,
                                            errors,
                                            elapsed_ms: start.elapsed().as_millis(),
                                        });
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(e) => {
                        let error_msg = e.to_string();
                        errors.push(error_msg.clone());
                        send_progress(ImportProgressEvent::Error {
                            file: file_name.to_string(),
                            message: error_msg,
                        });
                        if config.stop_on_error {
                            send_progress(ImportProgressEvent::Finished {
                                total_rows,
                                elapsed_ms: start.elapsed().as_millis(),
                            });
                            return Ok(ImportResult {
                                success: false,
                                rows_imported: total_rows,
                                errors,
                                elapsed_ms: start.elapsed().as_millis(),
                            });
                        }
                    }
                }
            }
        }

        let elapsed_ms = start.elapsed().as_millis();
        send_progress(ImportProgressEvent::FileFinished {
            file: file_name.to_string(),
            rows_imported: total_rows,
        });

        Ok(ImportResult {
            success: errors.is_empty(),
            rows_imported: total_rows,
            errors,
            elapsed_ms,
        })
    }

    async fn export(
        &self,
        plugin: &dyn DatabasePlugin,
        connection: &dyn DbConnection,
        config: &ExportConfig,
    ) -> Result<ExportResult> {
        self.export_with_progress(plugin, connection, config, None)
            .await
    }

    async fn export_with_progress(
        &self,
        plugin: &dyn DatabasePlugin,
        connection: &dyn DbConnection,
        config: &ExportConfig,
        progress_tx: Option<ExportProgressSender>,
    ) -> Result<ExportResult> {
        let start = Instant::now();
        let mut output = String::new();
        let mut total_rows = 0u64;
        let total_tables = config.tables.len();
        let is_streaming = progress_tx.is_some();

        let send_progress = |event: ExportProgressEvent| {
            if let Some(tx) = &progress_tx {
                let _ = tx.send(event);
            }
        };
        let mut structure_blocks = StructureBlockBuilder::new();

        for (index, table) in config.tables.iter().enumerate() {
            send_progress(ExportProgressEvent::TableStart {
                table: table.clone(),
                table_index: index,
                total_tables,
            });

            if config.include_schema {
                send_progress(ExportProgressEvent::GettingStructure {
                    table: table.clone(),
                });

                match plugin
                    .export_table_create_sql(
                        connection,
                        &config.database,
                        config.schema.as_deref(),
                        table,
                    )
                    .await
                {
                    Ok(schema_sql) => {
                        let mut schema_output =
                            structure_blocks.next(plugin, config, table, &schema_sql);
                        let progress_data = if is_streaming {
                            std::mem::take(&mut schema_output)
                        } else {
                            schema_output.clone()
                        };
                        send_progress(ExportProgressEvent::StructureExported {
                            table: table.clone(),
                            data: progress_data,
                        });
                        if !is_streaming {
                            output.push_str(&schema_output);
                        }
                    }
                    Err(e) => {
                        let error_output =
                            format!("-- Failed to export structure for {}: {}\n\n", table, e);
                        if !is_streaming {
                            output.push_str(&error_output);
                        }
                        send_progress(ExportProgressEvent::Error {
                            table: table.clone(),
                            message: format!("Failed to export structure: {}", e),
                        });
                    }
                }
            }

            if config.include_data {
                send_progress(ExportProgressEvent::FetchingData {
                    table: table.clone(),
                });

                match plugin
                    .export_table_data_sql(
                        connection,
                        &config.database,
                        config.schema.as_deref(),
                        table,
                        config.where_clause.as_deref(),
                        config.limit,
                    )
                    .await
                {
                    Ok(data_sql) => {
                        let mut data_output = String::new();
                        let rows_count = if !data_sql.is_empty() {
                            data_output.push_str("-- Data for table ");
                            data_output.push_str(table);
                            data_output.push('\n');
                            data_output.push_str(&data_sql);
                            data_output.push('\n');
                            data_sql.lines().filter(|l| l.starts_with("INSERT")).count() as u64
                        } else {
                            0
                        };
                        total_rows += rows_count;
                        let progress_data = if is_streaming {
                            std::mem::take(&mut data_output)
                        } else {
                            data_output.clone()
                        };
                        send_progress(ExportProgressEvent::DataExported {
                            table: table.clone(),
                            rows: rows_count,
                            data: progress_data,
                        });
                        if !is_streaming {
                            output.push_str(&data_output);
                        }
                    }
                    Err(e) => {
                        let error_output =
                            format!("-- Failed to export data for {}: {}\n\n", table, e);
                        if !is_streaming {
                            output.push_str(&error_output);
                        }
                        send_progress(ExportProgressEvent::Error {
                            table: table.clone(),
                            message: format!("Failed to export data: {}", e),
                        });
                    }
                }
            }

            send_progress(ExportProgressEvent::TableFinished {
                table: table.clone(),
            });
        }

        let elapsed_ms = start.elapsed().as_millis();
        send_progress(ExportProgressEvent::Finished {
            total_rows,
            elapsed_ms,
        });

        Ok(ExportResult {
            success: true,
            output,
            rows_exported: total_rows,
            elapsed_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{DROP_WARNING, StructureBlockBuilder, build_structure_block};
    use crate::import_export::ExportConfig;
    use crate::mysql::MySqlPlugin;
    use crate::postgresql::PostgresPlugin;
    use crate::{DataFormat, DatabasePlugin};

    fn config(
        include_schema: bool,
        drop_before_create: bool,
        schema: Option<&str>,
    ) -> ExportConfig {
        ExportConfig {
            format: DataFormat::Sql,
            database: "shop".to_string(),
            schema: schema.map(str::to_string),
            tables: vec!["users".to_string()],
            columns: None,
            include_schema,
            include_data: true,
            drop_before_create,
            where_clause: None,
            limit: None,
            csv_config: None,
        }
    }

    #[test]
    fn drop_disabled_keeps_current_output() {
        let plugin = MySqlPlugin::new();
        let block = build_structure_block(
            &plugin,
            &config(true, false, None),
            "users",
            "CREATE TABLE `users` (`id` INT)",
            true,
        );

        assert_eq!(
            block,
            "-- Table structure for users\nCREATE TABLE `users` (`id` INT);\n\n"
        );

        // 空 CREATE 在关闭开关时与旧实现一致：整块输出为空。
        let empty = build_structure_block(&plugin, &config(true, false, None), "users", "", true);
        assert!(empty.is_empty());
    }

    #[test]
    fn drop_enabled_uses_unqualified_mysql_reference() {
        let plugin = MySqlPlugin::new();
        let block = build_structure_block(
            &plugin,
            &config(true, true, None),
            "users",
            "CREATE TABLE `users` (`id` INT)",
            false,
        );

        assert_eq!(
            block,
            "-- Table structure for users\nDROP TABLE IF EXISTS `users`;\nCREATE TABLE `users` (`id` INT);\n\n"
        );
    }

    #[test]
    fn drop_reference_matches_postgres_create_reference() {
        let plugin = PostgresPlugin::new();
        let table_ref = plugin.format_export_table_reference("shop", Some("public"), "users");
        let create_sql = format!("CREATE TABLE {table_ref} (id INT)");
        let block = build_structure_block(
            &plugin,
            &config(true, true, Some("public")),
            "users",
            &create_sql,
            false,
        );

        assert!(block.contains(&format!("DROP TABLE IF EXISTS {table_ref};")));
        assert!(block.contains(&format!("CREATE TABLE {table_ref}")));
    }

    #[test]
    fn empty_create_never_emits_drop() {
        let plugin = MySqlPlugin::new();
        let block = build_structure_block(&plugin, &config(true, true, None), "users", "", true);

        assert!(block.is_empty());
    }

    #[test]
    fn data_only_never_emits_drop() {
        let plugin = MySqlPlugin::new();
        let block = build_structure_block(
            &plugin,
            &config(false, true, None),
            "users",
            "CREATE TABLE `users` (`id` INT)",
            true,
        );

        assert!(!block.contains("DROP TABLE"));
        assert!(!block.contains(DROP_WARNING));
    }

    #[test]
    fn warning_only_on_first_emitted_block() {
        let plugin = MySqlPlugin::new();
        let config = config(true, true, None);
        let mut builder = StructureBlockBuilder::new();

        // 首表结构为空时整块跳过，警示注释顺延到下一块。
        assert!(builder.next(&plugin, &config, "broken", "").is_empty());

        let second = builder.next(&plugin, &config, "users", "CREATE TABLE `users` (`id` INT)");
        assert!(second.starts_with(DROP_WARNING));
        assert_eq!(second.matches("-- DROP TABLE enabled").count(), 1);

        let third = builder.next(
            &plugin,
            &config,
            "orders",
            "CREATE TABLE `orders` (`id` INT)",
        );
        assert!(!third.contains("-- DROP TABLE enabled"));
        assert!(third.contains("DROP TABLE IF EXISTS `orders`;"));
    }
}
