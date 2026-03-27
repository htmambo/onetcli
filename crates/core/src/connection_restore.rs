use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::storage::{ConnectionType, get_config_dir, now};
use crate::tab_container::TabContainerState;

const CONNECTION_RESTORE_STATE_FILE: &str = "connection_restore_state.json";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionRestoreKind {
    SshTerminal,
    SerialTerminal,
    Sftp,
    Database,
    DatabaseWorkspace,
    Redis,
    RedisWorkspace,
    MongoDb,
    MongoDbWorkspace,
}

impl ConnectionRestoreKind {
    pub fn connection_type(&self) -> ConnectionType {
        match self {
            ConnectionRestoreKind::SshTerminal | ConnectionRestoreKind::Sftp => {
                ConnectionType::SshSftp
            }
            ConnectionRestoreKind::SerialTerminal => ConnectionType::Serial,
            ConnectionRestoreKind::Database | ConnectionRestoreKind::DatabaseWorkspace => {
                ConnectionType::Database
            }
            ConnectionRestoreKind::Redis | ConnectionRestoreKind::RedisWorkspace => {
                ConnectionType::Redis
            }
            ConnectionRestoreKind::MongoDb | ConnectionRestoreKind::MongoDbWorkspace => {
                ConnectionType::MongoDB
            }
        }
    }

    pub fn is_workspace(&self) -> bool {
        matches!(
            self,
            ConnectionRestoreKind::DatabaseWorkspace
                | ConnectionRestoreKind::RedisWorkspace
                | ConnectionRestoreKind::MongoDbWorkspace
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionRestorePayload {
    pub kind: ConnectionRestoreKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_connection_id: Option<i64>,
    pub title: String,
}

impl ConnectionRestorePayload {
    pub fn is_valid(&self) -> bool {
        if self.title.trim().is_empty() {
            return false;
        }

        if self.kind.is_workspace() {
            self.workspace_id.is_some()
        } else {
            self.connection_id.is_some()
        }
    }

    pub fn into_tab_data(self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionRestoreItem {
    pub snapshot_id: String,
    pub kind: ConnectionRestoreKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_connection_id: Option<i64>,
    pub title: String,
}

impl ConnectionRestoreItem {
    pub fn connection_type(&self) -> ConnectionType {
        self.kind.connection_type()
    }

    pub fn is_workspace(&self) -> bool {
        self.kind.is_workspace()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionRestoreSnapshot {
    pub version: usize,
    pub saved_at: i64,
    pub items: Vec<ConnectionRestoreItem>,
}

impl Default for ConnectionRestoreSnapshot {
    fn default() -> Self {
        Self {
            version: 1,
            saved_at: now(),
            items: Vec::new(),
        }
    }
}

fn get_connection_restore_path() -> Result<PathBuf> {
    let config_dir = get_config_dir()?;
    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir)?;
    }
    Ok(config_dir.join(CONNECTION_RESTORE_STATE_FILE))
}

fn save_connection_restore_snapshot_to_path(
    snapshot: &ConnectionRestoreSnapshot,
    path: &Path,
) -> Result<()> {
    if snapshot.items.is_empty() {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(snapshot)?;
    std::fs::write(path, json)?;
    Ok(())
}

fn load_connection_restore_snapshot_from_path(path: &Path) -> Result<ConnectionRestoreSnapshot> {
    if !path.exists() {
        return Ok(ConnectionRestoreSnapshot::default());
    }

    let json = std::fs::read_to_string(path).context("读取连接恢复快照文件失败")?;
    let snapshot = serde_json::from_str(&json).context("解析连接恢复快照 JSON 失败")?;
    Ok(snapshot)
}

pub fn connection_restore_state_exists() -> bool {
    get_connection_restore_path()
        .map(|path| path.exists())
        .unwrap_or(false)
}

pub fn save_connection_restore_snapshot(snapshot: &ConnectionRestoreSnapshot) -> Result<()> {
    let path = get_connection_restore_path()?;
    save_connection_restore_snapshot_to_path(snapshot, &path)
}

pub fn load_connection_restore_snapshot() -> Result<ConnectionRestoreSnapshot> {
    let path = get_connection_restore_path()?;
    load_connection_restore_snapshot_from_path(&path)
}

pub fn clear_connection_restore_snapshot() -> Result<()> {
    let path = get_connection_restore_path()?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

pub fn restore_payload_from_tab_data(data: &Value) -> Option<ConnectionRestorePayload> {
    serde_json::from_value::<ConnectionRestorePayload>(data.clone())
        .ok()
        .filter(ConnectionRestorePayload::is_valid)
}

pub fn snapshot_from_tab_state(state: &TabContainerState) -> ConnectionRestoreSnapshot {
    let items = state
        .tabs
        .iter()
        .filter_map(|tab| {
            let payload = restore_payload_from_tab_data(&tab.data)?;
            Some(ConnectionRestoreItem {
                snapshot_id: tab.id.to_string(),
                kind: payload.kind,
                connection_id: payload.connection_id,
                workspace_id: payload.workspace_id,
                active_connection_id: payload.active_connection_id,
                title: payload.title,
            })
        })
        .collect();

    ConnectionRestoreSnapshot {
        items,
        ..ConnectionRestoreSnapshot::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tab_container::{TabContainerConfig, TabItemState};

    fn temp_path(name: &str) -> PathBuf {
        let unique = format!(
            "onetcli-connection-restore-{}-{}.json",
            std::process::id(),
            name
        );
        std::env::temp_dir().join(unique)
    }

    #[test]
    fn 从标签状态中过滤出可恢复项() {
        let restorable = ConnectionRestorePayload {
            kind: ConnectionRestoreKind::SshTerminal,
            connection_id: Some(42),
            workspace_id: None,
            active_connection_id: None,
            title: "服务器".to_string(),
        };
        let invalid_workspace = ConnectionRestorePayload {
            kind: ConnectionRestoreKind::DatabaseWorkspace,
            connection_id: None,
            workspace_id: None,
            active_connection_id: None,
            title: "空工作区".to_string(),
        };
        let state = TabContainerState {
            version: Some(1),
            tabs: vec![
                TabItemState {
                    id: "ssh-terminal-42-1".into(),
                    from: "ssh".into(),
                    key: "Terminal".into(),
                    data: restorable.into_tab_data(),
                },
                TabItemState {
                    id: "home".into(),
                    from: "app".into(),
                    key: "Home".into(),
                    data: Value::Null,
                },
                TabItemState {
                    id: "workspace-db".into(),
                    from: "home".into(),
                    key: "Database".into(),
                    data: invalid_workspace.into_tab_data(),
                },
            ],
            active_index: 0,
            config: TabContainerConfig::default(),
        };

        let snapshot = snapshot_from_tab_state(&state);
        assert_eq!(snapshot.items.len(), 1);
        assert_eq!(snapshot.items[0].snapshot_id, "ssh-terminal-42-1");
        assert_eq!(snapshot.items[0].connection_id, Some(42));
    }

    #[test]
    fn 工作区恢复项至少需要工作区_id() {
        let payload = ConnectionRestorePayload {
            kind: ConnectionRestoreKind::RedisWorkspace,
            connection_id: None,
            workspace_id: None,
            active_connection_id: Some(7),
            title: "Redis 工作区".to_string(),
        };

        assert!(!payload.is_valid());
    }

    #[test]
    fn 快照文件支持读写往返() {
        let path = temp_path("roundtrip");
        if path.exists() {
            std::fs::remove_file(&path).expect("测试前清理临时文件失败");
        }

        let snapshot = ConnectionRestoreSnapshot {
            version: 1,
            saved_at: 123,
            items: vec![ConnectionRestoreItem {
                snapshot_id: "sftp-1-2".to_string(),
                kind: ConnectionRestoreKind::Sftp,
                connection_id: Some(1),
                workspace_id: None,
                active_connection_id: None,
                title: "SFTP".to_string(),
            }],
        };

        save_connection_restore_snapshot_to_path(&snapshot, &path).expect("写入快照失败");
        let loaded = load_connection_restore_snapshot_from_path(&path).expect("读取快照失败");
        assert_eq!(loaded, snapshot);

        std::fs::remove_file(&path).expect("测试后清理临时文件失败");
    }
}
