use db::ipc::{IpcDriverRegistry, driver_icon_from_asset_path, driver_icon_from_file_path};
use gpui_component::{Icon, Size};
use one_core::storage::DbConnectionConfig;

pub(crate) fn external_driver_icon_for_config(
    config: &DbConnectionConfig,
    size: impl Into<Size>,
) -> Option<Icon> {
    external_driver_icon_for_config_with_registry(config, size, &IpcDriverRegistry::load_default())
}

fn external_driver_icon_for_config_with_registry(
    config: &DbConnectionConfig,
    size: impl Into<Size>,
    registry: &IpcDriverRegistry,
) -> Option<Icon> {
    let display = registry.display_for_config(config)?;
    let icon_asset_path = display.icon_asset_path;
    let icon_file_path = display.icon_file_path;
    if icon_asset_path.is_none() && icon_file_path.is_none() {
        return None;
    }
    Some(match icon_file_path {
        Some(path) => driver_icon_from_file_path(path, size),
        None => driver_icon_from_asset_path(icon_asset_path?, size),
    })
}

pub(crate) fn external_driver_icon_for_driver_id(
    driver_id: &str,
    size: impl Into<Size>,
) -> Option<Icon> {
    let registry = IpcDriverRegistry::load_default();
    let display = registry.display_for_driver_id(driver_id)?;
    match (display.icon_file_path, display.icon_asset_path) {
        (Some(path), _) => Some(driver_icon_from_file_path(path, size)),
        (None, Some(path)) => Some(driver_icon_from_asset_path(path, size)),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use db::ipc::{
        EXTERNAL_DRIVER_ID_PARAM, IpcDriverEntry, IpcDriverManifest, IpcDriverRegistry,
        IpcDriverTransport, IpcDriverUi,
    };
    use one_core::storage::DatabaseType;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn manifest(id: &str, icon: &str) -> IpcDriverManifest {
        IpcDriverManifest {
            id: id.to_string(),
            name: format!("{id}-name"),
            category: None,
            description: String::new(),
            version: String::new(),
            protocol_version: None,
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
            connection: Default::default(),
            ui: IpcDriverUi {
                icon: icon.to_string(),
                default_port: None,
                form: None,
            },
            manifest_dir: PathBuf::from(format!("/drivers/{id}")),
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
    fn external_driver_icon_prefers_file_path_over_asset() {
        let registry = IpcDriverRegistry::from_drivers(vec![manifest("demo", "icons/demo.svg")]);
        let icon = external_driver_icon_for_config_with_registry(
            &external_config("demo"),
            Size::Medium,
            &registry,
        );
        assert!(icon.is_some());
    }

    #[test]
    fn external_driver_icon_none_for_non_external() {
        let registry = IpcDriverRegistry::from_drivers(vec![manifest("demo", "MySQL")]);
        let mut config = external_config("demo");
        config.database_type = DatabaseType::MySQL;
        assert!(
            external_driver_icon_for_config_with_registry(&config, Size::Medium, &registry)
                .is_none()
        );
    }
}
