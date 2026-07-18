use crate::external_driver_display::external_driver_icon_for_driver_id;
use db::ipc::IpcDriverRegistry;
use gpui::{Styled, px};
use gpui_component::{Icon, IconName, Sizable};
use one_core::storage::DatabaseType;
use rust_i18n::t;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NewConnectionCategory {
    All,
    Database,
    DomesticDatabase,
    NoSql,
    Terminal,
}

impl NewConnectionCategory {
    pub(super) fn all() -> [Self; 5] {
        [
            Self::All,
            Self::Database,
            Self::DomesticDatabase,
            Self::NoSql,
            Self::Terminal,
        ]
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::All => "全部",
            Self::Database => "数据库",
            Self::DomesticDatabase => "国产数据库",
            Self::NoSql => "NoSQL",
            Self::Terminal => "终端",
        }
    }

    pub(super) fn icon(self) -> IconName {
        match self {
            Self::All => IconName::AppsColor,
            Self::Database => IconName::Database,
            Self::DomesticDatabase => IconName::Database,
            Self::NoSql => IconName::Server,
            Self::Terminal => IconName::Terminal,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum NewConnectionKind {
    Ssh,
    Terminal,
    Redis,
    MongoDB,
    Serial,
    PortForwarding,
    Rdp,
    Vnc,
    Database(DatabaseType),
    ExternalDatabase {
        driver_id: String,
        name: String,
        description: String,
        category: Option<String>,
    },
}

impl NewConnectionKind {
    pub(super) fn all() -> Vec<Self> {
        let mut items = vec![
            Self::Ssh,
            Self::Terminal,
            Self::Redis,
            Self::MongoDB,
            Self::Serial,
            Self::PortForwarding,
            Self::Rdp,
            Self::Vnc,
        ];
        items.extend(
            DatabaseType::builtin_all()
                .iter()
                .copied()
                .map(Self::Database),
        );
        items.extend(
            IpcDriverRegistry::load_default()
                .drivers()
                .iter()
                .map(|driver| Self::ExternalDatabase {
                    driver_id: driver.id.clone(),
                    name: driver.name.clone(),
                    description: driver.description.clone(),
                    category: driver.category.clone(),
                }),
        );
        items
    }

    pub(super) fn label(&self) -> String {
        match self {
            Self::Ssh => "SSH / SFTP".to_string(),
            Self::Terminal => "Terminal".to_string(),
            Self::Redis => "Redis".to_string(),
            Self::MongoDB => "MongoDB".to_string(),
            Self::Serial => t!("Serial.new").to_string(),
            Self::PortForwarding => t!("NewConnection.port_forwarding").to_string(),
            Self::Rdp => t!("NewConnection.rdp").to_string(),
            Self::Vnc => t!("NewConnection.vnc").to_string(),
            Self::Database(db_type) => db_type.as_str().to_string(),
            Self::ExternalDatabase { name, .. } => name.clone(),
        }
    }

    pub(super) fn description(&self) -> String {
        match self {
            Self::Ssh => "远程服务器终端与文件连接".to_string(),
            Self::Terminal => "打开一个本地终端标签页".to_string(),
            Self::Redis => "Redis 单机、哨兵或集群连接".to_string(),
            Self::MongoDB => "MongoDB 数据库连接".to_string(),
            Self::Serial => "串口设备连接".to_string(),
            Self::PortForwarding => t!("NewConnection.description_port_forwarding").to_string(),
            Self::Rdp => t!("NewConnection.description_rdp").to_string(),
            Self::Vnc => t!("NewConnection.description_vnc").to_string(),
            Self::Database(_) => "关系型数据库连接".to_string(),
            Self::ExternalDatabase { description, .. } => description.clone(),
        }
    }

    pub(super) fn category(&self) -> NewConnectionCategory {
        match self {
            Self::Ssh | Self::Terminal | Self::Serial | Self::PortForwarding | Self::Rdp | Self::Vnc => NewConnectionCategory::Terminal,
            Self::Redis | Self::MongoDB => NewConnectionCategory::NoSql,
            Self::Database(_) => NewConnectionCategory::Database,
            Self::ExternalDatabase { category, .. } => {
                if is_domestic_database_category(category.as_deref()) {
                    NewConnectionCategory::DomesticDatabase
                } else {
                    NewConnectionCategory::Database
                }
            }
        }
    }

    pub(super) fn icon(&self) -> Icon {
        match self {
            Self::Ssh => IconName::TerminalColor.color().with_size(px(40.0)),
            Self::Terminal => IconName::Terminal
                .mono()
                .text_color(gpui::rgb(0x8b5cf6))
                .with_size(px(40.0)),
            Self::Redis => IconName::Redis.color().with_size(px(40.0)),
            Self::MongoDB => IconName::MongoDB.color().with_size(px(40.0)),
            Self::Serial => IconName::SerialPort.color().with_size(px(40.0)),
            Self::PortForwarding => IconName::PortForwardingColor.color().with_size(px(40.0)),
            Self::Rdp => IconName::Rdp.color().with_size(px(40.0)),
            Self::Vnc => IconName::Vnc.color().with_size(px(40.0)),
            Self::Database(db_type) => db_type.as_icon().with_size(px(40.0)),
            Self::ExternalDatabase { driver_id, .. } => {
                external_driver_icon_for_driver_id(driver_id, px(40.0)).unwrap_or_else(|| {
                    IconName::Database.color().with_size(px(40.0))
                })
            }
        }
    }
}

fn is_domestic_database_category(category: Option<&str>) -> bool {
    category == Some("domestic_database")
}

#[cfg(test)]
mod tests {
    use super::*;
    use db::ipc::{
        IpcDriverEntry, IpcDriverManifest, IpcDriverRegistry, IpcDriverTransport,
    };
    use std::path::PathBuf;

    fn manifest(id: &str, name: &str) -> IpcDriverManifest {
        manifest_with_category(id, name, None)
    }

    fn manifest_with_category(
        id: &str,
        name: &str,
        category: Option<&str>,
    ) -> IpcDriverManifest {
        IpcDriverManifest {
            id: id.into(),
            name: name.into(),
            category: category.map(str::to_string),
            description: String::new(),
            version: String::new(),
            entry: IpcDriverEntry {
                command: "driver".into(),
                args: Vec::new(),
                working_dir: None,
                commands: Default::default(),
                env_from_config: Default::default(),
            },
            transport: IpcDriverTransport::local_socket(format!("{id}.sock")),
            dialect: Default::default(),
            capabilities: None,
            connection: Default::default(),
            ui: Default::default(),
            manifest_dir: PathBuf::from("/tmp"),
        }
    }

    #[test]
    fn connection_categories_include_domestic_database() {
        assert_eq!(
            NewConnectionCategory::all(),
            [
                NewConnectionCategory::All,
                NewConnectionCategory::Database,
                NewConnectionCategory::DomesticDatabase,
                NewConnectionCategory::NoSql,
                NewConnectionCategory::Terminal,
            ]
        );
        assert_eq!(
            "国产数据库",
            NewConnectionCategory::DomesticDatabase.label()
        );
    }

    #[test]
    fn ipc_database_driver_uses_manifest_category_for_domestic_database() {
        let registry = IpcDriverRegistry::from_drivers(vec![
            manifest("dm", "Dameng DM"),
            manifest_with_category("kingbase", "KingbaseES", Some("domestic_database")),
            manifest_with_category("gbase8s", "GBase 8s", Some("domestic_database")),
            manifest("iotdb", "Apache IoTDB"),
        ]);

        let mut categories: Vec<(String, NewConnectionCategory)> = registry
            .drivers()
            .iter()
            .map(|driver| {
                let kind = NewConnectionKind::ExternalDatabase {
                    driver_id: driver.id.clone(),
                    name: driver.name.clone(),
                    description: driver.description.clone(),
                    category: driver.category.clone(),
                };
                (driver.id.clone(), kind.category())
            })
            .collect();
        categories.sort_by(|left, right| left.0.cmp(&right.0));

        assert_eq!(
            categories,
            vec![
                ("dm".to_string(), NewConnectionCategory::Database),
                (
                    "gbase8s".to_string(),
                    NewConnectionCategory::DomesticDatabase
                ),
                ("iotdb".to_string(), NewConnectionCategory::Database),
                (
                    "kingbase".to_string(),
                    NewConnectionCategory::DomesticDatabase
                ),
            ]
        );
    }
}
