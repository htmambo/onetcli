use crate::ipc::registry::EXTERNAL_DRIVER_ID_PARAM;
use crate::ipc::{IpcDriverManifest, IpcDriverRegistry};
use gpui_component::{Icon, IconName, IconNamed, Sizable, Size};
use one_core::storage::{DatabaseType, DbConnectionConfig};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IpcDriverDisplay {
    pub driver_id: String,
    pub name: String,
    pub icon_asset_path: Option<String>,
    pub icon_file_path: Option<PathBuf>,
}

impl IpcDriverManifest {
    pub fn icon_asset_path(&self) -> Option<String> {
        self.icon_asset_path_for(&self.ui.icon, "icon")
    }

    pub fn preferred_icon_asset_path(&self) -> Option<String> {
        self.icon_asset_path()
    }

    pub fn icon_file_path(&self) -> Option<PathBuf> {
        self.icon_file_path_for(&self.ui.icon)
    }

    pub fn preferred_icon_file_path(&self) -> Option<PathBuf> {
        self.icon_file_path()
    }

    fn icon_asset_path_for(&self, icon: &str, resource: &str) -> Option<String> {
        let icon = icon.trim();
        if icon.is_empty() {
            return None;
        }
        let asset_path = builtin_icon_asset_path(icon)
            .unwrap_or_else(|| format!("driver://{}/{resource}{}", self.id, icon_extension(icon)));
        Some(asset_path)
    }

    fn icon_file_path_for(&self, icon: &str) -> Option<PathBuf> {
        let icon = icon.trim();
        if icon.is_empty() || builtin_icon_asset_path(icon).is_some() {
            return None;
        }
        Some(self.manifest_dir.join(icon))
    }
}

pub fn driver_icon_from_asset_path(path: impl Into<String>, size: impl Into<Size>) -> Icon {
    Icon::default().path(path.into()).color().with_size(size)
}

pub fn driver_icon_from_file_path(path: impl Into<PathBuf>, size: impl Into<Size>) -> Icon {
    Icon::default()
        .file_path(path.into())
        .color()
        .with_size(size)
}

impl IpcDriverRegistry {
    pub fn display_for_driver_id(&self, driver_id: &str) -> Option<IpcDriverDisplay> {
        let driver = self.find(driver_id)?;
        Some(IpcDriverDisplay {
            driver_id: driver.id.clone(),
            name: driver.name.clone(),
            icon_asset_path: driver.preferred_icon_asset_path(),
            icon_file_path: driver.preferred_icon_file_path(),
        })
    }

    pub fn display_for_config(&self, config: &DbConnectionConfig) -> Option<IpcDriverDisplay> {
        if config.database_type != DatabaseType::External {
            return None;
        }
        let driver_id = config.get_param(EXTERNAL_DRIVER_ID_PARAM)?;
        self.display_for_driver_id(driver_id)
    }
}

fn builtin_icon_asset_path(icon: &str) -> Option<String> {
    let icon_name = match icon {
        "Database" => IconName::Database,
        "DuckDB" => IconName::DuckDB,
        "ClickHouse" | "ClickHouseColor" => IconName::ClickHouseColor,
        "MongoDB" => IconName::MongoDB,
        "MySQL" | "MySQLColor" => IconName::MySQLColor,
        "PostgreSQL" | "PostgreSQLColor" => IconName::PostgreSQLColor,
        "Redis" | "RedisColor" => IconName::RedisColor,
        "SQLite" | "SQLiteColor" => IconName::SQLiteColor,
        "Server" => IconName::Server,
        "Terminal" | "TerminalColor" => IconName::TerminalColor,
        _ => return None,
    };
    Some(icon_name.path().to_string())
}

fn icon_extension(icon: &str) -> String {
    Path::new(icon)
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| !extension.trim().is_empty())
        .map(|extension| format!(".{extension}"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::registry::{
        IpcDriverEntry, IpcDriverTransport, IpcDriverUi, EXTERNAL_DRIVER_ID_PARAM,
    };
    use std::collections::HashMap;

    fn manifest(id: &str, icon: &str, dir: &str) -> IpcDriverManifest {
        IpcDriverManifest {
            id: id.to_string(),
            name: format!("{id}-name"),
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
                icon: icon.to_string(),
                default_port: None,
                form: None,
            },
            manifest_dir: PathBuf::from(dir),
        }
    }

    fn external_config(driver_id: &str) -> DbConnectionConfig {
        let mut extra_params = HashMap::new();
        extra_params.insert(EXTERNAL_DRIVER_ID_PARAM.to_string(), driver_id.to_string());
        DbConnectionConfig {
            id: "1".to_string(),
            database_type: DatabaseType::External,
            name: "saved".to_string(),
            host: "localhost".to_string(),
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
    fn display_prefers_builtin_asset_for_named_icons() {
        let registry = IpcDriverRegistry::from_drivers(vec![manifest("demo", "DuckDB", "/d")]);
        let display = registry.display_for_driver_id("demo").unwrap();
        assert_eq!(display.icon_asset_path.as_deref(), Some("icons/duckdb.svg"));
        assert_eq!(display.icon_file_path, None);
    }

    #[test]
    fn display_uses_file_path_for_custom_icons() {
        let registry =
            IpcDriverRegistry::from_drivers(vec![manifest("demo", "icons/demo.svg", "/drivers/demo")]);
        let display = registry.display_for_driver_id("demo").unwrap();
        assert_eq!(
            display.icon_file_path,
            Some(PathBuf::from("/drivers/demo/icons/demo.svg"))
        );
        assert!(display.icon_asset_path.is_some());
    }

    #[test]
    fn display_for_config_requires_external_type_and_driver_id() {
        let registry = IpcDriverRegistry::from_drivers(vec![manifest("demo", "DuckDB", "/d")]);
        assert!(registry.display_for_config(&external_config("demo")).is_some());

        let mut mysql = external_config("demo");
        mysql.database_type = DatabaseType::MySQL;
        assert!(registry.display_for_config(&mysql).is_none());

        let mut no_driver = external_config("demo");
        no_driver.extra_params.clear();
        assert!(registry.display_for_config(&no_driver).is_none());
    }
}
