use crate::cloud_sync::sync_type::SyncableItem;
use crate::crypto;
use crate::storage::traits::Entity;
use gpui_component::Size::Large;
use gpui_component::{Icon, IconName, Sizable};
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ConnectionType {
    All,
    Database,
    SshSftp,
    Redis,
    MongoDB,
    ChatDB,
    Serial,
}

impl fmt::Display for ConnectionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ConnectionType::All => "All",
            ConnectionType::Database => "Database",
            ConnectionType::SshSftp => "SshSftp",
            ConnectionType::Redis => "Redis",
            ConnectionType::MongoDB => "MongoDB",
            ConnectionType::ChatDB => "ChatDB",
            ConnectionType::Serial => "Serial",
        };
        write!(f, "{}", s)
    }
}

impl ConnectionType {
    pub fn all() -> Vec<ConnectionType> {
        vec![
            ConnectionType::All,
            ConnectionType::Database,
            ConnectionType::SshSftp,
            ConnectionType::Redis,
            ConnectionType::MongoDB,
            ConnectionType::ChatDB,
            ConnectionType::Serial,
        ]
    }
    pub fn from_str(s: &str) -> Self {
        match s {
            "Database" => ConnectionType::Database,
            "SshSftp" => ConnectionType::SshSftp,
            "Redis" => ConnectionType::Redis,
            "MongoDB" => ConnectionType::MongoDB,
            "ChatDB" => ConnectionType::ChatDB,
            "Serial" => ConnectionType::Serial,
            _ => ConnectionType::Database,
        }
    }

    pub fn label(&self) -> String {
        match self {
            ConnectionType::All => t!("ConnectionType.all").to_string(),
            ConnectionType::Database => t!("ConnectionType.database").to_string(),
            ConnectionType::SshSftp => t!("ConnectionType.ssh_sftp").to_string(),
            ConnectionType::Redis => t!("ConnectionType.redis").to_string(),
            ConnectionType::MongoDB => t!("ConnectionType.mongodb").to_string(),
            ConnectionType::ChatDB => t!("ConnectionType.chatdb").to_string(),
            ConnectionType::Serial => t!("ConnectionType.serial").to_string(),
        }
    }

    pub fn icon(&self) -> IconName {
        match self {
            ConnectionType::All => IconName::Server,
            ConnectionType::Database => IconName::Database,
            ConnectionType::SshSftp => IconName::TerminalColor,
            ConnectionType::Redis => IconName::Redis,
            ConnectionType::MongoDB => IconName::MongoDB,
            ConnectionType::ChatDB => IconName::AI,
            ConnectionType::Serial => IconName::SerialPort,
        }
    }
}

/// Database type enumeration
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DatabaseType {
    MySQL,
    PostgreSQL,
    SQLite,
    DuckDB,
    MSSQL,
    Oracle,
    ClickHouse,
    External,
}

impl DatabaseType {
    pub fn all() -> &'static [DatabaseType] {
        Self::builtin_all()
    }

    pub fn builtin_all() -> &'static [DatabaseType] {
        &[
            DatabaseType::MySQL,
            DatabaseType::PostgreSQL,
            DatabaseType::SQLite,
            DatabaseType::DuckDB,
            DatabaseType::MSSQL,
            DatabaseType::Oracle,
            DatabaseType::ClickHouse,
        ]
    }

    pub fn as_str(&self) -> &str {
        match self {
            DatabaseType::MySQL => "MySQL",
            DatabaseType::PostgreSQL => "PostgreSQL",
            DatabaseType::SQLite => "SQLite",
            DatabaseType::DuckDB => "DuckDB",
            DatabaseType::MSSQL => "MSSQL",
            DatabaseType::Oracle => "Oracle",
            DatabaseType::ClickHouse => "ClickHouse",
            DatabaseType::External => "External",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "MySQL" => Some(DatabaseType::MySQL),
            "PostgreSQL" => Some(DatabaseType::PostgreSQL),
            "SQLite" => Some(DatabaseType::SQLite),
            "DuckDB" => Some(DatabaseType::DuckDB),
            "MSSQL" => Some(DatabaseType::MSSQL),
            "Oracle" => Some(DatabaseType::Oracle),
            "ClickHouse" => Some(DatabaseType::ClickHouse),
            "External" => Some(DatabaseType::External),
            _ => None,
        }
    }

    pub fn as_icon(&self) -> Icon {
        match self {
            DatabaseType::MySQL => IconName::MySQLColor.color().with_size(Large),
            DatabaseType::PostgreSQL => IconName::PostgreSQLColor.color().with_size(Large),
            DatabaseType::SQLite => IconName::SQLiteColor.color().with_size(Large),
            DatabaseType::DuckDB => IconName::DuckDB.color().with_size(Large),
            DatabaseType::MSSQL => IconName::MSSQLColor.color().with_size(Large),
            DatabaseType::Oracle => IconName::OracleColor.color().with_size(Large),
            DatabaseType::ClickHouse => IconName::ClickHouseColor.color().with_size(Large),
            DatabaseType::External => IconName::Database.color().with_size(Large),
        }
    }
    pub fn as_node_icon(&self) -> Icon {
        match self {
            DatabaseType::MySQL => IconName::MySQLLineColor.color().with_size(Large),
            DatabaseType::PostgreSQL => IconName::PostgreSQLLineColor.color().with_size(Large),
            DatabaseType::SQLite => IconName::SQLiteLineColor.color().with_size(Large),
            DatabaseType::DuckDB => IconName::DuckDB.color().with_size(Large),
            DatabaseType::MSSQL => IconName::MSSQLLineColor.color().with_size(Large),
            DatabaseType::Oracle => IconName::OracleLineColor.color().with_size(Large),
            DatabaseType::ClickHouse => IconName::ClickHouseLineColor.color().with_size(Large),
            DatabaseType::External => IconName::Database.color().with_size(Large),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshParams {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: SshAuthMethod,
    /// 关联证书引用（保存引用 + 当前凭据快照）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_ref: Option<CertificateReference>,
    /// 连接超时（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connect_timeout: Option<u64>,
    /// 心跳间隔（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keepalive_interval: Option<u64>,
    /// 最大心跳失败次数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keepalive_max: Option<usize>,
    /// 是否启用旧版 KEX 兼容模式
    #[serde(default)]
    pub enable_legacy_kex: bool,
    /// 默认工作目录
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_directory: Option<String>,
    /// 初始化脚本
    #[serde(skip_serializing_if = "Option::is_none")]
    pub init_script: Option<String>,
    /// SFTP 本地目录（留空则使用用户主目录）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sftp_local_directory: Option<String>,
    /// SFTP 远程目录（留空则使用服务器默认目录）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sftp_remote_directory: Option<String>,
    /// 跳板机配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jump_server: Option<JumpServerConfig>,
    /// 代理配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy: Option<ProxyConfig>,
}

/// 跳板机配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JumpServerConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: SshAuthMethod,
}

/// 代理类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProxyType {
    Socks5,
    Http,
}

/// 代理配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub proxy_type: ProxyType,
    pub host: String,
    pub port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SshAuthMethod {
    Password {
        password: String,
    },
    PrivateKey {
        ssh_private_key: String,
        passphrase: Option<String>,
    },
    Agent,
    AutoPublicKey,
}

/// Redis 连接模式
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RedisMode {
    /// 单机模式
    #[default]
    Standalone,
    /// 哨兵模式
    Sentinel,
    /// 集群模式
    Cluster,
}

/// Redis 哨兵配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisSentinelConfig {
    /// 主节点名称
    pub master_name: String,
    /// 哨兵节点列表（host:port）
    pub sentinels: Vec<String>,
    /// 哨兵密码
    pub sentinel_password: Option<String>,
}

/// Redis 集群节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisClusterConfig {
    /// 集群节点列表（host:port）
    pub nodes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisParams {
    pub host: String,
    pub port: u16,
    pub password: Option<String>,
    pub username: Option<String>,
    /// 关联证书引用（保存引用 + 当前凭据快照）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_ref: Option<CertificateReference>,
    pub db_index: u8,
    /// 连接模式
    #[serde(default)]
    pub mode: RedisMode,
    /// 是否启用 TLS
    #[serde(default)]
    pub use_tls: bool,
    /// 连接超时（秒）
    #[serde(default)]
    pub connect_timeout: Option<u64>,
    /// 哨兵配置
    #[serde(default)]
    pub sentinel: Option<RedisSentinelConfig>,
    /// 集群配置
    #[serde(default)]
    pub cluster: Option<RedisClusterConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MongoDBParams {
    #[serde(default)]
    pub connection_string: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    /// 关联证书引用（保存引用 + 当前凭据快照）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_ref: Option<CertificateReference>,
    #[serde(default)]
    pub auth_source: Option<String>,
    #[serde(default)]
    pub replica_set: Option<String>,
    #[serde(default)]
    pub read_preference: Option<String>,
    #[serde(default)]
    pub use_srv_record: bool,
    #[serde(default)]
    pub direct_connection: bool,
    #[serde(default)]
    pub use_tls: bool,
    #[serde(default)]
    pub connect_timeout_seconds: Option<u64>,
    #[serde(default)]
    pub application_name: Option<String>,
}

/// 串口校验位
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SerialParity {
    #[default]
    None,
    Odd,
    Even,
}

impl SerialParity {
    pub fn all() -> &'static [SerialParity] {
        &[SerialParity::None, SerialParity::Odd, SerialParity::Even]
    }

    pub fn label(&self) -> &'static str {
        match self {
            SerialParity::None => "None",
            SerialParity::Odd => "Odd",
            SerialParity::Even => "Even",
        }
    }
}

/// 串口流控
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SerialFlowControl {
    #[default]
    None,
    Software,
    Hardware,
}

impl SerialFlowControl {
    pub fn all() -> &'static [SerialFlowControl] {
        &[
            SerialFlowControl::None,
            SerialFlowControl::Software,
            SerialFlowControl::Hardware,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            SerialFlowControl::None => "None",
            SerialFlowControl::Software => "XON/XOFF",
            SerialFlowControl::Hardware => "RTS/CTS",
        }
    }
}

/// 串口连接参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerialParams {
    /// 串口设备路径，如 /dev/ttyUSB0 或 COM1
    pub port_name: String,
    /// 波特率
    #[serde(default = "default_baud_rate")]
    pub baud_rate: u32,
    /// 数据位 (5/6/7/8)
    #[serde(default = "default_data_bits")]
    pub data_bits: u8,
    /// 停止位 (1/2)
    #[serde(default = "default_stop_bits")]
    pub stop_bits: u8,
    /// 校验位
    #[serde(default)]
    pub parity: SerialParity,
    /// 流控
    #[serde(default)]
    pub flow_control: SerialFlowControl,
}

fn default_baud_rate() -> u32 {
    115200
}

fn default_data_bits() -> u8 {
    8
}

fn default_stop_bits() -> u8 {
    1
}

impl Default for SerialParams {
    fn default() -> Self {
        Self {
            port_name: String::new(),
            baud_rate: 115200,
            data_bits: 8,
            stop_bits: 1,
            parity: SerialParity::None,
            flow_control: SerialFlowControl::None,
        }
    }
}

/// Connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbConnectionConfig {
    #[serde(skip)]
    pub id: String,
    pub database_type: DatabaseType,
    #[serde(skip)]
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: Option<String>,
    pub service_name: Option<String>,
    pub sid: Option<String>,
    /// 数据库主认证证书引用（保存引用 + 当前凭据快照）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_ref: Option<CertificateReference>,
    /// SSH 隧道认证证书引用（保存引用 + 当前凭据快照）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_tunnel_credential_ref: Option<CertificateReference>,
    #[serde(skip)]
    pub workspace_id: Option<i64>,
    #[serde(default)]
    pub extra_params: std::collections::HashMap<String, String>,
}

impl DbConnectionConfig {
    pub fn get_param(&self, key: &str) -> Option<&String> {
        self.extra_params.get(key)
    }

    pub fn get_param_as<T: std::str::FromStr>(&self, key: &str) -> Option<T> {
        self.extra_params.get(key).and_then(|v| v.parse().ok())
    }

    pub fn get_param_bool(&self, key: &str) -> bool {
        self.extra_params
            .get(key)
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false)
    }

    pub fn server_info(&self) -> String {
        match self.database_type {
            DatabaseType::SQLite | DatabaseType::DuckDB => format!("{}", self.host),
            _ => format!("{}:{}", self.host, self.port),
        }
    }

    pub fn is_change(&self, other: &DbConnectionConfig) -> bool {
        self.host != other.host
            || self.port != other.port
            || self.username != other.username
            || self.password != other.password
            || self.database != other.database
            || self.service_name != other.service_name
            || self.sid != other.sid
            || self.credential_ref != other.credential_ref
            || self.ssh_tunnel_credential_ref != other.ssh_tunnel_credential_ref
            || self.extra_params != other.extra_params
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CertificateKind {
    UsernamePassword,
    SshPrivateKey,
}

impl CertificateKind {
    pub fn from_str(value: &str) -> Self {
        match value {
            "SshPrivateKey" => Self::SshPrivateKey,
            _ => Self::UsernamePassword,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::UsernamePassword => "账号密码",
            Self::SshPrivateKey => "SSH 私钥",
        }
    }
}

impl fmt::Display for CertificateKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::UsernamePassword => "UsernamePassword",
            Self::SshPrivateKey => "SshPrivateKey",
        };
        write!(f, "{}", value)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CertificateReference {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud_id: Option<String>,
}

impl CertificateReference {
    pub fn from_certificate(certificate: &Certificate) -> Self {
        Self {
            local_id: certificate.id,
            cloud_id: certificate.cloud_id.clone(),
        }
    }

    pub fn matches_certificate(&self, certificate: &Certificate) -> bool {
        self.matches_ids(certificate.id, certificate.cloud_id.as_deref())
    }

    pub fn matches_ids(&self, local_id: Option<i64>, cloud_id: Option<&str>) -> bool {
        if let Some(reference_cloud_id) = self.cloud_id.as_deref() {
            return cloud_id == Some(reference_cloud_id);
        }

        matches!((self.local_id, local_id), (Some(reference_id), Some(local_id)) if reference_id == local_id)
    }

    pub fn sync_with_certificate(&mut self, certificate: &Certificate) -> bool {
        let mut changed = false;

        if self.local_id != certificate.id {
            self.local_id = certificate.id;
            changed = true;
        }

        if self.cloud_id != certificate.cloud_id {
            self.cloud_id = certificate.cloud_id.clone();
            changed = true;
        }

        changed
    }
}

/// 统一管理的证书实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Certificate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    pub name: String,
    pub kind: CertificateKind,
    pub params: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remark: Option<String>,
    #[serde(default = "default_sync_enabled")]
    pub sync_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_synced_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
}

impl Certificate {
    pub fn username(&self) -> Option<&str> {
        self.params.get("username")?.as_str()
    }
    pub fn password(&self) -> Option<&str> {
        self.params.get("password")?.as_str()
    }
    pub fn key_path(&self) -> Option<&str> {
        self.params.get("ssh_private_key")?.as_str()
    }
    pub fn passphrase(&self) -> Option<&str> {
        self.params.get("passphrase")?.as_str()
    }
    pub fn ssh_private_key(&self) -> Option<&str> {
        self.params.get("ssh_private_key")?.as_str()
    }

    pub fn set_username(&mut self, value: &str) {
        self.params.as_object_mut().unwrap().insert(
            "username".to_string(),
            serde_json::Value::String(value.to_string()),
        );
    }
    pub fn set_password(&mut self, value: Option<String>) {
        let obj = self.params.as_object_mut().unwrap();
        match value {
            Some(v) => obj.insert("password".to_string(), serde_json::Value::String(v)),
            None => obj.remove("password"),
        };
    }
    pub fn set_key_path(&mut self, value: Option<String>) {
        let obj = self.params.as_object_mut().unwrap();
        match value {
            Some(v) => obj.insert("ssh_private_key".to_string(), serde_json::Value::String(v)),
            None => obj.remove("ssh_private_key"),
        };
    }
    pub fn set_passphrase(&mut self, value: Option<String>) {
        let obj = self.params.as_object_mut().unwrap();
        match value {
            Some(v) => obj.insert("passphrase".to_string(), serde_json::Value::String(v)),
            None => obj.remove("passphrase"),
        };
    }

    pub fn display_subtitle(&self) -> String {
        match self.kind {
            CertificateKind::UsernamePassword => {
                let username = self.username().unwrap_or("");
                format!("{} / 账号密码", username)
            }
            CertificateKind::SshPrivateKey => {
                let username = self.username().unwrap_or("");
                let has_key = self.key_path().map(|k| !k.is_empty()).unwrap_or(false);
                let key_info = if has_key { "[已存储私钥]" } else { "未设置私钥" };
                format!("{} / {}", username, key_info)
            }
        }
    }
}

impl Entity for Certificate {
    fn id(&self) -> Option<i64> {
        self.id
    }

    fn created_at(&self) -> i64 {
        self.created_at
            .expect("created_at 在从数据库读取后应该存在")
    }

    fn updated_at(&self) -> i64 {
        self.updated_at
            .expect("updated_at 在从数据库读取后应该存在")
    }
}

impl SyncableItem for Certificate {
    fn local_id(&self) -> Option<i64> {
        self.id
    }

    fn set_local_id(&mut self, id: Option<i64>) {
        self.id = id;
    }

    fn item_name(&self) -> &str {
        &self.name
    }

    fn cloud_id(&self) -> Option<&str> {
        self.cloud_id.as_deref()
    }

    fn set_cloud_id(&mut self, cloud_id: Option<String>) {
        self.cloud_id = cloud_id;
    }

    fn updated_at(&self) -> Option<i64> {
        self.updated_at
    }

    fn last_synced_at(&self) -> Option<i64> {
        self.last_synced_at
    }
}

/// Workspace for organizing connections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
    /// 云端 ID（用于同步）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud_id: Option<String>,
    /// 最后同步时间戳
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_synced_at: Option<i64>,
}

impl Entity for Workspace {
    fn id(&self) -> Option<i64> {
        self.id
    }

    fn created_at(&self) -> i64 {
        self.created_at
            .expect("created_at 在从数据库读取后应该存在")
    }

    fn updated_at(&self) -> i64 {
        self.updated_at
            .expect("updated_at 在从数据库读取后应该存在")
    }
}

impl Workspace {
    pub fn new(name: String) -> Self {
        Self {
            id: None,
            name,
            sort_order: None,
            color: None,
            icon: None,
            created_at: None,
            updated_at: None,
            cloud_id: None,
            last_synced_at: None,
        }
    }
}

impl SyncableItem for Workspace {
    fn local_id(&self) -> Option<i64> {
        self.id
    }

    fn set_local_id(&mut self, id: Option<i64>) {
        self.id = id;
    }

    fn item_name(&self) -> &str {
        &self.name
    }

    fn cloud_id(&self) -> Option<&str> {
        self.cloud_id.as_deref()
    }

    fn set_cloud_id(&mut self, cloud_id: Option<String>) {
        self.cloud_id = cloud_id;
    }

    fn updated_at(&self) -> Option<i64> {
        self.updated_at
    }

    fn last_synced_at(&self) -> Option<i64> {
        self.last_synced_at
    }

    fn uses_sync_state(&self) -> bool {
        true
    }
}

/// Stored connection with ID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredConnection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    pub name: String,
    pub connection_type: ConnectionType,
    pub params: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<i64>,
    /// 已选中的数据库ID列表（JSON数组），None表示全选
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_databases: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remark: Option<String>,
    /// 是否启用云同步（默认 true）
    #[serde(default = "default_sync_enabled")]
    pub sync_enabled: bool,
    /// 云端记录 ID（同步成功后获得）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud_id: Option<String>,
    /// 最后同步时间戳
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_synced_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
    /// 连接创建者 ID（用户 UUID，用于权限判断）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
}

fn default_sync_enabled() -> bool {
    true
}

impl Entity for StoredConnection {
    fn id(&self) -> Option<i64> {
        self.id
    }

    fn created_at(&self) -> i64 {
        self.created_at
            .expect("created_at 在从数据库读取后应该存在")
    }

    fn updated_at(&self) -> i64 {
        self.updated_at
            .expect("updated_at 在从数据库读取后应该存在")
    }
}

impl SyncableItem for StoredConnection {
    fn local_id(&self) -> Option<i64> {
        self.id
    }

    fn set_local_id(&mut self, id: Option<i64>) {
        self.id = id;
    }

    fn item_name(&self) -> &str {
        &self.name
    }

    fn cloud_id(&self) -> Option<&str> {
        self.cloud_id.as_deref()
    }

    fn set_cloud_id(&mut self, cloud_id: Option<String>) {
        self.cloud_id = cloud_id;
    }

    fn updated_at(&self) -> Option<i64> {
        self.updated_at
    }

    fn last_synced_at(&self) -> Option<i64> {
        self.last_synced_at
    }
}

impl StoredConnection {
    pub fn new_database(
        name: String,
        params: DbConnectionConfig,
        workspace_id: Option<i64>,
    ) -> Self {
        Self {
            id: None,
            name,
            connection_type: ConnectionType::Database,
            params: serde_json::to_string(&params).expect("DbConnectionConfig 序列化不应失败"),
            sort_order: None,
            workspace_id,
            selected_databases: if let Some(database) = &params.database {
                Some(format!("[\"{}\"]", database))
            } else {
                None
            },
            remark: None,
            sync_enabled: true,
            cloud_id: None,
            last_synced_at: None,
            created_at: None,
            updated_at: None,
            owner_id: None,
        }
    }

    pub fn new_ssh(name: String, params: SshParams, workspace_id: Option<i64>) -> Self {
        Self {
            id: None,
            name,
            connection_type: ConnectionType::SshSftp,
            params: serde_json::to_string(&params).expect("SshParams 序列化不应失败"),
            sort_order: None,
            workspace_id,
            selected_databases: None,
            remark: None,
            sync_enabled: true,
            cloud_id: None,
            last_synced_at: None,
            created_at: None,
            updated_at: None,
            owner_id: None,
        }
    }

    pub fn new_redis(name: String, params: RedisParams, workspace_id: Option<i64>) -> Self {
        Self {
            id: None,
            name,
            connection_type: ConnectionType::Redis,
            params: serde_json::to_string(&params).expect("RedisParams 序列化不应失败"),
            sort_order: None,
            workspace_id,
            selected_databases: None,
            remark: None,
            sync_enabled: true,
            cloud_id: None,
            last_synced_at: None,
            created_at: None,
            updated_at: None,
            owner_id: None,
        }
    }

    pub fn new_mongodb(name: String, params: MongoDBParams, workspace_id: Option<i64>) -> Self {
        Self {
            id: None,
            name,
            connection_type: ConnectionType::MongoDB,
            params: serde_json::to_string(&params).expect("MongoDBParams 序列化不应失败"),
            sort_order: None,
            workspace_id,
            selected_databases: None,
            remark: None,
            sync_enabled: true,
            cloud_id: None,
            last_synced_at: None,
            created_at: None,
            updated_at: None,
            owner_id: None,
        }
    }

    pub fn to_ssh_params(&self) -> Result<SshParams, serde_json::Error> {
        serde_json::from_str(&self.params)
    }

    pub fn to_redis_params(&self) -> Result<RedisParams, serde_json::Error> {
        serde_json::from_str(&self.params)
    }

    pub fn to_mongodb_params(&self) -> Result<MongoDBParams, serde_json::Error> {
        serde_json::from_str(&self.params)
    }

    pub fn new_serial(name: String, params: SerialParams, workspace_id: Option<i64>) -> Self {
        Self {
            id: None,
            name,
            connection_type: ConnectionType::Serial,
            params: serde_json::to_string(&params).expect("SerialParams 序列化不应失败"),
            sort_order: None,
            workspace_id,
            selected_databases: None,
            remark: None,
            sync_enabled: true,
            cloud_id: None,
            last_synced_at: None,
            created_at: None,
            updated_at: None,
            owner_id: None,
        }
    }

    pub fn to_serial_params(&self) -> Result<SerialParams, serde_json::Error> {
        serde_json::from_str(&self.params)
    }

    pub fn to_db_connection(&self) -> Result<DbConnectionConfig, serde_json::Error> {
        let mut params: DbConnectionConfig = serde_json::from_str(&self.params)?;
        params.name = self.name.clone();
        params.workspace_id = self.workspace_id;
        params.id = self.id.unwrap_or(0).to_string();
        Ok(params)
    }

    pub fn from_db_connection(connection: DbConnectionConfig) -> Self {
        let name = connection.name.clone();
        let workspace_id = connection.workspace_id.clone();
        Self::new_database(name, connection, workspace_id)
    }

    /// 获取已选中的数据库列表，None表示全选
    pub fn get_selected_databases(&self) -> Option<Vec<String>> {
        self.selected_databases
            .as_ref()
            .and_then(|json| serde_json::from_str(json).ok())
    }

    /// 设置已选中的数据库列表，None表示全选
    pub fn set_selected_databases(&mut self, databases: Option<Vec<String>>) {
        self.selected_databases =
            databases.map(|dbs| serde_json::to_string(&dbs).unwrap_or_default());
    }

    /// 对 params 中的敏感字段进行加密，返回加密后的 params 字符串。
    /// 敏感字段包括：password、passphrase 以及嵌套结构中的同名字段。
    pub fn encrypt_params(&self) -> String {
        encrypt_json_passwords(&self.params)
    }

    /// 对 params 中的加密字段进行解密，返回解密后的 params 字符串。
    pub fn decrypt_params(&self) -> String {
        decrypt_json_passwords(&self.params)
    }

    /// 返回一个新的 StoredConnection，其 params 中的密码字段已解密
    pub fn with_decrypted_params(&self) -> Self {
        let mut cloned = self.clone();
        cloned.params = cloned.decrypt_params();
        cloned
    }
}

pub fn apply_certificate_to_connection_snapshot(
    connection: &mut StoredConnection,
    certificate: &Certificate,
) -> bool {
    match connection.connection_type {
        ConnectionType::Database => {
            let Ok(mut config) = connection.to_db_connection() else {
                return false;
            };

            let changed = apply_certificate_to_db_config(&mut config, certificate);
            if changed {
                if let Ok(params) = serde_json::to_string(&config) {
                    connection.params = params;
                }
            }
            changed
        }
        ConnectionType::SshSftp => {
            let Ok(mut params) = connection.to_ssh_params() else {
                return false;
            };

            let changed = apply_certificate_to_ssh_params(&mut params, certificate);
            if changed {
                if let Ok(serialized) = serde_json::to_string(&params) {
                    connection.params = serialized;
                }
            }
            changed
        }
        ConnectionType::Redis => {
            let Ok(mut params) = connection.to_redis_params() else {
                return false;
            };

            let changed = apply_certificate_to_redis_params(&mut params, certificate);
            if changed {
                if let Ok(serialized) = serde_json::to_string(&params) {
                    connection.params = serialized;
                }
            }
            changed
        }
        ConnectionType::MongoDB => {
            let Ok(mut params) = connection.to_mongodb_params() else {
                return false;
            };

            let changed = apply_certificate_to_mongodb_params(&mut params, certificate);
            if changed {
                if let Ok(serialized) = serde_json::to_string(&params) {
                    connection.params = serialized;
                }
            }
            changed
        }
        _ => false,
    }
}

pub fn detach_certificate_from_connection_snapshot(
    connection: &mut StoredConnection,
    local_id: Option<i64>,
    cloud_id: Option<&str>,
) -> bool {
    match connection.connection_type {
        ConnectionType::Database => {
            let Ok(mut config) = connection.to_db_connection() else {
                return false;
            };

            let changed = detach_certificate_from_db_config(&mut config, local_id, cloud_id);
            if changed {
                if let Ok(params) = serde_json::to_string(&config) {
                    connection.params = params;
                }
            }
            changed
        }
        ConnectionType::SshSftp => {
            let Ok(mut params) = connection.to_ssh_params() else {
                return false;
            };

            let changed = detach_certificate_from_ssh_params(&mut params, local_id, cloud_id);
            if changed {
                if let Ok(serialized) = serde_json::to_string(&params) {
                    connection.params = serialized;
                }
            }
            changed
        }
        ConnectionType::Redis => {
            let Ok(mut params) = connection.to_redis_params() else {
                return false;
            };

            let changed = detach_certificate_from_redis_params(&mut params, local_id, cloud_id);
            if changed {
                if let Ok(serialized) = serde_json::to_string(&params) {
                    connection.params = serialized;
                }
            }
            changed
        }
        ConnectionType::MongoDB => {
            let Ok(mut params) = connection.to_mongodb_params() else {
                return false;
            };

            let changed = detach_certificate_from_mongodb_params(&mut params, local_id, cloud_id);
            if changed {
                if let Ok(serialized) = serde_json::to_string(&params) {
                    connection.params = serialized;
                }
            }
            changed
        }
        _ => false,
    }
}

fn apply_certificate_to_db_config(
    config: &mut DbConnectionConfig,
    certificate: &Certificate,
) -> bool {
    let mut changed = false;

    if let Some(reference) = config.credential_ref.as_mut() {
        if reference.matches_certificate(certificate) {
            changed |= reference.sync_with_certificate(certificate);
            if certificate.kind == CertificateKind::UsernamePassword {
                if let Some(username) = certificate.username() {
                    if config.username != username {
                        config.username = username.to_string();
                        changed = true;
                    }
                }
                if let Some(password) = certificate.password() {
                    if config.password != password {
                        config.password = password.to_string();
                        changed = true;
                    }
                }
            }
        }
    }

    if let Some(reference) = config.ssh_tunnel_credential_ref.as_mut() {
        if reference.matches_certificate(certificate) {
            changed |= reference.sync_with_certificate(certificate);

            if let Some(username) = certificate.username() {
                if config.extra_params.get("ssh_username") != Some(&username.to_string()) {
                    config
                        .extra_params
                        .insert("ssh_username".to_string(), username.to_string());
                    changed = true;
                }
            }

            match certificate.kind {
                CertificateKind::UsernamePassword => {
                    if config.extra_params.get("ssh_auth_type").map(|v| v.as_str())
                        != Some("password")
                    {
                        config
                            .extra_params
                            .insert("ssh_auth_type".to_string(), "password".to_string());
                        changed = true;
                    }
                    let cert_pass = certificate.password().unwrap_or("");
                    if config.extra_params.get("ssh_password") != Some(&cert_pass.to_string()) {
                        config
                            .extra_params
                            .insert("ssh_password".to_string(), cert_pass.to_string());
                        changed = true;
                    }
                    changed |= config.extra_params.remove("ssh_private_key").is_some();
                    changed |= config
                        .extra_params
                        .remove("ssh_private_key_passphrase")
                        .is_some();
                }
                CertificateKind::SshPrivateKey => {
                    if config.extra_params.get("ssh_auth_type").map(|v| v.as_str())
                        != Some("private_key")
                    {
                        config
                            .extra_params
                            .insert("ssh_auth_type".to_string(), "private_key".to_string());
                        changed = true;
                    }
                    let key_content = certificate.ssh_private_key().unwrap_or("");
                    if config.extra_params.get("ssh_private_key")
                        != Some(&key_content.to_string())
                    {
                        config
                            .extra_params
                            .insert("ssh_private_key".to_string(), key_content.to_string());
                        changed = true;
                    }
                    match certificate.passphrase() {
                        Some(pass) if !pass.is_empty() => {
                            if config.extra_params.get("ssh_private_key_passphrase")
                                != Some(&pass.to_string())
                            {
                                config.extra_params.insert(
                                    "ssh_private_key_passphrase".to_string(),
                                    pass.to_string(),
                                );
                                changed = true;
                            }
                        }
                        _ => {
                            changed |= config
                                .extra_params
                                .remove("ssh_private_key_passphrase")
                                .is_some();
                        }
                    }
                    changed |= config.extra_params.remove("ssh_password").is_some();
                }
            }
        }
    }

    changed
}

fn detach_certificate_from_db_config(
    config: &mut DbConnectionConfig,
    local_id: Option<i64>,
    cloud_id: Option<&str>,
) -> bool {
    let mut changed = false;

    if config
        .credential_ref
        .as_ref()
        .is_some_and(|reference| reference.matches_ids(local_id, cloud_id))
    {
        config.credential_ref = None;
        changed = true;
    }

    if config
        .ssh_tunnel_credential_ref
        .as_ref()
        .is_some_and(|reference| reference.matches_ids(local_id, cloud_id))
    {
        config.ssh_tunnel_credential_ref = None;
        changed = true;
    }

    changed
}

fn apply_certificate_to_ssh_params(params: &mut SshParams, certificate: &Certificate) -> bool {
    let Some(reference) = params.credential_ref.as_mut() else {
        return false;
    };
    if !reference.matches_certificate(certificate) {
        return false;
    }

    let mut changed = reference.sync_with_certificate(certificate);

    if let Some(username) = certificate.username() {
        if params.username != username {
            params.username = username.to_string();
            changed = true;
        }
    }

    match certificate.kind {
        CertificateKind::UsernamePassword => {
            let password = certificate.password().unwrap_or("");
            if !matches!(params.auth_method, SshAuthMethod::Password { .. }) {
                params.auth_method = SshAuthMethod::Password {
                    password: password.to_string(),
                };
                changed = true;
            } else if let SshAuthMethod::Password { password: existing } = &mut params.auth_method {
                if *existing != password {
                    *existing = password.to_string();
                    changed = true;
                }
            }
        }
        CertificateKind::SshPrivateKey => {
            let key_content = certificate.key_path().unwrap_or("");
            let passphrase = certificate.passphrase().map(|s| s.to_string());
            if !matches!(params.auth_method, SshAuthMethod::PrivateKey { .. }) {
                params.auth_method = SshAuthMethod::PrivateKey {
                    ssh_private_key: key_content.to_string(),
                    passphrase,
                };
                changed = true;
            } else if let SshAuthMethod::PrivateKey {
                ssh_private_key: existing_key,
                passphrase: existing_passphrase,
            } = &mut params.auth_method
            {
                if *existing_key != key_content {
                    *existing_key = key_content.to_string();
                    changed = true;
                }
                if *existing_passphrase != passphrase {
                    *existing_passphrase = passphrase;
                    changed = true;
                }
            }
        }
    }

    changed
}

fn detach_certificate_from_ssh_params(
    params: &mut SshParams,
    local_id: Option<i64>,
    cloud_id: Option<&str>,
) -> bool {
    if params
        .credential_ref
        .as_ref()
        .is_some_and(|reference| reference.matches_ids(local_id, cloud_id))
    {
        params.credential_ref = None;
        return true;
    }
    false
}

fn apply_certificate_to_redis_params(params: &mut RedisParams, certificate: &Certificate) -> bool {
    let Some(reference) = params.credential_ref.as_mut() else {
        return false;
    };
    if !reference.matches_certificate(certificate) {
        return false;
    }

    let mut changed = reference.sync_with_certificate(certificate);
    if certificate.kind != CertificateKind::UsernamePassword {
        return changed;
    }

    let username = certificate.username().map(|s| s.to_string());
    if params.username != username {
        params.username = username;
        changed = true;
    }

    let password = certificate.password().map(|s| s.to_string());
    if params.password != password {
        params.password = password;
        changed = true;
    }

    changed
}

fn detach_certificate_from_redis_params(
    params: &mut RedisParams,
    local_id: Option<i64>,
    cloud_id: Option<&str>,
) -> bool {
    if params
        .credential_ref
        .as_ref()
        .is_some_and(|reference| reference.matches_ids(local_id, cloud_id))
    {
        params.credential_ref = None;
        return true;
    }
    false
}

fn apply_certificate_to_mongodb_params(
    params: &mut MongoDBParams,
    certificate: &Certificate,
) -> bool {
    let Some(reference) = params.credential_ref.as_mut() else {
        return false;
    };
    if !reference.matches_certificate(certificate) {
        return false;
    }

    let mut changed = reference.sync_with_certificate(certificate);
    if certificate.kind != CertificateKind::UsernamePassword {
        return changed;
    }

    let username = certificate.username().map(|s| s.to_string());
    if params.username != username {
        params.username = username;
        changed = true;
    }

    let password = certificate.password().map(|s| s.to_string());
    if params.password != password {
        params.password = password;
        changed = true;
    }

    changed
}

fn detach_certificate_from_mongodb_params(
    params: &mut MongoDBParams,
    local_id: Option<i64>,
    cloud_id: Option<&str>,
) -> bool {
    if params
        .credential_ref
        .as_ref()
        .is_some_and(|reference| reference.matches_ids(local_id, cloud_id))
    {
        params.credential_ref = None;
        return true;
    }
    false
}

/// 递归加密 JSON 中所有名为 password 或 passphrase 的字符串字段
fn encrypt_json_passwords(json_str: &str) -> String {
    match serde_json::from_str::<Value>(json_str) {
        Ok(mut value) => {
            encrypt_value(&mut value);
            serde_json::to_string(&value).unwrap_or_else(|_| json_str.to_string())
        }
        Err(_) => json_str.to_string(),
    }
}

/// 递归解密 JSON 中所有名为 password 或 passphrase 的字符串字段
fn decrypt_json_passwords(json_str: &str) -> String {
    match serde_json::from_str::<Value>(json_str) {
        Ok(mut value) => {
            decrypt_value(&mut value);
            serde_json::to_string(&value).unwrap_or_else(|_| json_str.to_string())
        }
        Err(_) => json_str.to_string(),
    }
}

/// 判断字段名是否为敏感字段
fn is_sensitive_field(key: &str) -> bool {
    key == "password"
        || key == "passphrase"
        || key.ends_with("_password")
        || key.ends_with("_passphrase")
}

/// 递归遍历 JSON Value，加密敏感字段
fn encrypt_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                if is_sensitive_field(key) {
                    if let Value::String(s) = val {
                        *s = crypto::encrypt_password(s);
                    }
                } else {
                    encrypt_value(val);
                }
            }
        }
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                encrypt_value(item);
            }
        }
        _ => {}
    }
}

/// 递归遍历 JSON Value，解密敏感字段
fn decrypt_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                if is_sensitive_field(key) {
                    if let Value::String(s) = val {
                        *s = crypto::decrypt_password(s);
                    }
                } else {
                    decrypt_value(val);
                }
            }
        }
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                decrypt_value(item);
            }
        }
        _ => {}
    }
}

/// 检测 params 中是否存在“已加密字段解密失败”的情况。
///
/// 规则：敏感字段（password/passphrase）若以 ENC: 开头，且解密结果为空，视为失败。
pub fn has_decrypt_failure_in_sensitive_fields(json_str: &str) -> bool {
    match serde_json::from_str::<Value>(json_str) {
        Ok(value) => has_decrypt_failure_in_value(&value),
        Err(_) => false,
    }
}

fn has_decrypt_failure_in_value(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, val)| {
            if is_sensitive_field(key) {
                if let Value::String(s) = val {
                    return crypto::is_encrypted(s) && crypto::decrypt_password(s).is_empty();
                }
                false
            } else {
                has_decrypt_failure_in_value(val)
            }
        }),
        Value::Array(arr) => arr.iter().any(has_decrypt_failure_in_value),
        _ => false,
    }
}

/// Generic key-value storage model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyValue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    pub key: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

impl KeyValue {
    pub fn new(key: String, value: String) -> Self {
        Self {
            id: None,
            key,
            value,
            created_at: None,
            updated_at: None,
        }
    }
}

pub fn parse_db_type(s: &str) -> DatabaseType {
    match s {
        "MySQL" => DatabaseType::MySQL,
        "PostgreSQL" => DatabaseType::PostgreSQL,
        "SQLite" => DatabaseType::SQLite,
        "DuckDB" => DatabaseType::DuckDB,
        _ => DatabaseType::MySQL,
    }
}

#[cfg(test)]
mod serial_tests {
    use super::*;

    #[test]
    fn serial_params_serialize_deserialize() {
        let params = SerialParams {
            port_name: "/dev/ttyUSB0".to_string(),
            baud_rate: 115200,
            data_bits: 8,
            stop_bits: 1,
            parity: SerialParity::None,
            flow_control: SerialFlowControl::None,
        };
        let json = serde_json::to_string(&params).unwrap();
        let p2: SerialParams = serde_json::from_str(&json).unwrap();
        assert_eq!(p2.port_name, "/dev/ttyUSB0");
        assert_eq!(p2.baud_rate, 115200);
        assert_eq!(p2.data_bits, 8);
        assert_eq!(p2.stop_bits, 1);
        assert_eq!(p2.parity, SerialParity::None);
        assert_eq!(p2.flow_control, SerialFlowControl::None);
    }

    #[test]
    fn serial_params_defaults_from_minimal_json() {
        let json = r#"{"port_name":"/dev/tty0"}"#;
        let p: SerialParams = serde_json::from_str(json).unwrap();
        assert_eq!(p.port_name, "/dev/tty0");
        assert_eq!(p.baud_rate, 115200);
        assert_eq!(p.data_bits, 8);
        assert_eq!(p.stop_bits, 1);
        assert_eq!(p.parity, SerialParity::None);
        assert_eq!(p.flow_control, SerialFlowControl::None);
    }

    #[test]
    fn stored_connection_serial_roundtrip() {
        let params = SerialParams {
            port_name: "/dev/cu.usbserial-1420".to_string(),
            baud_rate: 9600,
            data_bits: 7,
            stop_bits: 2,
            parity: SerialParity::Even,
            flow_control: SerialFlowControl::Hardware,
        };
        let conn = StoredConnection::new_serial("我的串口".to_string(), params, Some(42));
        assert_eq!(conn.connection_type, ConnectionType::Serial);
        assert_eq!(conn.name, "我的串口");
        assert_eq!(conn.workspace_id, Some(42));

        let rt = conn.to_serial_params().unwrap();
        assert_eq!(rt.port_name, "/dev/cu.usbserial-1420");
        assert_eq!(rt.baud_rate, 9600);
        assert_eq!(rt.data_bits, 7);
        assert_eq!(rt.stop_bits, 2);
        assert_eq!(rt.parity, SerialParity::Even);
        assert_eq!(rt.flow_control, SerialFlowControl::Hardware);
    }

    #[test]
    fn connection_type_serial_methods() {
        assert_eq!(ConnectionType::Serial.label(), "串口");
        assert_eq!(ConnectionType::from_str("Serial"), ConnectionType::Serial);
        assert_eq!(format!("{}", ConnectionType::Serial), "Serial");
        assert!(ConnectionType::all().contains(&ConnectionType::Serial));
    }

    #[test]
    fn serial_enums_defaults_and_labels() {
        assert_eq!(SerialParity::default(), SerialParity::None);
        assert_eq!(SerialFlowControl::default(), SerialFlowControl::None);
        assert_eq!(SerialParity::all().len(), 3);
        assert_eq!(SerialFlowControl::all().len(), 3);
        assert_eq!(SerialParity::Odd.label(), "Odd");
        assert_eq!(SerialParity::Even.label(), "Even");
        assert_eq!(SerialFlowControl::Software.label(), "XON/XOFF");
        assert_eq!(SerialFlowControl::Hardware.label(), "RTS/CTS");
    }

    #[test]
    fn certificate_reference_prefers_cloud_id_when_present() {
        let reference = CertificateReference {
            local_id: Some(1),
            cloud_id: Some("certificate-cloud-1".to_string()),
        };

        assert!(reference.matches_ids(Some(99), Some("certificate-cloud-1")));
        assert!(!reference.matches_ids(Some(1), Some("certificate-cloud-2")));
        assert!(!reference.matches_ids(Some(1), None));
    }

    #[test]
    fn certificate_reference_falls_back_to_local_id_before_sync() {
        let reference = CertificateReference {
            local_id: Some(7),
            cloud_id: None,
        };

        assert!(reference.matches_ids(Some(7), None));
        assert!(!reference.matches_ids(Some(8), None));
    }

    #[test]
    fn ssh_auth_method_agent_serialize_deserialize() {
        let auth = SshAuthMethod::Agent;
        let json = serde_json::to_string(&auth).expect("Agent 认证方式应可序列化");
        let parsed: SshAuthMethod =
            serde_json::from_str(&json).expect("Agent 认证方式应可反序列化");
        assert!(matches!(parsed, SshAuthMethod::Agent));
    }

    #[test]
    fn ssh_params_defaults_legacy_kex_to_false_when_missing() {
        let json = r#"{"host":"127.0.0.1","port":22,"username":"root","auth_method":"Agent"}"#;
        let parsed: SshParams = serde_json::from_str(json).expect("旧格式 SSH 参数应可反序列化");
        assert!(!parsed.enable_legacy_kex);
    }

    #[test]
    fn ssh_auth_method_auto_publickey_serialize_deserialize() {
        let auth = SshAuthMethod::AutoPublicKey;
        let json = serde_json::to_string(&auth).expect("自动公钥认证方式应可序列化");
        let parsed: SshAuthMethod =
            serde_json::from_str(&json).expect("自动公钥认证方式应可反序列化");
        assert!(matches!(parsed, SshAuthMethod::AutoPublicKey));
    }
}
