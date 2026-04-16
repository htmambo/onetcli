use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::broadcast;

use crate::local_pty_host::{dispatch_request, mark_session_detached, subscribe_output, SessionRegistry};
use crate::local_pty_protocol::{local_pty_endpoint, LocalPtyHostEvent, LocalPtyHostRequest};

pub(crate) async fn run(registry: Arc<SessionRegistry>) -> Result<()> {
    let endpoint = local_pty_endpoint();
    let pipe_name = endpoint
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("无效的 named pipe 路径"))?;

    let listener = ServerOptions::new()
        .create(pipe_name)
        .context("创建 Windows Named Pipe Server 失败")?;
    tracing::info!(endpoint = %pipe_name, "local-pty-host Windows listener 已启动");

    loop {
        listener.connect().await.context("接受 client 连接失败")?;
        // 为下一个连接创建新的 server
        let next_listener = ServerOptions::new()
            .create(pipe_name)
            .context("创建下一个 Windows Named Pipe Server 失败")?;
        let current_listener = std::mem::replace(&mut listener, next_listener);
        let registry = registry.clone();
        tokio::spawn(handle_client(registry, current_listener));
    }
}

async fn handle_client(registry: Arc<SessionRegistry>, server: NamedPipeServer) {
    let (reader, mut writer) = tokio::io::split(server);
    let mut lines = BufReader::new(reader).lines();
    let mut active_session: Option<String> = None;
    let mut output_rx: Option<broadcast::Receiver<Vec<u8>>> = None;

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

                        match &request {
                            LocalPtyHostRequest::Attach { session_id, .. } => {
                                active_session = Some(session_id.clone());
                                output_rx = subscribe_output(&registry, session_id).await;
                            }
                            LocalPtyHostRequest::Spawn { .. } => {
                                active_session = None;
                                output_rx = None;
                            }
                            _ => {}
                        }

                        if let Some(event) = dispatch_request(&registry, request).await {
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
            else => break,
        }
    }

    if let Some(session_id) = active_session {
        mark_session_detached(&registry, &session_id).await;
    }
}

async fn write_event(
    writer: &mut tokio::io::WriteHalf<NamedPipeServer>,
    event: LocalPtyHostEvent,
) -> std::io::Result<()> {
    let json = serde_json::to_string(&event)?;
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await
}
