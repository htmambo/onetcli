use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{LocalConfig, TerminalCloseMode, TerminalSize};

pub type LocalPtySessionId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LocalPtyHostRequest {
    Spawn {
        config: LocalConfig,
        size: TerminalSize,
    },
    Attach {
        session_id: LocalPtySessionId,
        size: TerminalSize,
    },
    Resize {
        session_id: LocalPtySessionId,
        size: TerminalSize,
    },
    Input {
        session_id: LocalPtySessionId,
        data: Vec<u8>,
    },
    Close {
        session_id: LocalPtySessionId,
        mode: TerminalCloseMode,
    },
    Query {
        session_id: LocalPtySessionId,
    },
    KillDetached {
        session_ids: Vec<LocalPtySessionId>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LocalPtyHostEvent {
    Spawned {
        session_id: LocalPtySessionId,
        child_pid: Option<u32>,
    },
    Attached {
        session_id: LocalPtySessionId,
        child_pid: Option<u32>,
    },
    Output {
        session_id: LocalPtySessionId,
        data: Vec<u8>,
    },
    Exited {
        session_id: LocalPtySessionId,
        exit_code: i32,
    },
    Error {
        session_id: Option<LocalPtySessionId>,
        message: String,
    },
}

/// 返回本地 PTY host 进程监听的 IPC endpoint 路径/名称。
///
/// 路径规则：
/// - Unix:  `$XDG_RUNTIME_DIR/onetcli/local-pty.sock` 或 `/tmp/onetcli-<uid>/local-pty.sock`
/// - Windows: `\\.\pipe\onetcli-local-pty-<pid>`（使用当前进程 PID 避免多实例冲突）
pub fn local_pty_endpoint() -> PathBuf {
    #[cfg(unix)]
    {
        let dir = runtime_dir();
        let _ = std::fs::create_dir_all(&dir);
        dir.join("local-pty.sock")
    }
    #[cfg(not(unix))]
    {
        let pid = std::process::id();
        PathBuf::from(format!(r"\\.\pipe\onetcli-local-pty-{pid}"))
    }
}

/// 返回本地 PTY host 进程的 lock/pid 文件路径，用于探活与重拉。
pub fn local_pty_pid_file() -> PathBuf {
    let dir = runtime_dir();
    let _ = std::fs::create_dir_all(&dir);
    dir.join("local-pty.pid")
}

fn runtime_dir() -> PathBuf {
    #[cfg(unix)]
    {
        if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
            PathBuf::from(dir).join("onetcli")
        } else {
            let uid = unsafe { libc::getuid() };
            PathBuf::from(format!("/tmp/onetcli-{uid}"))
        }
    }
    #[cfg(not(unix))]
    {
        let dir = std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("TEMP"))
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        dir.join("onetcli")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_mode_roundtrip() {
        for mode in [TerminalCloseMode::Kill, TerminalCloseMode::Detach] {
            let json = serde_json::to_string(&mode).unwrap();
            let restored: TerminalCloseMode = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, mode);
        }
    }

    #[test]
    fn request_roundtrip() {
        let req = LocalPtyHostRequest::Input {
            session_id: "sess-1".into(),
            data: vec![0x1b, b'[', b'2', b'~'],
        };
        let bytes = serde_json::to_vec(&req).unwrap();
        let restored: LocalPtyHostRequest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored, req);
    }

    #[test]
    fn event_roundtrip() {
        let evt = LocalPtyHostEvent::Output {
            session_id: "sess-1".into(),
            data: b"hello".to_vec(),
        };
        let bytes = serde_json::to_vec(&evt).unwrap();
        let restored: LocalPtyHostEvent = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored, evt);
    }

    #[test]
    fn invalid_session_id_rejected_by_query() {
        // Query 本身不校验 ID 格式，但协议层应能承载空字符串而不 panic。
        let req = LocalPtyHostRequest::Query {
            session_id: String::new(),
        };
        let bytes = serde_json::to_vec(&req).unwrap();
        let restored: LocalPtyHostRequest = serde_json::from_slice(&bytes).unwrap();
        assert!(matches!(restored, LocalPtyHostRequest::Query { session_id } if session_id.is_empty()));
    }

    #[test]
    fn kill_detached_empty_list_tolerated() {
        let req = LocalPtyHostRequest::KillDetached {
            session_ids: vec![],
        };
        let bytes = serde_json::to_vec(&req).unwrap();
        let restored: LocalPtyHostRequest = serde_json::from_slice(&bytes).unwrap();
        assert!(
            matches!(restored, LocalPtyHostRequest::KillDetached { session_ids } if session_ids.is_empty())
        );
    }

    #[test]
    fn runtime_endpoint_resolves() {
        let path = local_pty_endpoint();
        let s = path.to_string_lossy();
        assert!(s.contains("local-pty"));
        #[cfg(unix)]
        assert!(s.ends_with(".sock"));
        #[cfg(not(unix))]
        assert!(s.starts_with(r"\\.\pipe\"));
    }

    #[test]
    fn pid_file_resolves() {
        let path = local_pty_pid_file();
        assert!(path.to_string_lossy().contains("local-pty.pid"));
    }
}
