use crate::connection::{DbConnection, DbError};
use crate::executor::{QueryResult, SqlResult};
use crate::import_export::{
    ExportConfig, ExportProgressSender, ExportResult, ImportConfig, ImportProgressSender,
    ImportResult,
};
use crate::ipc::connection::ExternalDbConnection;
use crate::ipc::protocol::{database_metadata_params, table_metadata_params};
use crate::ipc::registry::{EXTERNAL_DRIVER_ID_PARAM, IpcDriverDialect, IpcDriverManifest, IpcDriverRegistry, TableReferenceSchemaMode};
use crate::oracle::OraclePlugin;
use crate::plugin::{ConnectionLifecycle, DatabasePlugin, SqlCompletionInfo};
use crate::plugin_manifest::{DatabaseCapabilities, DatabaseUiCapabilities, DatabaseUiManifest};
use crate::types::*;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use one_core::storage::{DatabaseType, DbConnectionConfig};
use gpui::{TextAlign, px};
use sqlparser::dialect::{Dialect, GenericDialect};

#[derive(Clone)]
pub struct ExternalDatabasePlugin {
    registry: IpcDriverRegistry,
}

impl ExternalDatabasePlugin {
    pub fn new() -> Self {
        Self {
            registry: IpcDriverRegistry::load_default(),
        }
    }

    fn driver_for_config(&self, config: &DbConnectionConfig) -> Result<IpcDriverManifest, DbError> {
        let driver_id = config
            .get_param(EXTERNAL_DRIVER_ID_PARAM)
            .ok_or_else(|| DbError::connection("external driver id is required"))?;
        self.registry.find(driver_id).ok_or_else(|| {
            DbError::connection(format!("external driver '{}' not found", driver_id))
        })
    }

    fn driver_for_id(&self, driver_id: &str) -> Option<IpcDriverManifest> {
        self.registry.find(driver_id)
    }

    fn is_oracle_compatible_driver(driver: &IpcDriverManifest) -> bool {
        matches!(
            driver.dialect.compatible_database_type,
            Some(DatabaseType::Oracle)
        ) || {
            let id = driver.id.to_ascii_lowercase();
            id.contains("oracle") || id == "dm" || id.contains("dameng")
        }
    }

    fn oracle_table_save_request(
        driver: &IpcDriverManifest,
        request: &TableSaveRequest,
    ) -> TableSaveRequest {
        let mut request = request.clone();
        let uses_schema_as_database = driver.dialect.uses_schema_as_database
            || driver.effective_capabilities().uses_schema_as_database;
        if request.schema.is_none()
            && uses_schema_as_database
            && !request.database.trim().is_empty()
        {
            request.schema = Some(request.database.clone());
        }
        request
    }

    fn with_oracle_copy_sql<F>(&self, request: &CopySqlRequest, f: F) -> Option<String>
    where
        F: FnOnce(&OraclePlugin, &CopySqlRequest) -> String,
    {
        let driver_id = request.driver_id.as_deref()?;
        let driver = self.driver_for_id(driver_id)?;
        if !Self::is_oracle_compatible_driver(&driver) {
            return None;
        }
        let oracle_plugin = OraclePlugin::new();
        Some(f(&oracle_plugin, request))
    }

    fn generate_default_table_changes_sql(&self, request: &TableSaveRequest) -> String {
        let mut sql_statements = Vec::new();
        for change in &request.changes {
            if let Some(sql) = self.build_table_change_sql(request, change) {
                sql_statements.push(sql);
            }
        }
        if sql_statements.is_empty() {
            rust_i18n::t!("Error.no_changes").to_string()
        } else {
            sql_statements.join(";\n\n") + ";"
        }
    }

    async fn custom_object_view(
        &self,
        connection: &dyn DbConnection,
        view: ObjectViewKind,
        db_node_type: DbNodeType,
        default_title: &str,
        scope: ObjectViewScope<'_>,
    ) -> Result<Option<ObjectView>> {
        let mut params = serde_json::Map::new();
        params.insert("view".to_string(), serde_json::json!(view.as_str()));
        if let Some(database) = scope.database {
            params.insert("database".to_string(), serde_json::json!(database));
        }
        if let Some(schema) = scope.schema {
            params.insert("schema".to_string(), serde_json::json!(schema));
        }
        if let Some(table) = scope.table {
            params.insert("table".to_string(), serde_json::json!(table));
        }

        let view = self
            .optional_metadata::<DriverObjectView>(
                connection,
                "metadata.object_view",
                serde_json::Value::Object(params),
            )
            .await?;

        Ok(view.and_then(|view| object_view_from_driver(db_node_type, default_title, view)))
    }

    async fn metadata<T>(
        &self,
        connection: &dyn DbConnection,
        method: &str,
        params: serde_json::Value,
    ) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let sql = serde_json::json!({ "method": method, "params": params }).to_string();
        match connection
            .query(&format!("/*omnihub-ipc-metadata*/ {sql}"))
            .await?
        {
            SqlResult::Query(query) => decode_single_cell(query),
            SqlResult::Error(error) => Err(anyhow!(error.message)),
            SqlResult::Exec(_) => Err(anyhow!(
                "external driver returned non-query metadata result"
            )),
        }
    }

    async fn optional_metadata<T>(
        &self,
        connection: &dyn DbConnection,
        method: &str,
        params: serde_json::Value,
    ) -> Result<Option<T>>
    where
        T: serde::de::DeserializeOwned,
    {
        match self.metadata(connection, method, params).await {
            Ok(value) => Ok(Some(value)),
            Err(error) if is_not_supported(&error) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

impl Default for ExternalDatabasePlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DatabasePlugin for ExternalDatabasePlugin {
    fn name(&self) -> DatabaseType {
        DatabaseType::External
    }

    fn quote_identifier(&self, identifier: &str) -> String {
        // 无连接上下文时使用默认双引号；query_table_data 使用驱动 dialect。
        format!("\"{}\"", identifier.replace('"', "\"\""))
    }

    fn format_table_reference(&self, database: &str, schema: Option<&str>, table: &str) -> String {
        // 多驱动 registry 无单驱动上下文：schema 优先（兼容 PreferSchema / PG-like）。
        IpcDriverDialect {
            table_reference_schema_mode: TableReferenceSchemaMode::PreferSchema,
            supports_schema: true,
            ..IpcDriverDialect::default()
        }
        .format_table_reference(database, schema, table)
    }

    fn get_completion_info(&self) -> SqlCompletionInfo {
        SqlCompletionInfo::default().with_standard_sql()
    }

    async fn create_connection(
        &self,
        config: DbConnectionConfig,
    ) -> Result<Box<dyn DbConnection + Send + Sync>, DbError> {
        let driver = self.driver_for_config(&config)?;
        Ok(Box::new(ExternalDbConnection::new(config, driver)))
    }

    fn connection_lifecycle(&self, config: &DbConnectionConfig) -> ConnectionLifecycle {
        let Ok(driver) = self.driver_for_config(config) else {
            return ConnectionLifecycle::default();
        };

        let close_on_release = driver.connection.close_on_release;
        let physical_open_lock_key =
            if driver.connection.single_file && driver.connection.single_connection {
                ConnectionLifecycle::single_file(
                    &driver.id,
                    config,
                    &driver.connection.path_fields,
                )
                .physical_open_lock_key
            } else {
                None
            };

        ConnectionLifecycle {
            close_on_release,
            physical_open_lock_key,
        }
    }

    async fn query_table_data(
        &self,
        connection: &dyn DbConnection,
        request: TableDataRequest,
    ) -> Result<TableDataResponse> {
        let start_time = std::time::Instant::now();
        let driver = self.driver_for_config(connection.config())?;
        let dialect = &driver.dialect;

        let where_clause = match request.where_clause {
            Some(ref clause) if !clause.trim().is_empty() => format!(" WHERE {}", clause.trim()),
            _ => String::new(),
        };
        let mut order_clause = match request.order_by_clause {
            Some(ref clause) if !clause.trim().is_empty() => format!(" ORDER BY {}", clause.trim()),
            _ => String::new(),
        };
        if order_clause.is_empty() {
            if let Some(default_order_by) = dialect
                .default_order_by
                .as_deref()
                .filter(|order_by| !order_by.trim().is_empty())
            {
                order_clause = format!(" ORDER BY {}", default_order_by.trim());
            }
        }

        let offset = (request.page.saturating_sub(1)) * request.page_size;
        let table_ref = dialect.format_table_reference(
            &request.database,
            request.schema.as_deref(),
            &request.table,
        );

        let count_sql = format!("SELECT COUNT(*) FROM {}{}", table_ref, where_clause);
        let total_count = match connection.query(&count_sql).await? {
            SqlResult::Query(result) => result
                .rows
                .first()
                .and_then(|row| row.first())
                .and_then(|value| value.as_ref())
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0),
            _ => 0,
        };

        let pagination = dialect.format_pagination(request.page_size, offset, &order_clause);
        let data_sql = if let Some(row_id_column) = dialect
            .row_id_column
            .as_deref()
            .filter(|column| !column.trim().is_empty())
        {
            let row_id_alias = dialect
                .row_id_alias
                .as_deref()
                .filter(|alias| !alias.trim().is_empty())
                .unwrap_or("__rowid__");
            format!(
                "SELECT {} AS {}, t.* FROM {} t{}{}{}",
                row_id_column.trim(),
                dialect.quote_identifier(row_id_alias.trim()),
                table_ref,
                where_clause,
                order_clause,
                pagination
            )
        } else {
            format!(
                "SELECT * FROM {}{}{}{}",
                table_ref, where_clause, order_clause, pagination
            )
        };

        let sql_result = connection.query(&data_sql).await?;
        let duration = start_time.elapsed().as_millis();
        let query_result = match sql_result {
            SqlResult::Query(query_result) => Ok::<QueryResult, anyhow::Error>(query_result),
            SqlResult::Exec(_) => Err(anyhow!("query type error")),
            SqlResult::Error(sql_error_info) => Err(anyhow!(sql_error_info.message)),
        }?;

        Ok(TableDataResponse {
            query_result,
            total_count,
            page: request.page,
            page_size: request.page_size,
            duration,
        })
    }

    async fn list_databases(&self, connection: &dyn DbConnection) -> Result<Vec<String>> {
        self.metadata(connection, "metadata.list_databases", serde_json::json!({}))
            .await
    }

    async fn list_databases_view(&self, connection: &dyn DbConnection) -> Result<ObjectView> {
        if let Some(view) = self
            .custom_object_view(
                connection,
                ObjectViewKind::Databases,
                DbNodeType::Database,
                "Databases",
                ObjectViewScope::default(),
            )
            .await?
        {
            return Ok(view);
        }

        let rows = self
            .list_databases_detailed(connection)
            .await?
            .into_iter()
            .map(|db| vec![db.name, db.comment.unwrap_or_default()])
            .collect();
        Ok(object_view(
            DbNodeType::Database,
            "Databases",
            vec!["Name", "Comment"],
            rows,
        ))
    }

    async fn list_databases_detailed(
        &self,
        connection: &dyn DbConnection,
    ) -> Result<Vec<DatabaseInfo>> {
        match self
            .metadata(
                connection,
                "metadata.list_databases_detailed",
                serde_json::json!({}),
            )
            .await
        {
            Ok(databases) => Ok(databases),
            Err(error) if is_not_supported(&error) => {
                Ok(names_to_databases(self.list_databases(connection).await?))
            }
            Err(error) => Err(error),
        }
    }

    fn capabilities(&self) -> DatabaseCapabilities {
        merge_capabilities(
            self.registry
                .drivers()
                .iter()
                .map(IpcDriverManifest::effective_capabilities),
        )
    }

    fn sql_dialect(&self) -> Box<dyn Dialect> {
        Box::new(GenericDialect {})
    }

    async fn list_schemas(
        &self,
        connection: &dyn DbConnection,
        database: &str,
    ) -> Result<Vec<String>> {
        self.metadata(
            connection,
            "metadata.list_schemas",
            database_metadata_params(database, None),
        )
        .await
    }

    async fn list_schemas_view(
        &self,
        connection: &dyn DbConnection,
        database: &str,
    ) -> Result<ObjectView> {
        if let Some(view) = self
            .custom_object_view(
                connection,
                ObjectViewKind::Schemas,
                DbNodeType::Schema,
                "Schemas",
                ObjectViewScope {
                    database: Some(database),
                    ..Default::default()
                },
            )
            .await?
        {
            return Ok(view);
        }

        let rows = self
            .list_schemas(connection, database)
            .await?
            .into_iter()
            .map(|schema| vec![schema])
            .collect();
        Ok(object_view(
            DbNodeType::Schema,
            "Schemas",
            vec!["Name"],
            rows,
        ))
    }

    async fn list_tables(
        &self,
        connection: &dyn DbConnection,
        database: &str,
        schema: Option<String>,
    ) -> Result<Vec<TableInfo>> {
        self.metadata(
            connection,
            "metadata.list_tables",
            database_metadata_params(database, schema),
        )
        .await
    }

    async fn list_tables_view(
        &self,
        connection: &dyn DbConnection,
        database: &str,
        schema: Option<String>,
    ) -> Result<ObjectView> {
        if let Some(view) = self
            .custom_object_view(
                connection,
                ObjectViewKind::Tables,
                DbNodeType::Table,
                "Tables",
                ObjectViewScope {
                    database: Some(database),
                    schema: schema.as_deref(),
                    table: None,
                },
            )
            .await?
        {
            return Ok(view);
        }

        let rows = self
            .list_tables(connection, database, schema)
            .await?
            .into_iter()
            .map(|table| vec![table.name, table.comment.unwrap_or_default()])
            .collect();
        Ok(object_view(
            DbNodeType::Table,
            "Tables",
            vec!["Name", "Comment"],
            rows,
        ))
    }

    async fn list_columns(
        &self,
        connection: &dyn DbConnection,
        database: &str,
        schema: Option<String>,
        table: &str,
    ) -> Result<Vec<ColumnInfo>> {
        self.metadata(
            connection,
            "metadata.list_columns",
            table_metadata_params(database, schema, table),
        )
        .await
    }

    async fn list_columns_view(
        &self,
        connection: &dyn DbConnection,
        database: &str,
        schema: Option<String>,
        table: &str,
    ) -> Result<ObjectView> {
        if let Some(view) = self
            .custom_object_view(
                connection,
                ObjectViewKind::Columns,
                DbNodeType::Column,
                "Columns",
                ObjectViewScope {
                    database: Some(database),
                    schema: schema.as_deref(),
                    table: Some(table),
                },
            )
            .await?
        {
            return Ok(view);
        }

        let rows = self
            .list_columns(connection, database, schema, table)
            .await?
            .into_iter()
            .map(|col| vec![col.name, col.data_type, col.is_nullable.to_string()])
            .collect();
        Ok(object_view(
            DbNodeType::Column,
            "Columns",
            vec!["Name", "Type", "Nullable"],
            rows,
        ))
    }

    async fn list_indexes(
        &self,
        connection: &dyn DbConnection,
        database: &str,
        schema: Option<String>,
        table: &str,
    ) -> Result<Vec<IndexInfo>> {
        self.metadata(
            connection,
            "metadata.list_indexes",
            table_metadata_params(database, schema, table),
        )
        .await
    }

    async fn list_indexes_view(
        &self,
        connection: &dyn DbConnection,
        database: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Result<ObjectView> {
        if let Some(view) = self
            .custom_object_view(
                connection,
                ObjectViewKind::Indexes,
                DbNodeType::Index,
                "Indexes",
                ObjectViewScope {
                    database: Some(database),
                    schema,
                    table: Some(table),
                },
            )
            .await?
        {
            return Ok(view);
        }

        let rows = self
            .list_indexes(connection, database, schema.map(str::to_string), table)
            .await?
            .into_iter()
            .map(|idx| vec![idx.name, idx.columns.join(", "), idx.is_unique.to_string()])
            .collect();
        Ok(object_view(
            DbNodeType::Index,
            "Indexes",
            vec!["Name", "Columns", "Unique"],
            rows,
        ))
    }

    async fn list_views(
        &self,
        connection: &dyn DbConnection,
        database: &str,
        schema: Option<String>,
    ) -> Result<Vec<ViewInfo>> {
        self.metadata(
            connection,
            "metadata.list_views",
            database_metadata_params(database, schema),
        )
        .await
    }

    async fn list_views_view(
        &self,
        connection: &dyn DbConnection,
        database: &str,
    ) -> Result<ObjectView> {
        if let Some(view) = self
            .custom_object_view(
                connection,
                ObjectViewKind::Views,
                DbNodeType::View,
                "Views",
                ObjectViewScope {
                    database: Some(database),
                    ..Default::default()
                },
            )
            .await?
        {
            return Ok(view);
        }

        let rows = self
            .list_views(connection, database, None)
            .await?
            .into_iter()
            .map(|view| vec![view.name, view.comment.unwrap_or_default()])
            .collect();
        Ok(object_view(
            DbNodeType::View,
            "Views",
            vec!["Name", "Comment"],
            rows,
        ))
    }

    async fn list_foreign_keys(
        &self,
        connection: &dyn DbConnection,
        database: &str,
        schema: Option<String>,
        table: &str,
    ) -> Result<Vec<ForeignKeyDefinition>> {
        Ok(self
            .optional_metadata(
                connection,
                "metadata.list_foreign_keys",
                table_metadata_params(database, schema, table),
            )
            .await?
            .unwrap_or_default())
    }

    async fn list_table_triggers(
        &self,
        connection: &dyn DbConnection,
        database: &str,
        schema: Option<String>,
        table: &str,
    ) -> Result<Vec<TriggerInfo>> {
        let triggers = self
            .optional_metadata(
                connection,
                "metadata.list_table_triggers",
                table_metadata_params(database, schema, table),
            )
            .await?
            .unwrap_or_default();
        Ok(fill_trigger_table_names(triggers, table))
    }

    async fn list_table_checks(
        &self,
        connection: &dyn DbConnection,
        database: &str,
        schema: Option<String>,
        table: &str,
    ) -> Result<Vec<CheckInfo>> {
        let checks = self
            .optional_metadata(
                connection,
                "metadata.list_table_checks",
                table_metadata_params(database, schema, table),
            )
            .await?
            .unwrap_or_default();
        Ok(fill_check_table_names(checks, table))
    }

    async fn list_functions(
        &self,
        connection: &dyn DbConnection,
        database: &str,
    ) -> Result<Vec<FunctionInfo>> {
        Ok(self
            .optional_metadata(
                connection,
                "metadata.list_functions",
                database_metadata_params(database, None),
            )
            .await?
            .unwrap_or_default())
    }

    async fn list_functions_view(
        &self,
        connection: &dyn DbConnection,
        database: &str,
    ) -> Result<ObjectView> {
        if let Some(view) = self
            .custom_object_view(
                connection,
                ObjectViewKind::Functions,
                DbNodeType::Function,
                "Functions",
                ObjectViewScope {
                    database: Some(database),
                    ..Default::default()
                },
            )
            .await?
        {
            return Ok(view);
        }

        let rows = self
            .list_functions(connection, database)
            .await?
            .into_iter()
            .map(|function| vec![function.name, function.return_type.unwrap_or_default()])
            .collect();
        Ok(object_view(
            DbNodeType::Function,
            "Functions",
            vec!["Name", "Return Type"],
            rows,
        ))
    }

    fn ui_manifest(&self) -> DatabaseUiManifest {
        self.registry
            .drivers()
            .first()
            .and_then(|driver| driver.ui.form.clone())
            .unwrap_or_default()
    }

    async fn list_procedures(
        &self,
        connection: &dyn DbConnection,
        database: &str,
    ) -> Result<Vec<FunctionInfo>> {
        Ok(self
            .optional_metadata(
                connection,
                "metadata.list_procedures",
                database_metadata_params(database, None),
            )
            .await?
            .unwrap_or_default())
    }

    async fn list_procedures_view(
        &self,
        connection: &dyn DbConnection,
        database: &str,
    ) -> Result<ObjectView> {
        if let Some(view) = self
            .custom_object_view(
                connection,
                ObjectViewKind::Procedures,
                DbNodeType::Procedure,
                "Procedures",
                ObjectViewScope {
                    database: Some(database),
                    ..Default::default()
                },
            )
            .await?
        {
            return Ok(view);
        }

        let rows = self
            .list_procedures(connection, database)
            .await?
            .into_iter()
            .map(|procedure| vec![procedure.name, procedure.parameters.join(", ")])
            .collect();
        Ok(object_view(
            DbNodeType::Procedure,
            "Procedures",
            vec!["Name", "Parameters"],
            rows,
        ))
    }

    async fn list_triggers(
        &self,
        connection: &dyn DbConnection,
        database: &str,
    ) -> Result<Vec<TriggerInfo>> {
        Ok(self
            .optional_metadata(
                connection,
                "metadata.list_triggers",
                database_metadata_params(database, None),
            )
            .await?
            .unwrap_or_default())
    }

    async fn list_triggers_view(
        &self,
        connection: &dyn DbConnection,
        database: &str,
    ) -> Result<ObjectView> {
        if let Some(view) = self
            .custom_object_view(
                connection,
                ObjectViewKind::Triggers,
                DbNodeType::Trigger,
                "Triggers",
                ObjectViewScope {
                    database: Some(database),
                    ..Default::default()
                },
            )
            .await?
        {
            return Ok(view);
        }

        let rows = self
            .list_triggers(connection, database)
            .await?
            .into_iter()
            .map(|trigger| vec![trigger.name, trigger.table_name, trigger.event])
            .collect();
        Ok(object_view(
            DbNodeType::Trigger,
            "Triggers",
            vec!["Name", "Table", "Event"],
            rows,
        ))
    }

    async fn list_sequences(
        &self,
        connection: &dyn DbConnection,
        database: &str,
        schema: Option<String>,
    ) -> Result<Vec<SequenceInfo>> {
        Ok(self
            .optional_metadata(
                connection,
                "metadata.list_sequences",
                database_metadata_params(database, schema),
            )
            .await?
            .unwrap_or_default())
    }

    async fn list_sequences_view(
        &self,
        connection: &dyn DbConnection,
        database: &str,
    ) -> Result<ObjectView> {
        if let Some(view) = self
            .custom_object_view(
                connection,
                ObjectViewKind::Sequences,
                DbNodeType::Sequence,
                "Sequences",
                ObjectViewScope {
                    database: Some(database),
                    ..Default::default()
                },
            )
            .await?
        {
            return Ok(view);
        }

        let rows = self
            .list_sequences(connection, database, None)
            .await?
            .into_iter()
            .map(|sequence| {
                vec![
                    sequence.name,
                    sequence.increment.unwrap_or_default().to_string(),
                ]
            })
            .collect();
        Ok(object_view(
            DbNodeType::Sequence,
            "Sequences",
            vec!["Name", "Increment"],
            rows,
        ))
    }

    fn build_column_definition(&self, column: &ColumnInfo, include_name: bool) -> String {
        let nullable = if column.is_nullable { "" } else { " NOT NULL" };
        let default = column
            .default_value
            .as_ref()
            .map(|value| format!(" DEFAULT {value}"))
            .unwrap_or_default();
        let name = if include_name {
            format!("{} ", self.quote_identifier(&column.name))
        } else {
            String::new()
        };
        format!("{name}{}{nullable}{default}", column.data_type)
    }

    fn build_create_database_sql(
        &self,
        request: &crate::plugin::DatabaseOperationRequest,
    ) -> String {
        format!(
            "CREATE DATABASE {}",
            self.quote_identifier(&request.database_name)
        )
    }

    fn build_modify_database_sql(
        &self,
        request: &crate::plugin::DatabaseOperationRequest,
    ) -> String {
        format!(
            "ALTER DATABASE {}",
            self.quote_identifier(&request.database_name)
        )
    }

    fn build_drop_database_sql(&self, database_name: &str) -> String {
        format!("DROP DATABASE {}", self.quote_identifier(database_name))
    }

    fn build_limit_clause(&self) -> String {
        "LIMIT".to_string()
    }

    fn build_where_and_limit_clause(
        &self,
        request: &TableSaveRequest,
        original_data: &[String],
    ) -> (String, String) {
        (
            self.build_table_change_where_clause(request, original_data),
            String::new(),
        )
    }

    fn generate_table_changes_sql(&self, request: &TableSaveRequest) -> String {
        if let Some(driver_id) = request.driver_id.as_deref() {
            if let Some(driver) = self.driver_for_id(driver_id) {
                if Self::is_oracle_compatible_driver(&driver) {
                    let oracle_plugin = OraclePlugin::new();
                    return oracle_plugin.generate_table_changes_sql(
                        &Self::oracle_table_save_request(&driver, request),
                    );
                }
            }
        }
        self.generate_default_table_changes_sql(request)
    }

    fn generate_copy_insert_sql(&self, request: &CopySqlRequest) -> String {
        if let Some(sql) = self.with_oracle_copy_sql(request, |plugin, req| {
            plugin.generate_copy_insert_sql(req)
        }) {
            return sql;
        }
        crate::plugin::default_generate_copy_insert_sql(self, request)
    }

    fn generate_copy_insert_with_comments_sql(&self, request: &CopySqlRequest) -> String {
        if let Some(sql) = self.with_oracle_copy_sql(request, |plugin, req| {
            plugin.generate_copy_insert_with_comments_sql(req)
        }) {
            return sql;
        }
        crate::plugin::default_generate_copy_insert_with_comments_sql(self, request)
    }

    fn generate_copy_update_sql(&self, request: &CopySqlRequest) -> String {
        if let Some(sql) = self.with_oracle_copy_sql(request, |plugin, req| {
            plugin.generate_copy_update_sql(req)
        }) {
            return sql;
        }
        crate::plugin::default_generate_copy_update_sql(self, request)
    }

    fn generate_copy_delete_sql(&self, request: &CopySqlRequest) -> String {
        if let Some(sql) = self.with_oracle_copy_sql(request, |plugin, req| {
            plugin.generate_copy_delete_sql(req)
        }) {
            return sql;
        }
        crate::plugin::default_generate_copy_delete_sql(self, request)
    }

    fn rename_table(&self, _database: &str, old_name: &str, new_name: &str) -> String {
        format!(
            "ALTER TABLE {} RENAME TO {}",
            self.quote_identifier(old_name),
            self.quote_identifier(new_name)
        )
    }

    fn build_column_def(&self, col: &ColumnDefinition) -> String {
        let nullable = if col.is_nullable { "" } else { " NOT NULL" };
        let default = col
            .default_value
            .as_ref()
            .map(|value| format!(" DEFAULT {value}"))
            .unwrap_or_default();
        format!(
            "{} {}{nullable}{default}",
            self.quote_identifier(&col.name),
            col.data_type
        )
    }

    fn build_create_table_sql(&self, design: &TableDesign) -> String {
        let columns = design
            .columns
            .iter()
            .map(|column| self.build_column_def(column))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "CREATE TABLE {} ({})",
            self.quote_identifier(&design.table_name),
            columns
        )
    }

    fn build_alter_table_sql(&self, _original: &TableDesign, _new: &TableDesign) -> String {
        String::new()
    }

    async fn import_data_with_progress(
        &self,
        _connection: &dyn DbConnection,
        _config: &ImportConfig,
        _data: &str,
        _file_name: &str,
        _progress_tx: Option<ImportProgressSender>,
    ) -> Result<ImportResult> {
        Err(anyhow!("external database import is not supported yet"))
    }

    async fn export_data_with_progress(
        &self,
        _connection: &dyn DbConnection,
        _config: &ExportConfig,
        _progress_tx: Option<ExportProgressSender>,
    ) -> Result<ExportResult> {
        Err(anyhow!("external database export is not supported yet"))
    }
}


/// 驱动省略 table_name 时，用请求中的表名回填。
fn fill_trigger_table_names(triggers: Vec<TriggerInfo>, fallback_table: &str) -> Vec<TriggerInfo> {
    triggers
        .into_iter()
        .map(|mut trigger| {
            if trigger.table_name.is_empty() {
                trigger.table_name = fallback_table.to_string();
            }
            trigger
        })
        .collect()
}

fn fill_check_table_names(checks: Vec<CheckInfo>, fallback_table: &str) -> Vec<CheckInfo> {
    checks
        .into_iter()
        .map(|mut check| {
            if check.table_name.is_empty() {
                check.table_name = fallback_table.to_string();
            }
            check
        })
        .collect()
}

fn decode_single_cell<T>(query: crate::executor::QueryResult) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let value = query
        .rows
        .first()
        .and_then(|row| row.first())
        .and_then(|cell| cell.as_deref())
        .ok_or_else(|| anyhow!("external metadata response is empty"))?;
    serde_json::from_str(value).map_err(Into::into)
}

fn names_to_databases(names: Vec<String>) -> Vec<DatabaseInfo> {
    names
        .into_iter()
        .map(|name| DatabaseInfo {
            name,
            charset: None,
            collation: None,
            size: None,
            table_count: None,
            comment: None,
        })
        .collect()
}

fn is_not_supported(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<DbError>()
        .is_some_and(|error| matches!(error, DbError::NotSupported(_)))
}

fn merge_capabilities(
    capabilities: impl IntoIterator<Item = DatabaseCapabilities>,
) -> DatabaseCapabilities {
    capabilities
        .into_iter()
        .fold(DatabaseUiCapabilities::default(), |mut merged, current| {
            merged.supports_schema |= current.supports_schema;
            merged.uses_schema_as_database |= current.uses_schema_as_database;
            merged.supports_sequences |= current.supports_sequences;
            merged.supports_functions |= current.supports_functions;
            merged.supports_procedures |= current.supports_procedures;
            merged.supports_triggers |= current.supports_triggers;
            merged.supports_table_engine |= current.supports_table_engine;
            merged.supports_table_charset |= current.supports_table_charset;
            merged.supports_table_collation |= current.supports_table_collation;
            merged.supports_auto_increment |= current.supports_auto_increment;
            merged.supports_tablespace |= current.supports_tablespace;
            merged.supports_unsigned |= current.supports_unsigned;
            merged.supports_enum_values |= current.supports_enum_values;
            merged.show_charset_in_column_detail |= current.show_charset_in_column_detail;
            merged.show_collation_in_column_detail |= current.show_collation_in_column_detail;
            for engine in current.table_engines {
                if !merged.table_engines.contains(&engine) {
                    merged.table_engines.push(engine);
                }
            }
            merged
        })
}


const MIN_CUSTOM_COLUMN_WIDTH_PX: f32 = 1.0;

#[derive(Debug, Clone, Copy)]
enum ObjectViewKind {
    Databases,
    Schemas,
    Tables,
    Columns,
    Indexes,
    Views,
    Functions,
    Procedures,
    Triggers,
    Sequences,
}

impl ObjectViewKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Databases => "databases",
            Self::Schemas => "schemas",
            Self::Tables => "tables",
            Self::Columns => "columns",
            Self::Indexes => "indexes",
            Self::Views => "views",
            Self::Functions => "functions",
            Self::Procedures => "procedures",
            Self::Triggers => "triggers",
            Self::Sequences => "sequences",
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct ObjectViewScope<'a> {
    database: Option<&'a str>,
    schema: Option<&'a str>,
    table: Option<&'a str>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct DriverObjectView {
    #[serde(default)]
    title: String,
    #[serde(default)]
    columns: Vec<DriverObjectViewColumn>,
    #[serde(default)]
    rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct DriverObjectViewColumn {
    key: String,
    name: String,
    #[serde(default)]
    width_px: Option<f32>,
    #[serde(default)]
    align: Option<String>,
}

fn object_view_from_driver(
    db_node_type: DbNodeType,
    default_title: &str,
    view: DriverObjectView,
) -> Option<ObjectView> {
    if view.columns.is_empty() {
        return None;
    }
    let column_count = view.columns.len();
    let columns = view.columns.into_iter().map(column_from_driver).collect();
    let rows = view
        .rows
        .into_iter()
        .map(|row| normalize_object_view_row(row, column_count))
        .collect();
    let title = if view.title.trim().is_empty() {
        default_title.to_string()
    } else {
        view.title
    };
    Some(ObjectView {
        db_node_type,
        title,
        columns,
        rows,
    })
}

fn column_from_driver(column: DriverObjectViewColumn) -> gpui_component::table::Column {
    let mut col = gpui_component::table::Column::new(column.key, column.name);
    if let Some(width_px) = column.width_px {
        let width = width_px.max(MIN_CUSTOM_COLUMN_WIDTH_PX);
        col = col.width(px(width));
    }
    if let Some(align) = column.align.as_deref() {
        col.align = match align.to_ascii_lowercase().as_str() {
            "center" => TextAlign::Center,
            "right" => TextAlign::Right,
            _ => TextAlign::Left,
        };
    }
    col
}

fn normalize_object_view_row(mut row: Vec<String>, column_count: usize) -> Vec<String> {
    if row.len() < column_count {
        row.resize(column_count, String::new());
    } else if row.len() > column_count {
        row.truncate(column_count);
    }
    row
}

fn object_view(
    db_node_type: DbNodeType,
    title: impl Into<String>,
    columns: Vec<&'static str>,
    rows: Vec<Vec<String>>,
) -> ObjectView {
    ObjectView {
        db_node_type,
        title: title.into(),
        columns: columns
            .into_iter()
            .map(|name| gpui_component::table::Column::new(name, name))
            .collect(),
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_table_name_falls_back_to_request_table() {
        let triggers = fill_trigger_table_names(
            vec![TriggerInfo {
                name: "events_audit_trigger".into(),
                table_name: String::new(),
                event: "insert".into(),
                timing: "after".into(),
                definition: Some("INSERT INTO audit VALUES (NEW.id)".into()),
            }],
            "events",
        );
        assert_eq!(1, triggers.len());
        assert_eq!("events_audit_trigger", triggers[0].name);
        assert_eq!("events", triggers[0].table_name);
        assert_eq!("insert", triggers[0].event);
        assert_eq!("after", triggers[0].timing);
    }

    #[test]
    fn trigger_table_name_keeps_driver_value_when_present() {
        let triggers = fill_trigger_table_names(
            vec![TriggerInfo {
                name: "t1".into(),
                table_name: "orders".into(),
                event: "update".into(),
                timing: "before".into(),
                definition: None,
            }],
            "events",
        );
        assert_eq!("orders", triggers[0].table_name);
    }

    #[test]
    fn check_table_name_falls_back_to_request_table() {
        let checks = fill_check_table_names(
            vec![CheckInfo {
                name: "events_payload_check".into(),
                table_name: String::new(),
                definition: Some("payload IS NOT NULL".into()),
            }],
            "events",
        );
        assert_eq!(1, checks.len());
        assert_eq!("events_payload_check", checks[0].name);
        assert_eq!("events", checks[0].table_name);
    }
}



#[cfg(test)]
mod object_view_tests {
    use super::*;

    #[test]
    fn object_view_from_driver_maps_columns_and_pads_rows() {
        let view = DriverObjectView {
            title: "Event Columns".into(),
            columns: vec![
                DriverObjectViewColumn {
                    key: "name".into(),
                    name: "Field".into(),
                    width_px: Some(220.0),
                    align: None,
                },
                DriverObjectViewColumn {
                    key: "nullable".into(),
                    name: "Null?".into(),
                    width_px: Some(72.0),
                    align: Some("right".into()),
                },
            ],
            rows: vec![vec!["id".into()]],
        };

        let object = object_view_from_driver(DbNodeType::Column, "Columns", view).unwrap();
        assert_eq!("Event Columns", object.title);
        assert_eq!(2, object.columns.len());
        assert_eq!(vec!["id".to_string(), String::new()], object.rows[0]);
        assert_eq!(TextAlign::Right, object.columns[1].align);
    }

    #[test]
    fn object_view_from_driver_returns_none_without_columns() {
        let view = DriverObjectView {
            title: "Empty".into(),
            columns: vec![],
            rows: vec![],
        };
        assert!(object_view_from_driver(DbNodeType::Table, "Tables", view).is_none());
    }

    #[test]
    fn normalize_object_view_row_truncates_extra_cells() {
        assert_eq!(
            vec!["a".to_string(), "b".to_string()],
            normalize_object_view_row(vec!["a".into(), "b".into(), "c".into()], 2)
        );
    }
}

#[cfg(test)]
mod oracle_table_save_tests {
    use super::*;
    use crate::ipc::registry::{
        IpcDriverEntry, IpcDriverTransport, IpcDriverUi, LimitStyle, TableReferenceSchemaMode,
    };
    use std::path::PathBuf;

    fn column_info(name: &str, data_type: &str, is_primary_key: bool) -> ColumnInfo {
        ColumnInfo {
            name: name.to_string(),
            data_type: data_type.to_string(),
            is_nullable: !is_primary_key,
            is_primary_key,
            default_value: None,
            comment: None,
            charset: None,
            collation: None,
        }
    }

    fn oracle_driver(id: &str) -> IpcDriverManifest {
        let mut driver = IpcDriverManifest {
            id: id.to_string(),
            name: id.to_string(),
            category: None,
            description: String::new(),
            version: String::new(),
            entry: IpcDriverEntry {
                command: "driver".to_string(),
                args: Vec::new(),
                working_dir: None,
                commands: Default::default(),
                env_from_config: Default::default(),
            },
            transport: IpcDriverTransport::local_socket(format!("{id}.sock")),
            dialect: Default::default(),
            capabilities: None,
            ui: IpcDriverUi {
                icon: String::new(),
                default_port: None,
                form: None,
            },
            connection: Default::default(),
            manifest_dir: PathBuf::from("."),
        };
        driver.dialect.compatible_database_type = Some(DatabaseType::Oracle);
        driver.dialect.uses_schema_as_database = true;
        driver.dialect.table_reference_schema_mode = TableReferenceSchemaMode::PreferSchema;
        driver.dialect.limit_style = LimitStyle::OffsetFetch;
        driver
    }

    #[test]
    fn external_oracle_table_changes_use_oracle_date_literals() {
        let driver = oracle_driver("oracle-go");
        let plugin = ExternalDatabasePlugin {
            registry: IpcDriverRegistry::from_drivers(vec![driver]),
        };
        let request = TableSaveRequest {
            database: "APP".to_string(),
            schema: None,
            table: "EVENTS".to_string(),
            columns: vec![
                column_info("ID", "NUMBER", true),
                column_info("STARTED_AT", "DATE", false),
            ],
            index_infos: vec![],
            changes: vec![TableRowChange::Added {
                data: vec!["1".to_string(), "2026-06-21 14:05:06".to_string()],
            }],
            driver_id: Some("oracle-go".to_string()),
        };

        let sql = plugin.generate_table_changes_sql(&request);
        assert_eq!(
            "INSERT INTO \"APP\".\"EVENTS\" (\"ID\", \"STARTED_AT\") VALUES ('1', TO_DATE('2026-06-21 14:05:06', 'YYYY-MM-DD HH24:MI:SS'));",
            sql
        );
    }

    #[test]
    fn external_oracle_table_changes_use_oracle_lob_literals() {
        let driver = oracle_driver("oracle-go");
        let plugin = ExternalDatabasePlugin {
            registry: IpcDriverRegistry::from_drivers(vec![driver]),
        };
        let request = TableSaveRequest {
            database: "APP".to_string(),
            schema: None,
            table: "DOCS".to_string(),
            columns: vec![
                column_info("ID", "NUMBER", true),
                column_info("BODY", "CLOB", false),
            ],
            index_infos: vec![],
            changes: vec![TableRowChange::Updated {
                original_data: vec!["1".to_string(), "old".to_string()],
                changes: vec![TableCellChange {
                    column_index: 1,
                    column_name: "BODY".to_string(),
                    old_value: "old".to_string(),
                    new_value: "a".repeat(3_050),
                }],
                rowid: Some("AAABBB".to_string()),
            }],
            driver_id: Some("oracle-go".to_string()),
        };

        let sql = plugin.generate_table_changes_sql(&request);
        assert!(
            sql.contains("UPDATE \"APP\".\"DOCS\" SET \"BODY\" = TO_CLOB('"),
            "got: {sql}"
        );
        assert!(sql.contains(" || TO_CLOB('"), "got: {sql}");
        assert!(sql.contains("WHERE ROWID = 'AAABBB'"), "got: {sql}");
    }

    #[test]
    fn external_non_oracle_driver_keeps_default_sql() {
        let mut driver = oracle_driver("pg-go");
        driver.dialect.compatible_database_type = Some(DatabaseType::PostgreSQL);
        driver.dialect.uses_schema_as_database = false;
        let plugin = ExternalDatabasePlugin {
            registry: IpcDriverRegistry::from_drivers(vec![driver]),
        };
        let request = TableSaveRequest {
            database: "app".to_string(),
            schema: Some("public".to_string()),
            table: "events".to_string(),
            columns: vec![column_info("id", "INT", true)],
            index_infos: vec![],
            changes: vec![TableRowChange::Added {
                data: vec!["1".to_string()],
            }],
            driver_id: Some("pg-go".to_string()),
        };
        let sql = plugin.generate_table_changes_sql(&request);
        assert!(!sql.contains("TO_DATE("), "unexpected oracle sql: {sql}");
        assert!(sql.to_ascii_uppercase().contains("INSERT"), "{sql}");
    }

    #[test]
    fn external_oracle_copy_insert_uses_oracle_date_literals() {
        let driver = oracle_driver("oracle-go");
        let plugin = ExternalDatabasePlugin {
            registry: IpcDriverRegistry::from_drivers(vec![driver]),
        };
        let request = CopySqlRequest::new(
            "EVENTS",
            vec![
                column_info("ID", "NUMBER", true),
                column_info("STARTED_AT", "DATE", false),
            ],
        )
        .with_schema("APP")
        .with_column_names(vec!["ID".into(), "STARTED_AT".into()])
        .with_rows(vec![vec![
            Some("1".into()),
            Some("2026-06-21 14:05:06".into()),
        ]])
        .with_driver_id("oracle-go");

        let sql = plugin.generate_copy_insert_sql(&request);
        assert_eq!(
            r#"INSERT INTO "APP"."EVENTS" ("ID", "STARTED_AT") VALUES ('1', TO_DATE('2026-06-21 14:05:06', 'YYYY-MM-DD HH24:MI:SS'));"#,
            sql
        );
    }

    #[test]
    fn external_non_oracle_copy_keeps_default_literals() {
        let mut driver = oracle_driver("pg-go");
        driver.dialect.compatible_database_type = Some(DatabaseType::PostgreSQL);
        let plugin = ExternalDatabasePlugin {
            registry: IpcDriverRegistry::from_drivers(vec![driver]),
        };
        let request = CopySqlRequest::new(
            "events",
            vec![
                column_info("id", "INT", true),
                column_info("started_at", "TIMESTAMP", false),
            ],
        )
        .with_schema("public")
        .with_column_names(vec!["id".into(), "started_at".into()])
        .with_rows(vec![vec![
            Some("1".into()),
            Some("2026-06-21 14:05:06".into()),
        ]])
        .with_driver_id("pg-go");

        let sql = plugin.generate_copy_insert_sql(&request);
        assert!(!sql.contains("TO_DATE("), "got: {sql}");
        assert!(sql.contains("'2026-06-21 14:05:06'"), "got: {sql}");
    }
}


#[cfg(test)]
mod connection_lifecycle_tests {
    use super::*;
    use crate::ipc::registry::{
        IpcDriverConnection, IpcDriverEntry, IpcDriverTransport, IpcDriverUi,
    };
    use std::path::PathBuf;

    fn base_driver(id: &str) -> IpcDriverManifest {
        IpcDriverManifest {
            id: id.to_string(),
            name: id.to_string(),
            category: None,
            description: String::new(),
            version: String::new(),
            entry: IpcDriverEntry {
                command: "driver".to_string(),
                args: Vec::new(),
                working_dir: None,
                commands: Default::default(),
                env_from_config: Default::default(),
            },
            transport: IpcDriverTransport::local_socket(format!("{id}.sock")),
            dialect: Default::default(),
            capabilities: None,
            ui: IpcDriverUi {
                icon: String::new(),
                default_port: None,
                form: None,
            },
            connection: Default::default(),
            manifest_dir: PathBuf::from("."),
        }
    }

    fn external_config(driver_id: &str, host: &str) -> DbConnectionConfig {
        let mut extra_params = std::collections::HashMap::new();
        extra_params.insert(EXTERNAL_DRIVER_ID_PARAM.to_string(), driver_id.to_string());
        DbConnectionConfig {
            id: "cfg-1".to_string(),
            database_type: DatabaseType::External,
            name: "external".to_string(),
            host: host.to_string(),
            port: 0,
            username: String::new(),
            password: String::new(),
            database: None,
            service_name: None,
            sid: None,
            credential_ref: None,
            ssh_tunnel_credential_ref: None,
            workspace_id: None,
            extra_params,
        }
    }

    #[test]
    fn external_single_file_lifecycle_sets_close_and_lock_key() {
        let mut driver = base_driver("sqlite-go");
        driver.connection = IpcDriverConnection {
            close_on_release: true,
            single_file: true,
            single_connection: true,
            path_fields: vec!["host".to_string()],
        };
        let plugin = ExternalDatabasePlugin {
            registry: IpcDriverRegistry::from_drivers(vec![driver]),
        };

        let lifecycle = plugin.connection_lifecycle(&external_config(
            "sqlite-go",
            "file:/tmp/shared.db",
        ));
        assert!(lifecycle.close_on_release);
        assert_eq!(
            Some("sqlite-go:/tmp/shared.db".to_string()),
            lifecycle.physical_open_lock_key
        );
    }

    #[test]
    fn external_lifecycle_without_single_file_has_no_lock() {
        let mut driver = base_driver("pg-go");
        driver.connection = IpcDriverConnection {
            close_on_release: true,
            single_file: false,
            single_connection: false,
            path_fields: vec![],
        };
        let plugin = ExternalDatabasePlugin {
            registry: IpcDriverRegistry::from_drivers(vec![driver]),
        };

        let lifecycle = plugin.connection_lifecycle(&external_config("pg-go", "localhost"));
        assert!(lifecycle.close_on_release);
        assert!(lifecycle.physical_open_lock_key.is_none());
    }

    #[test]
    fn external_lifecycle_missing_driver_returns_default() {
        let plugin = ExternalDatabasePlugin {
            registry: IpcDriverRegistry::from_drivers(vec![]),
        };
        let lifecycle = plugin.connection_lifecycle(&external_config("missing", "/tmp/x.db"));
        assert_eq!(ConnectionLifecycle::default(), lifecycle);
    }
}
