use crate::connection::DbError;
use crate::plugin_manifest::{DatabaseCapabilities, DatabaseUiManifest};
use one_core::storage::{DatabaseType, get_config_dir};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub const EXTERNAL_DRIVER_ID_PARAM: &str = "external_driver_id";
const DRIVER_MANIFEST_FILE: &str = "driver.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IpcDriverManifest {
    pub id: String,
    pub name: String,
    /// 驱动分类，如 `domestic_database` 用于国产数据库分组。
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
    /// 驱动声明的 IPC 协议版本（如 `1.0`）。缺省视为遗留驱动，按
    /// `LEGACY_IMPLICIT_VERSION` 兼容放行并告警；显式声明则按协议门禁校验。
    #[serde(default)]
    pub protocol_version: Option<String>,
    pub entry: IpcDriverEntry,
    pub transport: IpcDriverTransport,
    #[serde(default)]
    pub dialect: IpcDriverDialect,
    #[serde(default)]
    pub capabilities: Option<DatabaseCapabilities>,
    /// 连接生命周期策略（单文件驱动、释放时关闭等）。
    #[serde(default)]
    pub connection: IpcDriverConnection,
    #[serde(default)]
    pub ui: IpcDriverUi,
    #[serde(skip)]
    pub manifest_dir: PathBuf,
}

/// IPC 驱动连接生命周期声明（manifest `connection` 段）。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct IpcDriverConnection {
    /// 会话释放时是否关闭底层连接（单文件 DB 常用）。
    #[serde(default)]
    pub close_on_release: bool,
    /// 是否为单文件数据库。
    #[serde(default)]
    pub single_file: bool,
    /// 是否限制为单物理连接。
    #[serde(default)]
    pub single_connection: bool,
    /// 从配置解析文件路径的字段路径（如 `host`、`extra_params.path`）。
    #[serde(default)]
    pub path_fields: Vec<String>,
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

    pub fn format_table_reference(
        &self,
        database: &str,
        schema: Option<&str>,
        table: &str,
    ) -> String {
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

/// 宿主当前实现的 IPC 协议版本。
pub const HOST_PROTOCOL_VERSION: &str = "1.0";
/// 未显式声明 `protocol_version` 的遗留驱动被隐式认定的协议版本。
pub const LEGACY_IMPLICIT_VERSION: &str = "1.0";

/// 协议版本门禁结果。
#[derive(Clone, Debug, PartialEq, Eq)]
enum ProtocolCheck {
    /// 版本兼容，静默放行。
    Ok,
    /// 版本兼容但存在隐患（更高 minor、遗留隐式），放行并告警。
    OkWithWarning(String),
    /// 版本不兼容，拒绝加载该驱动。
    Reject(String),
}

/// 解析 `major.minor[.patch]` 形式的协议版本，返回 `(major, minor)`。
/// 接受 `1` / `1.0` / `1.0.0` 等简写；patch 段若存在必须是数字；
/// 非数字、空段或超过三段返回 `None`。
fn parse_protocol_version(version: &str) -> Option<(u64, u64)> {
    let mut parts = version.trim().split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next().map_or(Some(0), |m| m.parse::<u64>().ok())?;
    if let Some(patch) = parts.next() {
        // patch 段存在则必须是数字，且版本必须止于三段。
        patch.parse::<u64>().ok()?;
        if parts.next().is_some() {
            return None;
        }
    }
    Some((major, minor))
}

/// 校验驱动声明的协议版本与宿主的兼容性。
fn check_manifest_protocol(manifest: &IpcDriverManifest) -> ProtocolCheck {
    // 宿主常量为合法版本是编译期契约；拼错应立即可见而非静默放开门禁。
    let (host_major, host_minor) = parse_protocol_version(HOST_PROTOCOL_VERSION)
        .expect("HOST_PROTOCOL_VERSION must be a valid protocol version");

    let declared = match &manifest.protocol_version {
        // 遗留驱动：未显式声明协议版本 → 按隐式版本放行并告警。
        None => {
            return ProtocolCheck::OkWithWarning(format!(
                "external driver '{}' does not declare protocol_version; assuming legacy {}",
                manifest.id, LEGACY_IMPLICIT_VERSION
            ));
        }
        Some(declared) => declared.trim(),
    };

    let Some((driver_major, driver_minor)) = parse_protocol_version(declared) else {
        return ProtocolCheck::Reject(format!(
            "external driver '{}' has invalid protocol_version '{}'",
            manifest.id, declared
        ));
    };

    // major 不一致（无论新旧）一律拒绝：协议语义可能已破坏性变更。
    if driver_major != host_major {
        return ProtocolCheck::Reject(format!(
            "external driver '{}' protocol_version '{}' is incompatible with host '{}'",
            manifest.id, declared, HOST_PROTOCOL_VERSION
        ));
    }

    // 同 major 下驱动 minor 更高：协议向后兼容，放行并告警提示升级宿主。
    if driver_minor > host_minor {
        return ProtocolCheck::OkWithWarning(format!(
            "external driver '{}' protocol_version '{}' is newer than host '{}'; loading anyway",
            manifest.id, declared, HOST_PROTOCOL_VERSION
        ));
    }

    ProtocolCheck::Ok
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
        // 支持：1) 目录本身即驱动包；2) 解压后的单层包裹目录（outer/inner/driver.json）
        let mut root_is_wrapped_driver = false;
        if let Some(driver_dir) = driver_manifest_dir_for(dir)? {
            root_is_wrapped_driver = driver_dir != dir;
            push_driver_if_loadable(&mut drivers, &driver_dir);
        }

        if !root_is_wrapped_driver {
            for entry in std::fs::read_dir(dir).map_err(read_dir_error)? {
                let entry = entry.map_err(read_dir_error)?;
                if !entry.file_type().map_err(read_dir_error)?.is_dir() {
                    continue;
                }
                if let Some(driver_dir) = driver_manifest_dir_for(&entry.path())? {
                    push_driver_if_loadable(&mut drivers, &driver_dir);
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

/// 尝试加载单个驱动目录：成功则压入 `drivers`，失败则记录 warn 并跳过（不中止整体扫描）。
/// 所有失败路径（协议门禁 / JSON / validate / IO）在此统一留痕，避免被静默吞掉。
fn push_driver_if_loadable(drivers: &mut Vec<IpcDriverManifest>, driver_dir: &Path) {
    match load_manifest(driver_dir) {
        Ok(driver) => drivers.push(driver),
        Err(error) => tracing::warn!(
            driver_dir = %driver_dir.display(),
            error = %error,
            "skipped external driver due to load error"
        ),
    }
}

/// 反序列化 manifest 内容，同时收集未识别字段的完整路径（如 `entry.argss`）。
///
/// 注意：未知字段检测以反序列化成功为前提。若拼错的是必填字段（无 `serde(default)`），
/// serde 会先以 `missing field` 报错，此时不会进入未知字段收集分支——这是符合预期的
/// 行为，两类错误由 serde 的原生诊断消息各自负责。
fn parse_manifest_with_unknown(content: &str) -> Result<(IpcDriverManifest, Vec<String>), DbError> {
    let mut unknown_fields: Vec<String> = Vec::new();
    let mut deserializer = serde_json::Deserializer::from_str(content);
    let manifest: IpcDriverManifest =
        serde_ignored::deserialize(&mut deserializer, |ignored_path| {
            unknown_fields.push(ignored_path.to_string());
        })
        .map_err(|error| DbError::connection_with_source("invalid driver manifest", error))?;
    Ok((manifest, unknown_fields))
}

fn load_manifest(driver_dir: &Path) -> Result<IpcDriverManifest, DbError> {
    let path = driver_dir.join(DRIVER_MANIFEST_FILE);
    let content = std::fs::read_to_string(&path).map_err(|error| {
        DbError::connection_with_source("failed to read driver manifest", error)
    })?;
    // 与协议门禁同构：未知字段软告警放行（不拒绝加载），仅提示可能的拼写错误。
    let (mut manifest, unknown_fields) = parse_manifest_with_unknown(&content)?;
    manifest.manifest_dir = driver_dir.to_path_buf();
    manifest.validate()?;
    if !unknown_fields.is_empty() {
        tracing::warn!(
            driver_id = %manifest.id,
            manifest_path = %path.display(),
            unknown_fields = ?unknown_fields,
            "driver manifest contains {} unrecognized field(s); they will be ignored (check for typos)",
            unknown_fields.len()
        );
    }
    match check_manifest_protocol(&manifest) {
        ProtocolCheck::Ok => {}
        ProtocolCheck::OkWithWarning(message) => {
            tracing::warn!(driver_id = %manifest.id, "{message}");
        }
        ProtocolCheck::Reject(message) => {
            // 由 load_from_dir 的兜底 warn 统一留痕，此处仅返回错误避免双重日志。
            return Err(DbError::InvalidManifest(format!(
                "protocol check failed: {message}"
            )));
        }
    }
    Ok(manifest)
}

fn driver_manifest_dir_for(dir: &Path) -> Result<Option<PathBuf>, DbError> {
    if dir.join(DRIVER_MANIFEST_FILE).is_file() {
        return Ok(Some(dir.to_path_buf()));
    }
    single_wrapped_driver_dir(dir)
}

fn single_wrapped_driver_dir(dir: &Path) -> Result<Option<PathBuf>, DbError> {
    let mut found_dir = None;
    for entry in std::fs::read_dir(dir).map_err(read_dir_error)? {
        let entry = entry.map_err(read_dir_error)?;
        if ignored_archive_metadata(&entry.file_name()) {
            continue;
        }
        if !entry.file_type().map_err(read_dir_error)?.is_dir() {
            return Ok(None);
        }
        if found_dir.replace(entry.path()).is_some() {
            return Ok(None);
        }
    }
    let Some(driver_dir) = found_dir else {
        return Ok(None);
    };
    if driver_dir.join(DRIVER_MANIFEST_FILE).is_file() {
        Ok(Some(driver_dir))
    } else {
        Ok(None)
    }
}

fn ignored_archive_metadata(name: &OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    name == ".DS_Store" || name == "__MACOSX" || name.starts_with("._")
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
    fn parses_connection_lifecycle_from_manifest() {
        let manifest: IpcDriverManifest = serde_json::from_str(
            r#"{"id":"sf","name":"SF","entry":{"command":"x"},"transport":{"name":"sf.sock"},"connection":{"close_on_release":true,"single_file":true,"single_connection":true,"path_fields":["host"]}}"#,
        )
        .unwrap();
        assert!(manifest.connection.close_on_release);
        assert!(manifest.connection.single_file);
        assert!(manifest.connection.single_connection);
        assert_eq!(vec!["host".to_string()], manifest.connection.path_fields);
    }

    #[test]
    fn scans_single_wrapped_driver_directory() {
        let temp = tempfile::tempdir().unwrap();
        let outer_dir = temp.path().join("gbase8s");
        let driver_dir = outer_dir.join("gbase8s");
        fs::create_dir_all(&driver_dir).unwrap();
        fs::write(
            driver_dir.join(DRIVER_MANIFEST_FILE),
            r#"{"id":"gbase8s","name":"GBase 8s","entry":{"command":"./gbase8s-ipc-driver"},"transport":{"name":"gbase8s.sock"}}"#,
        )
        .unwrap();

        let registry = IpcDriverRegistry::load_from_dir(temp.path()).unwrap();

        assert_eq!(registry.drivers().len(), 1);
        assert_eq!(registry.find("gbase8s").unwrap().manifest_dir, driver_dir);
    }

    #[test]
    fn scans_single_driver_directory_as_root() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join(DRIVER_MANIFEST_FILE),
            r#"{"id":"demo","name":"Demo","entry":{"command":"python3"},"transport":{"name":"demo.sock"}}"#,
        )
        .unwrap();

        let registry = IpcDriverRegistry::load_from_dir(temp.path()).unwrap();
        assert_eq!(registry.drivers().len(), 1);
        assert_eq!(registry.find("demo").unwrap().manifest_dir, temp.path());
    }

    #[test]
    fn ignores_macos_archive_metadata_in_wrapped_driver_directory() {
        let temp = tempfile::tempdir().unwrap();
        let outer_dir = temp.path().join("gbase8s");
        let driver_dir = outer_dir.join("gbase8s");
        fs::create_dir_all(&driver_dir).unwrap();
        fs::write(outer_dir.join(".DS_Store"), b"").unwrap();
        fs::create_dir(outer_dir.join("__MACOSX")).unwrap();
        fs::write(
            driver_dir.join(DRIVER_MANIFEST_FILE),
            r#"{"id":"gbase8s","name":"GBase 8s","entry":{"command":"./gbase8s-ipc-driver"},"transport":{"name":"gbase8s.sock"}}"#,
        )
        .unwrap();

        let registry = IpcDriverRegistry::load_from_dir(temp.path()).unwrap();
        assert_eq!(registry.drivers().len(), 1);
        assert_eq!(registry.find("gbase8s").unwrap().manifest_dir, driver_dir);
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

    #[test]
    fn deserializes_optional_category() {
        let driver: IpcDriverManifest = serde_json::from_str(
            r#"{"id":"dm","name":"Dameng","category":"domestic_database","entry":{"command":"x"},"transport":{"name":"dm.sock"}}"#,
        )
        .unwrap();
        assert_eq!(Some("domestic_database".to_string()), driver.category);
    }

    /// 构造最小合法 manifest JSON，注入指定 `protocol_version` 字段（空串表示不声明）。
    fn manifest_json(protocol_fragment: &str) -> String {
        let fragment = if protocol_fragment.is_empty() {
            String::new()
        } else {
            format!("{protocol_fragment},")
        };
        format!(
            r#"{{"id":"demo","name":"Demo",{fragment}"entry":{{"command":"python3"}},"transport":{{"name":"demo.sock"}}}}"#
        )
    }

    fn load_single_driver(protocol_fragment: &str) -> Result<IpcDriverRegistry, DbError> {
        let temp = tempfile::tempdir().unwrap();
        let driver_dir = temp.path().join("demo");
        fs::create_dir(&driver_dir).unwrap();
        fs::write(
            driver_dir.join(DRIVER_MANIFEST_FILE),
            manifest_json(protocol_fragment),
        )
        .unwrap();
        // 保持 tempdir 存活到扫描完成。
        let registry = IpcDriverRegistry::load_from_dir(temp.path());
        drop(temp);
        registry
    }

    #[test]
    fn legacy_manifest_without_protocol_version_loads() {
        let registry = load_single_driver("").unwrap();
        assert_eq!(registry.drivers().len(), 1);
        assert!(registry.find("demo").unwrap().protocol_version.is_none());
    }

    #[test]
    fn explicit_v1_0_loads_cleanly() {
        let registry = load_single_driver(r#""protocol_version":"1.0""#).unwrap();
        assert_eq!(registry.drivers().len(), 1);
        assert_eq!(
            Some("1.0".to_string()),
            registry.find("demo").unwrap().protocol_version
        );
    }

    #[test]
    fn explicit_matching_version_is_ok_not_warning() {
        // 显式声明与宿主一致的版本 → 严格 Ok（无告警），与遗留隐式区分开。
        let manifest: IpcDriverManifest =
            serde_json::from_str(&manifest_json(r#""protocol_version":"1.0""#)).unwrap();
        assert_eq!(ProtocolCheck::Ok, check_manifest_protocol(&manifest));
    }

    #[test]
    fn legacy_implicit_version_yields_warning() {
        // 未声明版本 → OkWithWarning（遗留放行告警），而非静默 Ok。
        let manifest: IpcDriverManifest = serde_json::from_str(&manifest_json("")).unwrap();
        assert!(matches!(
            check_manifest_protocol(&manifest),
            ProtocolCheck::OkWithWarning(_)
        ));
    }

    #[test]
    fn explicit_v2_0_is_rejected() {
        let registry = load_single_driver(r#""protocol_version":"2.0""#).unwrap();
        assert!(registry.drivers().is_empty());
    }

    #[test]
    fn future_minor_loads_with_warning() {
        let registry = load_single_driver(r#""protocol_version":"1.5""#).unwrap();
        assert_eq!(registry.drivers().len(), 1);
    }

    #[test]
    fn invalid_semver_is_rejected() {
        for bad in ["abc", "1.x", "", "1.0.0.0"] {
            let fragment = format!(r#""protocol_version":"{}""#, bad);
            let registry = load_single_driver(&fragment).unwrap();
            assert!(
                registry.drivers().is_empty(),
                "protocol_version '{bad}' should be rejected"
            );
        }
    }

    #[test]
    fn parse_protocol_version_accepts_shorthand() {
        assert_eq!(Some((1, 0)), parse_protocol_version("1"));
        assert_eq!(Some((1, 0)), parse_protocol_version("1.0"));
        assert_eq!(Some((1, 2)), parse_protocol_version("1.2"));
        assert_eq!(Some((1, 2)), parse_protocol_version("1.2.3"));
        assert_eq!(None, parse_protocol_version("x.0"));
        assert_eq!(None, parse_protocol_version("1.0.0.0"));
        // patch 段必须是数字。
        assert_eq!(None, parse_protocol_version("1.0.abc"));
        assert_eq!(None, parse_protocol_version("1.0."));
    }

    #[test]
    fn host_protocol_version_const_parses() {
        assert!(parse_protocol_version(HOST_PROTOCOL_VERSION).is_some());
    }

    #[test]
    fn older_major_is_rejected() {
        let registry = load_single_driver(r#""protocol_version":"0.9""#).unwrap();
        assert!(registry.drivers().is_empty());
    }

    #[test]
    fn trims_surrounding_whitespace() {
        let registry = load_single_driver(r#""protocol_version":"  1.0  ""#).unwrap();
        assert_eq!(registry.drivers().len(), 1);
    }

    #[test]
    fn three_segment_version_loads() {
        let registry = load_single_driver(r#""protocol_version":"1.0.0""#).unwrap();
        assert_eq!(registry.drivers().len(), 1);
    }

    #[test]
    fn unknown_top_level_field_is_collected() {
        let json = r#"{"id":"demo","name":"Demo","entry":{"command":"python3"},"transport":{"name":"demo.sock"},"sandbox":{"level":1}}"#;
        let (manifest, unknown) = parse_manifest_with_unknown(json).unwrap();
        assert_eq!(manifest.id, "demo");
        assert_eq!(unknown, vec!["sandbox".to_string()]);
    }

    #[test]
    fn typo_in_nested_field_reports_full_path() {
        // 拼错有默认值的可选字段（args → argss），command 保持合法使反序列化成功。
        let json = r#"{"id":"demo","name":"Demo","entry":{"command":"python3","argss":["-x"]},"transport":{"name":"demo.sock"}}"#;
        let (manifest, unknown) = parse_manifest_with_unknown(json).unwrap();
        assert_eq!(manifest.entry.command, "python3");
        assert_eq!(unknown, vec!["entry.argss".to_string()]);
    }

    #[test]
    fn multiple_unknown_fields_are_all_collected() {
        let json = r#"{"id":"demo","name":"Demo","entry":{"command":"python3","bogus":1},"transport":{"name":"demo.sock"},"extra_top":true}"#;
        let (_manifest, unknown) = parse_manifest_with_unknown(json).unwrap();
        assert!(unknown.contains(&"entry.bogus".to_string()));
        assert!(unknown.contains(&"extra_top".to_string()));
        assert_eq!(unknown.len(), 2);
    }

    #[test]
    fn clean_manifest_produces_no_unknown_fields() {
        let json = r#"{"id":"demo","name":"Demo","entry":{"command":"python3"},"transport":{"name":"demo.sock"}}"#;
        let (_manifest, unknown) = parse_manifest_with_unknown(json).unwrap();
        assert!(unknown.is_empty());
    }

    #[test]
    fn absent_optional_section_does_not_trigger_unknown() {
        // 不提供 capabilities/ui/connection 等可选块 → 缺省不等于未知。
        let json = r#"{"id":"demo","name":"Demo","entry":{"command":"python3"},"transport":{"name":"demo.sock"}}"#;
        let (_manifest, unknown) = parse_manifest_with_unknown(json).unwrap();
        assert!(unknown.is_empty());
    }

    #[test]
    fn unknown_field_inside_optional_section_is_detected() {
        let json = r#"{"id":"demo","name":"Demo","entry":{"command":"python3"},"transport":{"name":"demo.sock"},"connection":{"close_on_release":true,"bogus_field":1}}"#;
        let (_manifest, unknown) = parse_manifest_with_unknown(json).unwrap();
        assert_eq!(unknown, vec!["connection.bogus_field".to_string()]);
    }
}
