use std::io::{BufRead, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::sync::{mpsc, Mutex};

use crate::local_pty_protocol::{
    local_pty_endpoint, LocalPtyHostEvent, LocalPtyHostRequest, LocalPtySessionId,
};
use crate::{TerminalBackend, TerminalCloseMode, TerminalSize};

/// 本地 PTY host client：管理到 host 子进程的连接，并为 UI 侧暴露 `TerminalBackend`。
pub struct LocalPtyClient {
    request_tx: mpsc::UnboundedSender<LocalPtyHostRequest>,
    event_rx: mpsc::UnboundedReceiver<LocalPtyHostEvent>,
    _io_handle: std::thread::JoinHandle<()>,
}

impl LocalPtyClient {
    /// 确保 host 已运行并建立连接。
    pub fn connect() -> Result<Self> {
        ensure_host_running()?;
        let stream = connect_with_retry()?;
        let (event_tx, event_rx) = mpsc::unbounded_channel::<LocalPtyHostEvent>();
        let (request_tx, mut request_rx) = mpsc::unbounded_channel::<LocalPtyHostRequest>();

        let read_stream = stream.try_clone().context("克隆 UnixStream 失败")?;
        let write_stream = Arc::new(Mutex::new(stream));

        let io_handle = std::thread::spawn(move || {
            // 读线程
            let mut reader = std::io::BufReader::new(read_stream);
            let read_event_tx = event_tx.clone();
            std::thread::spawn(move || {
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            if let Ok(event) = serde_json::from_str::<LocalPtyHostEvent>(&line) {
                                let _ = read_event_tx.send(event);
                            }
                        }
                        Err(_) => break,
                    }
                }
            });

            // 写循环
            while let Some(req) = request_rx.blocking_recv() {
                let json = match serde_json::to_string(&req) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let mut guard = write_stream.blocking_lock();
                if guard.write_all(json.as_bytes()).is_err() {
                    break;
                }
                if guard.write_all(b"\n").is_err() {
                    break;
                }
                if guard.flush().is_err() {
                    break;
                }
            }
        });

        Ok(Self {
            request_tx,
            event_rx,
            _io_handle: io_handle,
        })
    }

    /// 消费下一个 host 事件（阻塞）。
    pub fn recv_event(&mut self, timeout: Duration) -> Option<LocalPtyHostEvent> {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if let Ok(event) = self.event_rx.try_recv() {
                return Some(event);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }

    /// 同步发起 Spawn 并等待响应，返回 session_id 和 child_pid。
    pub fn spawn_sync(
        &mut self,
        config: crate::LocalConfig,
        size: TerminalSize,
    ) -> Result<(LocalPtySessionId, Option<u32>)> {
        self.request_tx
            .send(LocalPtyHostRequest::Spawn { config, size })
            .map_err(|_| anyhow::anyhow!("host client 通道已关闭"))?;
        match self.recv_event(Duration::from_secs(5)) {
            Some(LocalPtyHostEvent::Spawned {
                session_id,
                child_pid,
            }) => Ok((session_id, child_pid)),
            Some(LocalPtyHostEvent::Error { message, .. }) => anyhow::bail!(message),
            _ => anyhow::bail!("等待 Spawned 响应超时"),
        }
    }

    /// 同步发起 Attach 并等待响应，返回 child_pid。
    pub fn attach_sync(
        &mut self,
        session_id: LocalPtySessionId,
        size: TerminalSize,
    ) -> Result<Option<u32>> {
        self.request_tx
            .send(LocalPtyHostRequest::Attach {
                session_id: session_id.clone(),
                size,
            })
            .map_err(|_| anyhow::anyhow!("host client 通道已关闭"))?;
        match self.recv_event(Duration::from_secs(5)) {
            Some(LocalPtyHostEvent::Attached { child_pid, .. }) => Ok(child_pid),
            Some(LocalPtyHostEvent::Error { message, .. }) => anyhow::bail!(message),
            _ => anyhow::bail!("等待 Attached 响应超时"),
        }
    }

    /// 发送单个请求（不消费 client）。
    pub fn send_request(&self, request: LocalPtyHostRequest) -> Result<()> {
        self.request_tx
            .send(request)
            .map_err(|_| anyhow::anyhow!("host client 通道已关闭"))
    }

    /// 将 client 拆分为请求发送端和事件接收端，用于后续异步处理。
    pub fn split(
        mut self,
    ) -> (
        mpsc::UnboundedSender<LocalPtyHostRequest>,
        mpsc::UnboundedReceiver<LocalPtyHostEvent>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();
        while let Ok(event) = self.event_rx.try_recv() {
            let _ = tx.send(event);
        }
        (self.request_tx, rx)
    }
}

/// 同步向 host 发送 KillDetached 请求，忽略所有错误（用于恢复跳过时的清理）。
pub fn kill_detached_sessions(session_ids: Vec<String>) {
    if session_ids.is_empty() {
        return;
    }
    let Ok(stream) = connect_with_retry() else {
        return;
    };
    let request = LocalPtyHostRequest::KillDetached { session_ids };
    let Ok(json) = serde_json::to_string(&request) else {
        return;
    };
    let mut stream = stream;
    let _ = stream.write_all(json.as_bytes());
    let _ = stream.write_all(b"\n");
    let _ = stream.flush();
}

fn ensure_host_running() -> Result<()> {
    let endpoint = local_pty_endpoint();
    if endpoint_exists_and_alive(&endpoint) {
        return Ok(());
    }

    if endpoint.exists() {
        let _ = std::fs::remove_file(&endpoint);
    }

    let current_exe = std::env::current_exe().context("获取当前可执行文件路径失败")?;
    let mut child = Command::new(&current_exe)
        .arg("--local-pty-host")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("启动 local-pty-host 失败")?;

    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        if endpoint.exists() && can_connect(&endpoint) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
        if let Ok(Some(_)) = child.try_wait() {
            anyhow::bail!("local-pty-host 进程过早退出");
        }
    }

    anyhow::bail!("local-pty-host 未在超时时间内就绪")
}

fn endpoint_exists_and_alive(path: &Path) -> bool {
    path.exists() && can_connect(path)
}

fn can_connect(path: &Path) -> bool {
    UnixStream::connect(path).is_ok()
}

fn connect_with_retry() -> Result<UnixStream> {
    let endpoint = local_pty_endpoint();
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        if let Ok(stream) = UnixStream::connect(&endpoint) {
            return Ok(stream);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    anyhow::bail!("无法连接到 local-pty-host endpoint")
}

/// 基于 host client 的 `TerminalBackend` 实现。
pub struct LocalPtyClientBackend {
    request_tx: mpsc::UnboundedSender<LocalPtyHostRequest>,
    session_id: LocalPtySessionId,
    child_pid: Option<u32>,
}

impl LocalPtyClientBackend {
    pub fn new(
        request_tx: mpsc::UnboundedSender<LocalPtyHostRequest>,
        session_id: LocalPtySessionId,
        child_pid: Option<u32>,
    ) -> Self {
        Self {
            request_tx,
            session_id,
            child_pid,
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn child_pid(&self) -> Option<u32> {
        self.child_pid
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn client_spawn_returns_session_id() {
        let _ = std::fs::remove_file(crate::local_pty_protocol::local_pty_endpoint());
        let host = tokio::spawn(async {
            let _ = crate::run_local_pty_host().await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let result = tokio::task::spawn_blocking(|| {
            let mut client = LocalPtyClient::connect()?;
            client.spawn_sync(
                crate::LocalConfig::default(),
                crate::TerminalSize::default(),
            )
        })
        .await
        .unwrap();

        let (session_id, _pid) = result.expect("spawn 应成功");
        assert!(session_id.starts_with("local-pty-"));

        host.abort();
        let _ = std::fs::remove_file(crate::local_pty_protocol::local_pty_endpoint());
    }

    #[tokio::test]
    async fn client_attach_missing_session_fails() {
        let _ = std::fs::remove_file(crate::local_pty_protocol::local_pty_endpoint());
        let host = tokio::spawn(async {
            let _ = crate::run_local_pty_host().await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let result = tokio::task::spawn_blocking(|| {
            let mut client = LocalPtyClient::connect()?;
            client.attach_sync("nonexistent".into(), crate::TerminalSize::default())
        })
        .await
        .unwrap();

        assert!(result.is_err());

        host.abort();
        let _ = std::fs::remove_file(crate::local_pty_protocol::local_pty_endpoint());
    }

    #[test]
    fn backend_close_sends_kill_request() {
        let (tx, mut rx) = mpsc::unbounded_channel::<LocalPtyHostRequest>();
        let backend = LocalPtyClientBackend::new(tx, "sess-1".into(), Some(42));
        backend.close(TerminalCloseMode::Kill);

        match rx.try_recv().unwrap() {
            LocalPtyHostRequest::Close { session_id, mode } => {
                assert_eq!(session_id, "sess-1");
                assert_eq!(mode, TerminalCloseMode::Kill);
            }
            other => panic!("expected Close(Kill), got {other:?}"),
        }
    }

    #[test]
    fn backend_close_sends_detach_request() {
        let (tx, mut rx) = mpsc::unbounded_channel::<LocalPtyHostRequest>();
        let backend = LocalPtyClientBackend::new(tx, "sess-2".into(), None);
        backend.close(TerminalCloseMode::Detach);

        match rx.try_recv().unwrap() {
            LocalPtyHostRequest::Close { session_id, mode } => {
                assert_eq!(session_id, "sess-2");
                assert_eq!(mode, TerminalCloseMode::Detach);
            }
            other => panic!("expected Close(Detach), got {other:?}"),
        }
    }

    #[test]
    fn backend_write_forwards_input() {
        let (tx, mut rx) = mpsc::unbounded_channel::<LocalPtyHostRequest>();
        let backend = LocalPtyClientBackend::new(tx, "sess-3".into(), Some(7));
        backend.write(vec![0x1b, b'[', b'2', b'~']);

        match rx.try_recv().unwrap() {
            LocalPtyHostRequest::Input { session_id, data } => {
                assert_eq!(session_id, "sess-3");
                assert_eq!(data, vec![0x1b, b'[', b'2', b'~']);
            }
            other => panic!("expected Input, got {other:?}"),
        }
    }
}

impl TerminalBackend for LocalPtyClientBackend {
    fn write(&self, data: Vec<u8>) {
        let _ = self.request_tx.send(LocalPtyHostRequest::Input {
            session_id: self.session_id.clone(),
            data,
        });
    }

    fn resize(&self, size: TerminalSize) {
        let _ = self.request_tx.send(LocalPtyHostRequest::Resize {
            session_id: self.session_id.clone(),
            size,
        });
    }

    fn close(&self, mode: TerminalCloseMode) {
        let _ = self.request_tx.send(LocalPtyHostRequest::Close {
            session_id: self.session_id.clone(),
            mode,
        });
    }
}
