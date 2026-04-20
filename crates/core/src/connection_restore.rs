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
    LocalTerminal,
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
    pub fn connection_type(&self) -> Option<ConnectionType> {
        match self {
            ConnectionRestoreKind::LocalTerminal => None,
            ConnectionRestoreKind::SshTerminal | ConnectionRestoreKind::Sftp => {
                Some(ConnectionType::SshSftp)
            }
            ConnectionRestoreKind::SerialTerminal => Some(ConnectionType::Serial),
            ConnectionRestoreKind::Database | ConnectionRestoreKind::DatabaseWorkspace => {
                Some(ConnectionType::Database)
            }
            ConnectionRestoreKind::Redis | ConnectionRestoreKind::RedisWorkspace => {
                Some(ConnectionType::Redis)
            }
            ConnectionRestoreKind::MongoDb | ConnectionRestoreKind::MongoDbWorkspace => {
                Some(ConnectionType::MongoDB)
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalTerminalRestoreState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buffer_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pty_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefer_live_restore: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_ligatures: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_height_scale: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor_blink: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_copy: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub middle_click_paste: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirm_multiline_paste: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirm_high_risk_command: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_behavior: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SshTerminalRestoreState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buffer_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectionRestorePayload {
    pub kind: ConnectionRestoreKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_connection_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_terminal: Option<LocalTerminalRestoreState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_terminal: Option<SshTerminalRestoreState>,
    pub title: String,
}

impl ConnectionRestorePayload {
    pub fn is_valid(&self) -> bool {
        if self.title.trim().is_empty() {
            return false;
        }

        match self.kind {
            ConnectionRestoreKind::LocalTerminal => self.local_terminal.is_some(),
            _ if self.kind.is_workspace() => self.workspace_id.is_some(),
            _ => self.connection_id.is_some(),
        }
    }

    pub fn into_tab_data(self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectionRestoreItem {
    pub snapshot_id: String,
    pub kind: ConnectionRestoreKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_connection_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_terminal: Option<LocalTerminalRestoreState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_terminal: Option<SshTerminalRestoreState>,
    pub title: String,
}

impl ConnectionRestoreItem {
    pub fn connection_type(&self) -> Option<ConnectionType> {
        self.kind.connection_type()
    }

    pub fn is_workspace(&self) -> bool {
        self.kind.is_workspace()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
        .or_else(|| {
            restore_legacy_local_terminal_payload(data).filter(ConnectionRestorePayload::is_valid)
        })
}

fn restore_legacy_local_terminal_payload(data: &Value) -> Option<ConnectionRestorePayload> {
    if data.get("kind").and_then(|value| value.as_str()) != Some("local_terminal") {
        return None;
    }

    Some(ConnectionRestorePayload {
        kind: ConnectionRestoreKind::LocalTerminal,
        connection_id: None,
        workspace_id: None,
        active_connection_id: None,
        local_terminal: Some(LocalTerminalRestoreState {
            working_dir: data
                .get("working_dir")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            buffer_content: data
                .get("buffer_content")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            pty_session_id: None,
            prefer_live_restore: None,
            font_size: data
                .get("font_size")
                .and_then(|value| value.as_f64())
                .map(|value| value as f32),
            font_family: data
                .get("font_family")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            font_ligatures: data.get("font_ligatures").and_then(|value| value.as_bool()),
            line_height_scale: data
                .get("line_height_scale")
                .and_then(|value| value.as_f64())
                .map(|value| value as f32),
            cursor_blink: data.get("cursor_blink").and_then(|value| value.as_bool()),
            auto_copy: data.get("auto_copy").and_then(|value| value.as_bool()),
            middle_click_paste: data
                .get("middle_click_paste")
                .and_then(|value| value.as_bool()),
            confirm_multiline_paste: data
                .get("confirm_multiline_paste")
                .and_then(|value| value.as_bool()),
            confirm_high_risk_command: data
                .get("confirm_high_risk_command")
                .and_then(|value| value.as_bool()),
            exit_behavior: data
                .get("exit_behavior")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            theme_name: data
                .get("theme_name")
                .and_then(|value| value.as_str())
                .map(str::to_string),
        }),
        ssh_terminal: None,
        title: data
            .get("title")
            .and_then(|value| value.as_str())
            .filter(|title| !title.trim().is_empty())
            .unwrap_or("Local Terminal")
            .to_string(),
    })
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
                local_terminal: payload.local_terminal,
                ssh_terminal: payload.ssh_terminal,
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
            local_terminal: None,
            ssh_terminal: Some(SshTerminalRestoreState {
                working_dir: Some("/srv/demo".to_string()),
                buffer_content: Some("pwd\r\n/srv/demo".to_string()),
                theme_name: None,
            }),
            title: "服务器".to_string(),
        };
        let invalid_workspace = ConnectionRestorePayload {
            kind: ConnectionRestoreKind::DatabaseWorkspace,
            connection_id: None,
            workspace_id: None,
            active_connection_id: None,
            local_terminal: None,
            ssh_terminal: None,
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
                    id: "local-terminal-1".into(),
                    from: "terminal".into(),
                    key: "Terminal".into(),
                    data: serde_json::json!({
                        "kind": "local_terminal",
                        "working_dir": "/tmp/demo",
                        "title": "本地终端",
                        "font_size": 14.0,
                        "font_family": "JetBrains Mono",
                    }),
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
        assert_eq!(snapshot.items.len(), 2);
        assert_eq!(snapshot.items[0].snapshot_id, "ssh-terminal-42-1");
        assert_eq!(snapshot.items[0].connection_id, Some(42));
        assert_eq!(
            snapshot.items[0]
                .ssh_terminal
                .as_ref()
                .and_then(|state| state.working_dir.as_deref()),
            Some("/srv/demo")
        );
        assert_eq!(snapshot.items[1].snapshot_id, "local-terminal-1");
        assert_eq!(snapshot.items[1].kind, ConnectionRestoreKind::LocalTerminal);
        assert_eq!(
            snapshot.items[1]
                .local_terminal
                .as_ref()
                .and_then(|state| state.working_dir.as_deref()),
            Some("/tmp/demo")
        );
    }

    #[test]
    fn 工作区恢复项至少需要工作区_id() {
        let payload = ConnectionRestorePayload {
            kind: ConnectionRestoreKind::RedisWorkspace,
            connection_id: None,
            workspace_id: None,
            active_connection_id: Some(7),
            local_terminal: None,
            ssh_terminal: None,
            title: "Redis 工作区".to_string(),
        };

        assert!(!payload.is_valid());
    }

    #[test]
    fn 本地终端恢复项仅需本地终端状态() {
        let payload = ConnectionRestorePayload {
            kind: ConnectionRestoreKind::LocalTerminal,
            connection_id: None,
            workspace_id: None,
            active_connection_id: None,
            local_terminal: Some(LocalTerminalRestoreState {
                working_dir: Some("/tmp".to_string()),
                buffer_content: None,
                pty_session_id: None,
                prefer_live_restore: None,
                font_size: None,
                font_family: None,
                font_ligatures: None,
                line_height_scale: None,
                cursor_blink: None,
                auto_copy: None,
                middle_click_paste: None,
                confirm_multiline_paste: None,
                confirm_high_risk_command: None,
                exit_behavior: None,
                theme_name: None,
            }),
            ssh_terminal: None,
            title: "本地终端".to_string(),
        };

        assert!(payload.is_valid());
    }

    #[test]
    fn 兼容旧版本地终端_tab_state_数据() {
        let payload = restore_payload_from_tab_data(&serde_json::json!({
            "kind": "local_terminal",
            "working_dir": "/tmp/legacy",
            "title": "Legacy Terminal",
            "font_size": 13.0,
        }))
        .expect("旧版本地终端数据应能转换为恢复载荷");

        assert_eq!(payload.kind, ConnectionRestoreKind::LocalTerminal);
        assert_eq!(payload.title, "Legacy Terminal");
        assert_eq!(
            payload
                .local_terminal
                .as_ref()
                .and_then(|state| state.working_dir.as_deref()),
            Some("/tmp/legacy")
        );
    }

    #[test]
    fn 本地终端快照支持_pty_session_id_往返() {
        let path = temp_path("pty-session-id");
        if path.exists() {
            std::fs::remove_file(&path).expect("测试前清理临时文件失败");
        }

        let snapshot = ConnectionRestoreSnapshot {
            version: 1,
            saved_at: 456,
            items: vec![ConnectionRestoreItem {
                snapshot_id: "local-hosted-1".to_string(),
                kind: ConnectionRestoreKind::LocalTerminal,
                connection_id: None,
                workspace_id: None,
                active_connection_id: None,
                local_terminal: Some(LocalTerminalRestoreState {
                    working_dir: Some("/tmp".to_string()),
                    buffer_content: Some("echo hello".to_string()),
                    pty_session_id: Some("local-pty-uuid-123".to_string()),
                    prefer_live_restore: Some(true),
                    font_size: None,
                    font_family: None,
                    font_ligatures: None,
                    line_height_scale: None,
                    cursor_blink: None,
                    auto_copy: None,
                    middle_click_paste: None,
                    confirm_multiline_paste: None,
                    confirm_high_risk_command: None,
                    exit_behavior: None,
                    theme_name: None,
                }),
                ssh_terminal: None,
                title: "Hosted Terminal".to_string(),
            }],
        };

        save_connection_restore_snapshot_to_path(&snapshot, &path).expect("写入快照失败");
        let loaded = load_connection_restore_snapshot_from_path(&path).expect("读取快照失败");
        let local = loaded.items[0].local_terminal.as_ref().unwrap();
        assert_eq!(local.pty_session_id.as_deref(), Some("local-pty-uuid-123"));
        assert_eq!(local.prefer_live_restore, Some(true));

        std::fs::remove_file(&path).expect("测试后清理临时文件失败");
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
            items: vec![
                ConnectionRestoreItem {
                    snapshot_id: "sftp-1-2".to_string(),
                    kind: ConnectionRestoreKind::Sftp,
                    connection_id: Some(1),
                    workspace_id: None,
                    active_connection_id: None,
                    local_terminal: None,
                    ssh_terminal: None,
                    title: "SFTP".to_string(),
                },
                ConnectionRestoreItem {
                    snapshot_id: "ssh-terminal-1".to_string(),
                    kind: ConnectionRestoreKind::SshTerminal,
                    connection_id: Some(9),
                    workspace_id: None,
                    active_connection_id: None,
                    local_terminal: None,
                    ssh_terminal: Some(SshTerminalRestoreState {
                        working_dir: Some("/srv/app".to_string()),
                        buffer_content: Some("ls\r\napp.log".to_string()),
                        theme_name: None,
                    }),
                    title: "SSH".to_string(),
                },
                ConnectionRestoreItem {
                    snapshot_id: "local-terminal-1".to_string(),
                    kind: ConnectionRestoreKind::LocalTerminal,
                    connection_id: None,
                    workspace_id: None,
                    active_connection_id: None,
                    local_terminal: Some(LocalTerminalRestoreState {
                        working_dir: Some("/tmp/restore".to_string()),
                        buffer_content: Some("ls\r\nREADME.md".to_string()),
                        pty_session_id: None,
                        prefer_live_restore: None,
                        font_size: Some(15.0),
                        font_family: Some("JetBrains Mono".to_string()),
                        font_ligatures: Some(true),
                        line_height_scale: Some(1.3),
                        cursor_blink: Some(true),
                        auto_copy: Some(true),
                        middle_click_paste: Some(true),
                        confirm_multiline_paste: Some(true),
                        confirm_high_risk_command: Some(true),
                        exit_behavior: Some("prompt".to_string()),
                        theme_name: Some("OneDark".to_string()),
                    }),
                    ssh_terminal: None,
                    title: "本地终端".to_string(),
                },
            ],
        };

        save_connection_restore_snapshot_to_path(&snapshot, &path).expect("写入快照失败");
        let loaded = load_connection_restore_snapshot_from_path(&path).expect("读取快照失败");
        assert_eq!(loaded, snapshot);

        std::fs::remove_file(&path).expect("测试后清理临时文件失败");
    }
}
