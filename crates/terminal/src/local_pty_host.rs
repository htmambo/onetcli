#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use tokio::sync::{RwLock, broadcast, mpsc};
use tracing;
use uuid::Uuid;

use crate::{
    TerminalCloseMode, TerminalSize,
    local_pty_protocol::{
        LocalPtyHostEvent, LocalPtyHostRequest, LocalPtySessionId, local_pty_pid_file,
    },
};

const DETACH_TTL: Duration = Duration::from_secs(600);
const HOST_FRAME_CAPACITY: usize = 64;

#[derive(Debug, Clone)]
struct SessionHandle {
    session_id: LocalPtySessionId,
    child_pid: Option<u32>,
    input_tx: mpsc::UnboundedSender<Vec<u8>>,
    resize_tx: mpsc::UnboundedSender<TerminalSize>,
    output_tx: broadcast::Sender<Vec<u8>>,
    exit_tx: broadcast::Sender<LocalPtyHostEvent>,
    attached: Arc<RwLock<bool>>,
    last_detached_at: Arc<RwLock<Option<Instant>>>,
    shutdown_tx: mpsc::UnboundedSender<()>,
}

pub(crate) struct SessionRegistry {
    sessions: RwLock<HashMap<LocalPtySessionId, SessionHandle>>,
}

impl SessionRegistry {
    fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    async fn get(&self, session_id: &str) -> Option<SessionHandle> {
        self.sessions.read().await.get(session_id).cloned()
    }

    async fn insert(&self, handle: SessionHandle) {
        self.sessions
            .write()
            .await
            .insert(handle.session_id.clone(), handle);
    }

    async fn remove(&self, session_id: &str) -> Option<SessionHandle> {
        self.sessions.write().await.remove(session_id)
    }

    async fn list_detached(&self) -> Vec<(LocalPtySessionId, Instant)> {
        // 先快照 handle（Arc clone，廉价），尽快释放 registry 读锁，
        // 避免持锁期间 await 各 handle 子锁阻塞 insert/remove。
        let handles: Vec<(LocalPtySessionId, SessionHandle)> = {
            let sessions = self.sessions.read().await;
            sessions
                .iter()
                .map(|(id, handle)| (id.clone(), handle.clone()))
                .collect()
        };

        let mut result = Vec::new();
        for (id, handle) in handles {
            let attached = *handle.attached.read().await;
            if !attached {
                if let Some(detached_at) = *handle.last_detached_at.read().await {
                    result.push((id, detached_at));
                }
            }
        }
        result
    }
}

pub async fn run_local_pty_host() -> Result<()> {
    tracing::info!("启动本地 PTY host 子进程");

    let registry = Arc::new(SessionRegistry::new());

    // TTL 清理任务
    let ttl_registry = registry.clone();
    let _ttl_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let now = Instant::now();
            let to_kill: Vec<String> = ttl_registry
                .list_detached()
                .await
                .into_iter()
                .filter(|(_, detached_at)| now.duration_since(*detached_at) > DETACH_TTL)
                .map(|(id, _)| id)
                .collect();
            for session_id in to_kill {
                tracing::info!(session_id = %session_id, "detached session TTL 到期，执行清理");
                if let Some(handle) = ttl_registry.remove(&session_id).await {
                    let _ = handle.shutdown_tx.send(());
                }
            }
        }
    });

    // 写入 pid 文件
    write_pid_file()?;

    // 启动传输层
    run_host_transport(registry).await
}

fn write_pid_file() -> Result<()> {
    let path = local_pty_pid_file();
    std::fs::write(&path, std::process::id().to_string()).context("写入 host pid 文件失败")?;
    Ok(())
}

async fn run_host_transport(registry: Arc<SessionRegistry>) -> Result<()> {
    #[cfg(unix)]
    {
        crate::local_pty_host_unix::run(registry).await
    }
    #[cfg(not(unix))]
    {
        crate::local_pty_host_windows::run(registry).await
    }
}

async fn handle_request(
    registry: &SessionRegistry,
    request: LocalPtyHostRequest,
) -> Option<LocalPtyHostEvent> {
    match request {
        LocalPtyHostRequest::Spawn { config, size } => {
            match spawn_session(registry, config, size).await {
                Ok(handle) => Some(LocalPtyHostEvent::Spawned {
                    session_id: handle.session_id,
                    child_pid: handle.child_pid,
                }),
                Err(e) => Some(LocalPtyHostEvent::Error {
                    session_id: None,
                    message: e.to_string(),
                }),
            }
        }
        LocalPtyHostRequest::Attach { session_id, size } => {
            match attach_session(registry, &session_id, size).await {
                Ok(handle) => Some(LocalPtyHostEvent::Attached {
                    session_id: handle.session_id,
                    child_pid: handle.child_pid,
                }),
                Err(e) => Some(LocalPtyHostEvent::Error {
                    session_id: Some(session_id),
                    message: e.to_string(),
                }),
            }
        }
        LocalPtyHostRequest::Resize { session_id, size } => {
            if let Some(handle) = registry.get(&session_id).await {
                let _ = handle.resize_tx.send(size);
            }
            None
        }
        LocalPtyHostRequest::Input { session_id, data } => {
            if let Some(handle) = registry.get(&session_id).await {
                let _ = handle.input_tx.send(data);
            }
            None
        }
        LocalPtyHostRequest::Close { session_id, mode } => {
            if let Some(handle) = registry.remove(&session_id).await {
                let _ = handle.shutdown_tx.send(());
                if mode == TerminalCloseMode::Kill {
                    // 异步等待一小段时间后强杀，给 shutdown 信号一些处理时间
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        let _ = handle.shutdown_tx.send(());
                    });
                }
            }
            None
        }
        LocalPtyHostRequest::Query { session_id } => {
            let exists = registry.get(&session_id).await.is_some();
            if exists {
                Some(LocalPtyHostEvent::Attached {
                    session_id,
                    child_pid: None,
                })
            } else {
                Some(LocalPtyHostEvent::Error {
                    session_id: Some(session_id),
                    message: "session not found".to_string(),
                })
            }
        }
        LocalPtyHostRequest::KillDetached { session_ids } => {
            for session_id in session_ids {
                if let Some(handle) = registry.remove(&session_id).await {
                    let _ = handle.shutdown_tx.send(());
                }
            }
            None
        }
    }
}

async fn spawn_session(
    registry: &SessionRegistry,
    config: crate::LocalConfig,
    size: TerminalSize,
) -> Result<SessionHandle> {
    let pty_system = native_pty_system();
    let pty_size = PtySize {
        rows: size.rows,
        cols: size.cols,
        pixel_width: size.pixel_width,
        pixel_height: size.pixel_height,
    };
    let pair = pty_system.openpty(pty_size).context("打开 PTY 失败")?;

    let shell = config.shell.clone().unwrap_or_else(|| default_shell());
    let mut cmd = CommandBuilder::new(&shell);
    cmd.args(&config.shell_args);
    if let Some(dir) = &config.working_dir {
        cmd.cwd(dir);
    }
    for (k, v) in &config.env {
        cmd.env(k, v);
    }

    let child = pair
        .slave
        .spawn_command(cmd)
        .context("在 PTY 中启动 shell 失败")?;
    let child_pid = child.process_id().map(|id| id as u32);

    let session_id = format!("local-pty-{}", Uuid::new_v4());
    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (resize_tx, mut resize_rx) = mpsc::unbounded_channel::<TerminalSize>();
    let (output_tx, _output_rx) = broadcast::channel::<Vec<u8>>(HOST_FRAME_CAPACITY);
    let (exit_tx, _exit_rx) = broadcast::channel::<LocalPtyHostEvent>(1);
    let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel::<()>();

    let _output_tx_for_task = output_tx.clone();
    let exit_tx_for_task = exit_tx.clone();
    let mut master_writer = pair
        .master
        .take_writer()
        .context("获取 PTY master writer 失败")?;
    let mut master_reader = pair
        .master
        .try_clone_reader()
        .context("获取 PTY master reader 失败")?;

    // 启动 session I/O 任务：写/resize/关闭在 async 任务中处理，读在 spawn_blocking 中处理
    let output_tx_for_read = output_tx.clone();
    let session_id_for_read = session_id.clone();
    let session_id_for_task = session_id.clone();
    tokio::spawn(async move {
        // 读任务：在 blocking 线程中持续读取 PTY master
        let read_task = tokio::task::spawn_blocking(move || {
            let mut buf = vec![0u8; 4096];
            loop {
                match master_reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if output_tx_for_read.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::debug!(session_id = %session_id_for_read, error = %e, "PTY master 读取结束");
                        break;
                    }
                }
            }
        });

        let mut read_task = std::pin::pin!(read_task);
        let session_id_inner = session_id_for_task.clone();
        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.recv() => {
                    tracing::debug!(session_id = %session_id_inner, "session I/O 任务收到关闭信号");
                    break;
                }
                Some(data) = input_rx.recv() => {
                    let _ = master_writer.write_all(&data);
                    let _ = master_writer.flush();
                }
                Some(size) = resize_rx.recv() => {
                    let pty_size = PtySize {
                        rows: size.rows,
                        cols: size.cols,
                        pixel_width: size.pixel_width,
                        pixel_height: size.pixel_height,
                    };
                    let _ = pair.master.resize(pty_size);
                }
                _ = &mut read_task => {
                    break;
                }
            }
        }

        let _ = exit_tx_for_task.send(LocalPtyHostEvent::Exited {
            session_id: session_id_for_task,
            exit_code: 0,
        });
    });

    let handle = SessionHandle {
        session_id: session_id.clone(),
        child_pid,
        input_tx,
        resize_tx,
        output_tx,
        exit_tx,
        attached: Arc::new(RwLock::new(true)),
        last_detached_at: Arc::new(RwLock::new(None)),
        shutdown_tx,
    };

    registry.insert(handle.clone()).await;
    Ok(handle)
}

async fn attach_session(
    registry: &SessionRegistry,
    session_id: &str,
    _size: TerminalSize,
) -> Result<SessionHandle> {
    let handle = registry
        .get(session_id)
        .await
        .context("session not found")?;
    {
        let mut attached = handle.attached.write().await;
        *attached = true;
    }
    {
        let mut last_detached = handle.last_detached_at.write().await;
        *last_detached = None;
    }
    Ok(handle)
}

fn default_shell() -> String {
    #[cfg(target_os = "windows")]
    {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}

// 供传输层调用：当 client 断开连接时标记 session 为 detached
pub(crate) async fn mark_session_detached(registry: &SessionRegistry, session_id: &str) {
    if let Some(handle) = registry.get(session_id).await {
        let mut attached = handle.attached.write().await;
        *attached = false;
        let mut last_detached = handle.last_detached_at.write().await;
        *last_detached = Some(Instant::now());
    }
}

// 供传输层调用：获取 session 的 broadcast receiver
pub(crate) async fn subscribe_output(
    registry: &SessionRegistry,
    session_id: &str,
) -> Option<broadcast::Receiver<Vec<u8>>> {
    registry
        .get(session_id)
        .await
        .map(|h| h.output_tx.subscribe())
}

// 供传输层调用：获取 session 的 exit 事件 receiver
pub(crate) async fn subscribe_exit(
    registry: &SessionRegistry,
    session_id: &str,
) -> Option<broadcast::Receiver<LocalPtyHostEvent>> {
    registry
        .get(session_id)
        .await
        .map(|h| h.exit_tx.subscribe())
}

// 供传输层调用：处理单个请求
pub(crate) async fn dispatch_request(
    registry: &SessionRegistry,
    request: LocalPtyHostRequest,
) -> Option<LocalPtyHostEvent> {
    handle_request(registry, request).await
}
