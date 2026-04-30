use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;

use crate::local_pty_host::{
    dispatch_request, mark_session_detached, subscribe_exit, subscribe_output, SessionRegistry,
};
use crate::local_pty_protocol::{local_pty_endpoint, LocalPtyHostEvent, LocalPtyHostRequest};

pub(crate) async fn run(registry: Arc<SessionRegistry>) -> Result<()> {
    let endpoint = local_pty_endpoint();
    if endpoint.exists() {
        // 尝试连接现有 endpoint 以确认是否存活
        if UnixStream::connect(&endpoint).await.is_ok() {
            anyhow::bail!("另一个 local-pty-host 正在运行: {}", endpoint.display());
        }
        let _ = std::fs::remove_file(&endpoint);
    }

    let listener = UnixListener::bind(&endpoint)
        .with_context(|| format!("绑定 Unix Domain Socket 失败: {}", endpoint.display()))?;
    tracing::info!(endpoint = %endpoint.display(), "local-pty-host Unix listener 已启动");

    loop {
        let (stream, _) = listener.accept().await.context("接受 client 连接失败")?;
        let registry = registry.clone();
        tokio::spawn(handle_client(registry, stream));
    }
}

async fn handle_client(registry: Arc<SessionRegistry>, stream: UnixStream) {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    let mut active_session: Option<String> = None;
    let mut output_rx: Option<broadcast::Receiver<Vec<u8>>> = None;
    let mut exit_rx: Option<broadcast::Receiver<LocalPtyHostEvent>> = None;

    loop {
        tokio::select! {
            biased;
            line = lines.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        let request: LocalPtyHostRequest = match serde_json::from_str(&line) {
                            Ok(req) => req,
                            Err(e) => {
                                let _ = write_event(&mut writer, LocalPtyHostEvent::Error {
                                    session_id: None,
                                    message: format!("invalid request: {e}"),
                                }).await;
                                continue;
                            }
                        };

                        // 跟踪 session attachment
                        match &request {
                            LocalPtyHostRequest::Attach { session_id, .. } => {
                                active_session = Some(session_id.clone());
                                output_rx = subscribe_output(&registry, session_id).await;
                                exit_rx = subscribe_exit(&registry, session_id).await;
                            }
                            _ => {}
                        }

                        let response = dispatch_request(&registry, request).await;
                        if let Some(LocalPtyHostEvent::Spawned { session_id, .. }) =
                            response.as_ref()
                        {
                            active_session = Some(session_id.clone());
                            output_rx = subscribe_output(&registry, session_id).await;
                            exit_rx = subscribe_exit(&registry, session_id).await;
                        }

                        if let Some(event) = response {
                            if write_event(&mut writer, event).await.is_err() {
                                break;
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        tracing::debug!(error = %e, "client 连接读取错误");
                        break;
                    }
                }
            }
            Some(data) = async { output_rx.as_mut()?.recv().await.ok() } => {
                if let Some(ref session_id) = active_session {
                    let event = LocalPtyHostEvent::Output {
                        session_id: session_id.clone(),
                        data,
                    };
                    if write_event(&mut writer, event).await.is_err() {
                        break;
                    }
                }
            }
            Some(event) = async { exit_rx.as_mut()?.recv().await.ok() } => {
                if let Some(ref session_id) = active_session {
                    let event = match event {
                        LocalPtyHostEvent::Exited { exit_code, .. } => LocalPtyHostEvent::Exited {
                            session_id: session_id.clone(),
                            exit_code,
                        },
                        other => other,
                    };
                    if write_event(&mut writer, event).await.is_err() {
                        break;
                    }
                }
            }
            else => break,
        }
    }

    if let Some(session_id) = active_session {
        mark_session_detached(&registry, &session_id).await;
    }
}

async fn write_event(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    event: LocalPtyHostEvent,
) -> std::io::Result<()> {
    let json = serde_json::to_string(&event)?;
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await
}
