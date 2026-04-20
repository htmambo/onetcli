use crate::connection::DbConnection;
use crate::executor::{ExecOptions, SqlResult};
use crate::import_export::{ExportConfig, ImportConfig};
use crate::DatabasePlugin;
use anyhow::{anyhow, Result};

pub mod csv;
pub mod json;
pub mod sql;
pub mod txt;
pub mod xml;

pub use csv::CsvFormatHandler;
pub use json::JsonFormatHandler;
pub use sql::SqlFormatHandler;
pub use txt::TxtFormatHandler;
pub use xml::XmlFormatHandler;

pub(super) fn format_import_table_reference(
    plugin: &dyn DatabasePlugin,
    config: &ImportConfig,
    table: &str,
) -> String {
    plugin.format_table_reference(&config.database, config.schema.as_deref(), table)
}

pub(super) fn build_export_select_sql(
    plugin: &dyn DatabasePlugin,
    config: &ExportConfig,
    table: &str,
) -> String {
    let table_ref =
        plugin.format_table_reference(&config.database, config.schema.as_deref(), table);
    let columns_str = if let Some(cols) = &config.columns {
        cols.iter()
            .map(|c| plugin.quote_identifier(c))
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        "*".to_string()
    };

    let mut select_sql = format!("SELECT {} FROM {}", columns_str, table_ref);
    if let Some(where_clause) = &config.where_clause {
        select_sql.push_str(" WHERE ");
        select_sql.push_str(where_clause);
    }
    if let Some(limit) = config.limit {
        let pagination = plugin.format_pagination(limit, 0, "");
        select_sql.push_str(&pagination);
    }
    select_sql
}

pub(super) fn build_insert_statement(
    plugin: &dyn DatabasePlugin,
    table_ref: &str,
    columns: &[String],
    sql_values: &[String],
) -> String {
    let mut sql = format!("INSERT INTO {} (", table_ref);
    for (i, col) in columns.iter().enumerate() {
        if i > 0 {
            sql.push_str(", ");
        }
        sql.push_str(&plugin.quote_identifier(col));
    }
    sql.push_str(") VALUES (");
    for (i, value) in sql_values.iter().enumerate() {
        if i > 0 {
            sql.push_str(", ");
        }
        sql.push_str(value);
    }
    sql.push(')');
    sql
}

pub(super) fn quote_sql_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub(super) async fn execute_import_statements(
    plugin: &dyn DatabasePlugin,
    connection: &dyn DbConnection,
    config: &ImportConfig,
    statements: &[String],
) -> Result<Vec<SqlResult>> {
    if statements.is_empty() {
        return Ok(Vec::new());
    }

    let script = statements.join(";\n");
    let options = ExecOptions {
        stop_on_error: config.stop_on_error,
        transactional: config.use_transaction,
        max_rows: None,
        streaming: false,
    };

    connection
        .execute(plugin, &script, options)
        .await
        .map_err(|e| anyhow!("Import failed: {}", e))
}

#[cfg(test)]
mod tests {
    use super::{build_export_select_sql, format_import_table_reference};
    use crate::import_export::{ExportConfig, ImportConfig};
    use crate::mssql::MsSqlPlugin;
    use crate::mysql::MySqlPlugin;

    #[test]
    fn test_format_import_table_reference_uses_database_for_mysql() {
        let plugin = MySqlPlugin::new();
        let config = ImportConfig {
            database: "analytics".to_string(),
            table: Some("orders".to_string()),
            ..ImportConfig::default()
        };

        let table_ref = format_import_table_reference(&plugin, &config, "orders");

        assert_eq!(table_ref, "`analytics`.`orders`");
    }

    #[test]
    fn test_format_import_table_reference_uses_schema_for_mssql() {
        let plugin = MsSqlPlugin::new();
        let config = ImportConfig {
            database: "warehouse".to_string(),
            schema: Some("sales".to_string()),
            table: Some("orders".to_string()),
            ..ImportConfig::default()
        };

        let table_ref = format_import_table_reference(&plugin, &config, "orders");

        assert_eq!(table_ref, "[warehouse].[sales].[orders]");
    }

    #[test]
    fn test_build_export_select_sql_keeps_schema_where_and_limit() {
        let plugin = MsSqlPlugin::new();
        let config = ExportConfig {
            database: "warehouse".to_string(),
            schema: Some("sales".to_string()),
            tables: vec!["orders".to_string()],
            columns: Some(vec!["id".to_string(), "name".to_string()]),
            where_clause: Some("status = 1".to_string()),
            limit: Some(10),
            ..ExportConfig::default()
        };

        let sql = build_export_select_sql(&plugin, &config, "orders");

        assert_eq!(
            sql,
            "SELECT [id], [name] FROM [warehouse].[sales].[orders] WHERE status = 1 ORDER BY (SELECT NULL) OFFSET 0 ROWS FETCH NEXT 10 ROWS ONLY"
        );
    }
}
