use one_core::storage::DbConnectionConfig;
use serde_json::{Value, json};

pub use ipc::protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

pub fn connection_config_params(config: &DbConnectionConfig) -> Value {
    connection_config_params_with_target(config, &config.host, config.port)
}

/// 构造 connect 参数，但使用宿主解析后的实际连接目标。
///
/// IPC driver 不负责 SSH/TCP tunnel；宿主若启用 SSH 隧道，会先建立本地端口转发，
/// 再把这里的 host/port 改成本地映射地址。
pub fn connection_config_params_with_target(
    config: &DbConnectionConfig,
    target_host: &str,
    target_port: u16,
) -> Value {
    json!({
        "config": {
            "id": config.id,
            "database_type": config.database_type.as_str(),
            "name": config.name,
            "host": target_host,
            "port": target_port,
            "username": config.username,
            "password": config.password,
            "database": config.database,
            "service_name": config.service_name,
            "sid": config.sid,
            "extra_params": config.extra_params,
        }
    })
}

pub fn empty_params() -> Value {
    json!({})
}

pub fn sql_params(sql: &str) -> Value {
    json!({ "sql": sql })
}

pub fn database_params(database: &str) -> Value {
    json!({ "database": database })
}

pub fn schema_params(schema: &str) -> Value {
    json!({ "schema": schema })
}

pub fn table_metadata_params(database: &str, schema: Option<String>, table: &str) -> Value {
    json!({ "database": database, "schema": schema, "table": table })
}

pub fn database_metadata_params(database: &str, schema: Option<String>) -> Value {
    json!({ "database": database, "schema": schema })
}

#[cfg(test)]
mod tests {
    use super::*;
    use one_core::storage::DatabaseType;
    use std::collections::HashMap;

    #[test]
    fn builds_json_rpc_request() {
        let request = JsonRpcRequest::new(7, "ping", empty_params());
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(value["jsonrpc"], "2.0");
        assert_eq!(value["id"], 7);
        assert_eq!(value["method"], "ping");
    }

    #[test]
    fn connection_config_uses_resolved_target() {
        let mut extra_params = HashMap::new();
        extra_params.insert("ssh_tunnel_enabled".into(), "true".into());
        extra_params.insert("ssh_target_host".into(), "db.internal".into());

        let config = DbConnectionConfig {
            id: "ext-1".into(),
            name: "External".into(),
            database_type: DatabaseType::External,
            host: "db.internal".into(),
            port: 5432,
            username: "u".into(),
            password: "p".into(),
            database: Some("main".into()),
            service_name: None,
            sid: None,
            credential_ref: None,
            ssh_tunnel_credential_ref: None,
            workspace_id: None,
            extra_params,
        };

        let params = connection_config_params_with_target(&config, "127.0.0.1", 16543);
        assert_eq!(params["config"]["host"], "127.0.0.1");
        assert_eq!(params["config"]["port"], 16543);
        assert_eq!(
            params["config"]["extra_params"]["ssh_target_host"],
            "db.internal"
        );
        assert_eq!(params["config"]["name"], "External");
    }

    #[test]
    fn connection_config_defaults_to_config_host_port() {
        let config = DbConnectionConfig {
            id: "ext-2".into(),
            name: "Direct".into(),
            database_type: DatabaseType::External,
            host: "db.example".into(),
            port: 3306,
            username: "root".into(),
            password: "secret".into(),
            database: None,
            service_name: None,
            sid: None,
            credential_ref: None,
            ssh_tunnel_credential_ref: None,
            workspace_id: None,
            extra_params: HashMap::new(),
        };

        let params = connection_config_params(&config);
        assert_eq!(params["config"]["host"], "db.example");
        assert_eq!(params["config"]["port"], 3306);
    }
}
