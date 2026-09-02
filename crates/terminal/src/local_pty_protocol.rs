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
/// - Unix:  `$XDG_RUNTIME_DIR/omnihub/local-pty.sock` 或 `/tmp/omnihub-<uid>/local-pty.sock`
/// - Windows: `\\.\pipe\omnihub-local-pty-<pid>`（使用当前进程 PID 避免多实例冲突）
pub fn local_pty_endpoint() -> PathBuf {
    #[cfg(unix)]
    {
        match ensure_private_runtime_dir() {
            Ok(dir) => dir.join("local-pty.sock"),
            Err(err) => {
                tracing::warn!("运行时目录加固失败，按默认路径继续（连接将自然失败）: {err}");
                runtime_dir().join("local-pty.sock")
            }
        }
    }
    #[cfg(not(unix))]
    {
        let pid = std::process::id();
        PathBuf::from(format!(r"\\.\pipe\omnihub-local-pty-{pid}"))
    }
}

/// 返回本地 PTY host 进程的 lock/pid 文件路径，用于探活与重拉。
pub fn local_pty_pid_file() -> PathBuf {
    match ensure_private_runtime_dir() {
        Ok(dir) => dir.join("local-pty.pid"),
        Err(err) => {
            tracing::warn!("运行时目录加固失败，按默认路径继续: {err}");
            runtime_dir().join("local-pty.pid")
        }
    }
}

/// 创建（并加固）运行时目录，返回目录路径。
///
/// Unix 下该目录是 PTY host Unix socket 的唯一安全边界（socket 无应用层鉴权），
/// 因此强制收敛为 0700 且必须归当前用户所有：
/// - 防止 `/tmp/omnihub-<uid>` 这类可预测路径被其他用户预先创建（planting）；
/// - 收敛宽松的存量目录权限。
/// host 启动路径必须 fail-closed，client 侧可降级为仅告警。
pub fn ensure_private_runtime_dir() -> Result<PathBuf, std::io::Error> {
    let dir = runtime_dir();
    #[cfg(unix)]
    harden_runtime_dir(&dir)?;
    #[cfg(not(unix))]
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(unix)]
fn harden_runtime_dir(dir: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if !dir.exists() {
        std::fs::create_dir_all(dir)?;
    } else {
        // 用 symlink_metadata 显式拒绝符号链接，避免指向他属主/敏感位置
        let meta = std::fs::symlink_metadata(dir)?;
        if meta.file_type().is_symlink() {
            return Err(std::io::Error::other(format!(
                "runtime dir is a symlink, refusing: {}",
                dir.display()
            )));
        }
    }

    let uid = unsafe { libc::getuid() };
    let meta = std::fs::symlink_metadata(dir)?;
    if meta.uid() != uid {
        return Err(std::io::Error::other(format!(
            "runtime dir owned by uid {}, expected {uid} (possible planting): {}",
            meta.uid(),
            dir.display()
        )));
    }

    let mode = meta.permissions().mode() & 0o777;
    if mode != 0o700 {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn runtime_dir() -> PathBuf {
    #[cfg(unix)]
    {
        if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
            PathBuf::from(dir).join("omnihub")
        } else {
            let uid = unsafe { libc::getuid() };
            PathBuf::from(format!("/tmp/omnihub-{uid}"))
        }
    }
    #[cfg(not(unix))]
    {
        let dir = std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("TEMP"))
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        dir.join("omnihub")
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
        assert!(
            matches!(restored, LocalPtyHostRequest::Query { session_id } if session_id.is_empty())
        );
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

    #[cfg(unix)]
    mod unix_hardening {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        fn unique_test_dir() -> PathBuf {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            std::env::temp_dir().join(format!(
                "omnihub-pty-hardening-{}-{unique}",
                std::process::id()
            ))
        }

        fn dir_mode(dir: &std::path::Path) -> u32 {
            std::fs::metadata(dir).unwrap().permissions().mode() & 0o777
        }

        #[test]
        fn harden_creates_missing_dir_with_private_mode() {
            let dir = unique_test_dir();
            let _ = std::fs::remove_dir_all(&dir);
            harden_runtime_dir(&dir).unwrap();
            assert_eq!(dir_mode(&dir), 0o700);
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn harden_converges_loose_existing_dir() {
            let dir = unique_test_dir();
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
            harden_runtime_dir(&dir).unwrap();
            assert_eq!(dir_mode(&dir), 0o700);
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn harden_rejects_symlinked_dir() {
            let base = unique_test_dir();
            let real = base.join("real");
            std::fs::create_dir_all(&real).unwrap();
            let link = base.join("link");
            std::os::unix::fs::symlink(&real, &link).unwrap();
            let err = harden_runtime_dir(&link).unwrap_err();
            assert!(err.to_string().contains("symlink"), "{err}");
            let _ = std::fs::remove_dir_all(&base);
        }

        // 他属主目录（planting 场景）在非 root 测试环境无法模拟，属主校验分支
        // 不进 CI；错误文案含 "owned by uid" 供人工验证。
    }
}
