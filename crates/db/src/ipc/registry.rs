use crate::connection::DbError;
use crate::plugin_manifest::{DatabaseCapabilities, DatabaseUiManifest};
use one_core::storage::{DatabaseType, get_config_dir};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const EXTERNAL_DRIVER_ID_PARAM: &str = "external_driver_id";
const DRIVER_MANIFEST_FILE: &str = "driver.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IpcDriverManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
    pub entry: IpcDriverEntry,
    pub transport: IpcDriverTransport,
    #[serde(default)]
    pub dialect: IpcDriverDialect,
    #[serde(default)]
    pub capabilities: Option<DatabaseCapabilities>,
    #[serde(default)]
    pub ui: IpcDriverUi,
    #[serde(skip)]
    pub manifest_dir: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IpcDriverEntry {
    pub command: String,
    /// 平台特定启动命令（如 windows / default）。缺省回退到 `command`。
    #[serde(default)]
    pub commands: HashMap<String, String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub working_dir: Option<String>,
    /// 启动子进程时从连接配置注入的环境变量：env_key -> config path。
    /// path 支持 `host` / `port` / `extra_params.jdk_home` 等。
    #[serde(default)]
    pub env_from_config: HashMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcDriverTransport {
    pub name: String,
    #[serde(default)]
    pub connect_timeout_ms: Option<u64>,
}

impl IpcDriverTransport {
    const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 5_000;

    pub fn local_socket(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            connect_timeout_ms: None,
        }
    }

    pub fn connect_timeout_ms(&self) -> u64 {
        self.connect_timeout_ms
            .unwrap_or(Self::DEFAULT_CONNECT_TIMEOUT_MS)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IpcDriverDialect {
    /// 兼容旧 manifest 的单串引号；左右引号未设置时回退到此字段。
    #[serde(default = "default_identifier_quote")]
    pub identifier_quote: String,
    /// 显式左引号（如 MSSQL `[`）；优先于 identifier_quote。
    #[serde(default)]
    pub identifier_quote_left: Option<String>,
    /// 显式右引号（如 MSSQL `]`）；缺省时与左引号相同。
    #[serde(default)]
    pub identifier_quote_right: Option<String>,
    #[serde(default)]
    pub limit_style: LimitStyle,
    #[serde(default)]
    pub table_reference_schema_mode: TableReferenceSchemaMode,
    #[serde(default)]
    pub row_id_column: Option<String>,
    #[serde(default)]
    pub row_id_alias: Option<String>,
    #[serde(default)]
    pub default_order_by: Option<String>,
    #[serde(default)]
    pub supports_schema: bool,
    #[serde(default)]
    pub supports_sequences: bool,
    #[serde(default)]
    pub uses_schema_as_database: bool,
    /// 可选：声明外部驱动兼容的宿主数据库类型，用于 SQL 方言回退（如 schema 切换）。
    #[serde(default)]
    pub compatible_database_type: Option<DatabaseType>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitStyle {
    #[default]
    LimitOffset,
    OffsetFetch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableReferenceSchemaMode {
    #[default]
    Auto,
    PreferSchema,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct IpcDriverUi {
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub default_port: Option<u16>,
    #[serde(default)]
    pub form: Option<DatabaseUiManifest>,
}

impl Default for IpcDriverDialect {
    fn default() -> Self {
        Self {
            identifier_quote: default_identifier_quote(),
            identifier_quote_left: None,
            identifier_quote_right: None,
            limit_style: LimitStyle::default(),
            table_reference_schema_mode: TableReferenceSchemaMode::default(),
            row_id_column: None,
            row_id_alias: None,
            default_order_by: None,
            supports_schema: false,
            supports_sequences: false,
            uses_schema_as_database: false,
            compatible_database_type: None,
        }
    }
}

impl IpcDriverDialect {
    pub fn quote_identifier(&self, identifier: &str) -> String {
        let (left, right) = self.identifier_quote_pair();
        let escaped = identifier.replace(right, &format!("{right}{right}"));
        format!("{left}{escaped}{right}")
    }

    fn identifier_quote_pair(&self) -> (&str, &str) {
        let left = self
            .identifier_quote_left
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| {
                if self.identifier_quote.is_empty() {
                    "\""
                } else {
                    self.identifier_quote.as_str()
                }
            });
        let right = self
            .identifier_quote_right
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(left);
        (left, right)
    }

    pub fn format_table_reference(&self, database: &str, schema: Option<&str>, table: &str) -> String {
        let prefer_schema = matches!(
            self.table_reference_schema_mode,
            TableReferenceSchemaMode::PreferSchema
        ) || (self.supports_schema && !self.uses_schema_as_database);
        if prefer_schema {
            if let Some(schema) = schema.filter(|schema| !schema.trim().is_empty()) {
                return format!(
                    "{}.{}",
                    self.quote_identifier(schema),
                    self.quote_identifier(table)
                );
            }
        }
        if database.trim().is_empty() {
            return self.quote_identifier(table);
        }
        format!(
            "{}.{}",
            self.quote_identifier(database),
            self.quote_identifier(table)
        )
    }

    pub fn format_pagination(&self, limit: usize, offset: usize, order_clause: &str) -> String {
        match self.limit_style {
            LimitStyle::LimitOffset => format!(" LIMIT {limit} OFFSET {offset}"),
            LimitStyle::OffsetFetch => {
                // OFFSET/FETCH requires ORDER BY in some engines; keep provided order_clause as-is.
                let _ = order_clause;
                format!(" OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY")
            }
        }
    }
}

fn default_identifier_quote() -> String {
    "\"".to_string()
}

impl IpcDriverManifest {
    pub fn command_working_dir(&self) -> PathBuf {
        self.entry
            .working_dir
            .as_deref()
            .map(|dir| self.manifest_dir.join(dir))
            .unwrap_or_else(|| self.manifest_dir.clone())
    }

    pub fn effective_capabilities(&self) -> DatabaseCapabilities {
        let mut capabilities = self
            .ui
            .form
            .as_ref()
            .map(|manifest| manifest.capabilities.clone())
            .unwrap_or_else(|| DatabaseCapabilities {
                supports_functions: true,
                supports_procedures: true,
                ..DatabaseCapabilities::default()
            });
        capabilities.supports_schema |= self.dialect.supports_schema;
        capabilities.supports_sequences |= self.dialect.supports_sequences;
        capabilities.uses_schema_as_database |= self.dialect.uses_schema_as_database;
        self.capabilities.clone().unwrap_or(capabilities)
    }

    fn validate(&self) -> Result<(), DbError> {
        if self.id.trim().is_empty() || self.name.trim().is_empty() {
            return Err(DbError::connection(
                "external driver id and name are required",
            ));
        }
        if self.entry.command.trim().is_empty() {
            return Err(DbError::connection(format!(
                "external driver '{}' command is required",
                self.id
            )));
        }
        if self.transport.name.trim().is_empty() {
            return Err(DbError::connection(format!(
                "external driver '{}' local socket name is required",
                self.id
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct IpcDriverRegistry {
    drivers: Vec<IpcDriverManifest>,
}

impl IpcDriverRegistry {
    pub fn load_default() -> Self {
        let dir = default_driver_dir();
        Self::load_from_dir(&dir).unwrap_or_else(|_| Self::empty())
    }

    pub fn load_from_dir(dir: &Path) -> Result<Self, DbError> {
        if !dir.exists() {
            return Ok(Self::empty());
        }

        let mut drivers = Vec::new();
        for entry in std::fs::read_dir(dir).map_err(read_dir_error)? {
            let entry = entry.map_err(read_dir_error)?;
            if entry.file_type().map_err(read_dir_error)?.is_dir() {
                if let Ok(driver) = load_manifest(&entry.path()) {
                    drivers.push(driver);
                }
            }
        }
        drivers.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(Self { drivers })
    }

    pub fn empty() -> Self {
        Self {
            drivers: Vec::new(),
        }
    }

    pub fn from_drivers(mut drivers: Vec<IpcDriverManifest>) -> Self {
        drivers.sort_by(|left, right| left.name.cmp(&right.name));
        Self { drivers }
    }

    pub fn drivers(&self) -> &[IpcDriverManifest] {
        &self.drivers
    }

    pub fn find(&self, driver_id: &str) -> Option<IpcDriverManifest> {
        self.drivers
            .iter()
            .find(|driver| driver.id == driver_id)
            .cloned()
    }
}

pub fn default_driver_dir() -> PathBuf {
    get_config_dir()
        .map(|dir| dir.join("ipc-drivers"))
        .unwrap_or_else(|_| PathBuf::from("ipc-drivers"))
}

fn load_manifest(driver_dir: &Path) -> Result<IpcDriverManifest, DbError> {
    let path = driver_dir.join(DRIVER_MANIFEST_FILE);
    let content = std::fs::read_to_string(&path).map_err(|error| {
        DbError::connection_with_source("failed to read driver manifest", error)
    })?;
    let mut manifest: IpcDriverManifest = serde_json::from_str(&content)
        .map_err(|error| DbError::connection_with_source("invalid driver manifest", error))?;
    manifest.manifest_dir = driver_dir.to_path_buf();
    manifest.validate()?;
    Ok(manifest)
}

fn read_dir_error(error: std::io::Error) -> DbError {
    DbError::connection_with_source("failed to scan external driver directory", error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_local_socket_transport() {
        let manifest: IpcDriverManifest = serde_json::from_str(
            r#"{"id":"demo","name":"Demo","entry":{"command":"python3"},"transport":{"name":"demo.sock"}}"#,
        )
        .unwrap();

        assert_eq!(manifest.transport.name, "demo.sock");
    }

    #[test]
    fn rejects_missing_transport() {
        let result = serde_json::from_str::<IpcDriverManifest>(
            r#"{"id":"demo","name":"Demo","entry":{"command":"python3"}}"#,
        );

        assert!(result.is_err());
    }

    #[test]
    fn rejects_local_socket_transport_without_name() {
        let mut manifest: IpcDriverManifest = serde_json::from_str(
            r#"{"id":"demo","name":"Demo","entry":{"command":"python3"},"transport":{"name":""}}"#,
        )
        .unwrap();
        manifest.manifest_dir = PathBuf::from(".");

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn scans_driver_manifests() {
        let temp = tempfile::tempdir().unwrap();
        let driver_dir = temp.path().join("demo");
        fs::create_dir(&driver_dir).unwrap();
        fs::write(
            driver_dir.join(DRIVER_MANIFEST_FILE),
            r#"{"id":"demo","name":"Demo","entry":{"command":"python3"},"transport":{"name":"demo.sock"}}"#,
        )
        .unwrap();

        let registry = IpcDriverRegistry::load_from_dir(temp.path()).unwrap();
        assert_eq!(registry.drivers().len(), 1);
        assert_eq!(registry.find("demo").unwrap().name, "Demo");
    }

    #[test]
    fn parses_top_level_capabilities() {
        let manifest: IpcDriverManifest = serde_json::from_str(
            r#"{"id":"demo","name":"Demo","entry":{"command":"python3"},"transport":{"name":"demo.sock"},"dialect":{"supports_schema":false},"capabilities":{"supports_schema":true,"supports_functions":true}}"#,
        )
        .unwrap();

        let capabilities = manifest.effective_capabilities();
        assert!(capabilities.supports_schema);
        assert!(capabilities.supports_functions);
    }

    #[test]
    fn falls_back_to_legacy_dialect_capabilities() {
        let manifest: IpcDriverManifest = serde_json::from_str(
            r#"{"id":"demo","name":"Demo","entry":{"command":"python3"},"transport":{"name":"demo.sock"},"dialect":{"supports_schema":true,"supports_sequences":true}}"#,
        )
        .unwrap();

        let capabilities = manifest.effective_capabilities();
        assert!(capabilities.supports_schema);
        assert!(capabilities.supports_sequences);
        assert!(capabilities.supports_functions);
        assert!(capabilities.supports_procedures);
    }

    #[test]
    fn falls_back_to_legacy_ui_form_capabilities() {
        let manifest: IpcDriverManifest = serde_json::from_str(
            r#"{"id":"demo","name":"Demo","entry":{"command":"python3"},"transport":{"name":"demo.sock"},"ui":{"form":{"schema_version":1,"capabilities":{"supports_triggers":true},"forms":[],"actions":{"actions":[]}}}}"#,
        )
        .unwrap();

        assert!(manifest.effective_capabilities().supports_triggers);
    }

    #[test]
    fn prefer_schema_table_reference_uses_schema() {
        let mut dialect = IpcDriverDialect::default();
        dialect.table_reference_schema_mode = TableReferenceSchemaMode::PreferSchema;
        assert_eq!(
            "\"APP\".\"EVENTS\"",
            dialect.format_table_reference("", Some("APP"), "EVENTS")
        );
    }

    #[test]
    fn offset_fetch_pagination_style() {
        let mut dialect = IpcDriverDialect::default();
        dialect.limit_style = LimitStyle::OffsetFetch;
        assert_eq!(
            " OFFSET 0 ROWS FETCH NEXT 25 ROWS ONLY",
            dialect.format_pagination(25, 0, " ORDER BY ROWID")
        );
    }

    #[test]
    fn quote_identifier_supports_bracket_pair() {
        let dialect = IpcDriverDialect {
            identifier_quote_left: Some("[".into()),
            identifier_quote_right: Some("]".into()),
            ..IpcDriverDialect::default()
        };
        assert_eq!("[users]", dialect.quote_identifier("users"));
        assert_eq!("[a]]b]", dialect.quote_identifier("a]b"));
    }

    #[test]
    fn quote_identifier_falls_back_to_legacy_single_quote() {
        let dialect = IpcDriverDialect {
            identifier_quote: "`".into(),
            ..IpcDriverDialect::default()
        };
        assert_eq!("`users`", dialect.quote_identifier("users"));
    }
}
