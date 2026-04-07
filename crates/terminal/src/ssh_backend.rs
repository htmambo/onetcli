use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::sync::oneshot;

use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

use ssh::{
    ChannelEvent, PtyConfig, RusshClient, SshChannel, SshClient, SshConnectConfig,
    SshConnectionStage,
};

use crate::pty_backend::{GpuiEventProxy, TerminalEvent};
use crate::{TerminalBackend, TerminalSize};

const SSH_PROMPT_READY_MARKER: &[u8] = b"\x1b]1337;OnetcliPromptReady=1\x07";

/// 从终端数据中提取当前工作目录
///
/// 支持多种协议:
/// - OSC 7: `\x1b]7;file://hostname/path\x07`
/// - OSC 1337: `\x1b]1337;CurrentDir=/path\x07`（可能无 \x07）
/// - OSC 2/1332: `\x1b]2;user@host:/path\x07`
/// - OSC 1: `\x1b]1;/path\x07`
fn extract_cwd(data: &[u8]) -> Option<String> {
    if let Ok(text) = std::str::from_utf8(data) {
        // OSC 1337: CurrentDir 属性（可能没有 \x07 终止符）
        if let Some(pos) = text.find("\x1b]1337;CurrentDir=") {
            let after = &text[pos + 17..];
            let end_pos = after
                .find('\x07')
                .or_else(|| after.find('\r'))
                .or_else(|| after.find('\n'))
                .unwrap_or(after.len());
            let path = after[..end_pos].trim();
            if !path.is_empty() {
                return Some(path.to_string());
            }
        }

        // OSC 7: file:// URI
        if let Some(pos) = text.find("\x1b]7;") {
            let after = &text[pos + 5..];
            if let Some(end) = after.find('\x07') {
                let uri = &after[..end];
                if let Some(rest) = uri.strip_prefix("file://") {
                    if let Some(slash_pos) = rest.find('/') {
                        let path = &rest[slash_pos..];
                        if !path.is_empty() && path.starts_with('/') {
                            return percent_decode(path);
                        }
                    }
                }
            }
        }

        // OSC 2/1332: 标题中带路径（user@host:/path 或 user@host:~ 格式）
        if let Some(pos) = text.find("\x1b]2;") {
            let after = &text[pos + 4..];
            let end_pos = after.find('\x07').unwrap_or(after.len());
            let title = &after[..end_pos];
            if let Some(at) = title.find('@') {
                if let Some(colon) = title[at..].find(':') {
                    let path_start = at + colon + 1;
                    if path_start < title.len() {
                        let path = &title[path_start..];
                        if path.starts_with('/') || path.starts_with('~') {
                            return Some(path.to_string());
                        }
                    }
                }
            }
        }

        // OSC 1: 简单路径
        if let Some(pos) = text.find("\x1b]1;") {
            let after = &text[pos + 4..];
            let end_pos = after.find('\x07').unwrap_or(after.len());
            let path = after[..end_pos].trim();
            if path.starts_with('/') || path.starts_with('~') {
                return Some(path.to_string());
            }
        }
    }
    None
}

fn contains_prompt_ready_marker(data: &[u8]) -> bool {
    data.windows(SSH_PROMPT_READY_MARKER.len())
        .any(|window| window == SSH_PROMPT_READY_MARKER)
}

/// 简单的 percent-decode 实现，将 %XX 编码转为实际字节
fn percent_decode(input: &str) -> Option<String> {
    let mut result = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = hex_digit(bytes[i + 1])?;
            let lo = hex_digit(bytes[i + 2])?;
            result.push(hi << 4 | lo);
            i += 3;
        } else {
            result.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(result).ok()
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

enum SshCommand {
    Write(Vec<u8>),
    Resize(TerminalSize),
    Shutdown,
}

pub struct SshBackend {
    command_tx: UnboundedSender<SshCommand>,
}

impl SshBackend {
    pub async fn connect(
        config: SshConnectConfig,
        pty_config: PtyConfig,
        term: Arc<FairMutex<Term<GpuiEventProxy>>>,
        event_proxy: GpuiEventProxy,
        event_tx: UnboundedSender<TerminalEvent>,
        notify_tx: UnboundedSender<()>,
        on_disconnect: Option<oneshot::Sender<()>>,
        init_commands: Option<String>,
    ) -> anyhow::Result<Self> {
        Self::connect_with_progress(
            config,
            pty_config,
            term,
            event_proxy,
            event_tx,
            notify_tx,
            on_disconnect,
            init_commands,
            |_| {},
        )
        .await
    }

    pub async fn connect_with_progress<F>(
        config: SshConnectConfig,
        pty_config: PtyConfig,
        term: Arc<FairMutex<Term<GpuiEventProxy>>>,
        event_proxy: GpuiEventProxy,
        event_tx: UnboundedSender<TerminalEvent>,
        notify_tx: UnboundedSender<()>,
        on_disconnect: Option<oneshot::Sender<()>>,
        init_commands: Option<String>,
        mut progress: F,
    ) -> anyhow::Result<Self>
    where
        F: FnMut(SshConnectionStage) + Send,
    {
        let mut client = RusshClient::connect_with_progress(config, |stage| {
            progress(stage);
        })
        .await?;
        progress(SshConnectionStage::OpeningSessionChannel);
        let mut channel = client.open_channel().await?;

        progress(SshConnectionStage::RequestingPty);
        channel.request_pty(&pty_config).await?;
        progress(SshConnectionStage::StartingShell);
        channel.request_shell().await?;

        // 有初始化命令时直接写入 shell
        if let Some(ref commands) = init_commands {
            progress(SshConnectionStage::RunningInitCommands);
            for line in commands.lines() {
                if !line.trim().is_empty() {
                    let mut data = line.as_bytes().to_vec();
                    data.push(b'\n');
                    channel.send_data(&data).await?;
                }
            }
        }

        let (command_tx, mut command_rx) = unbounded_channel::<SshCommand>();

        // 创建 PtyWrite 回写通道
        let (pty_write_tx, mut pty_write_rx) = unbounded_channel::<Vec<u8>>();
        event_proxy.set_ssh_write_back(pty_write_tx);

        tokio::spawn(async move {
            let mut shutdown = false;
            let mut processor: Processor<StdSyncHandler> = Processor::new();

            loop {
                tokio::select! {
                    biased;
                    Some(cmd) = command_rx.recv() => {
                        match cmd {
                            SshCommand::Write(data) => {
                                let send_result = tokio::time::timeout(
                                    Duration::from_secs(30),
                                    channel.send_data(&data)
                                ).await;
                                if send_result.is_err() || send_result.is_ok_and(|r| r.is_err()) {
                                    break;
                                }
                            }
                            SshCommand::Resize(size) => {
                                let _ = channel.resize_pty(size.cols as u32, size.rows as u32).await;
                            }
                            SshCommand::Shutdown => {
                                shutdown = true;
                                let _ = channel.close().await;
                                break;
                            }
                        }
                    }
                    Some(data) = pty_write_rx.recv() => {
                        // DA 响应等回写数据
                        let send_result = tokio::time::timeout(
                            Duration::from_secs(30),
                            channel.send_data(&data)
                        ).await;
                        if send_result.is_err() || send_result.is_ok_and(|r| r.is_err()) {
                            break;
                        }
                    }
                    event = channel.recv() => {
                        match event {
                            Some(ChannelEvent::Data(data)) => {
                                if let Some(path) = extract_cwd(&data) {
                                    let _ = event_tx.send(TerminalEvent::WorkingDirChanged(path));
                                }
                                if contains_prompt_ready_marker(&data) {
                                    let _ = event_tx.send(TerminalEvent::SshPromptReady);
                                }
                                processor.advance(&mut *term.lock(), &data);
                                let _ = notify_tx.send(());
                            }
                            Some(ChannelEvent::ExtendedData { data, .. }) => {
                                if let Some(path) = extract_cwd(&data) {
                                    let _ = event_tx.send(TerminalEvent::WorkingDirChanged(path));
                                }
                                if contains_prompt_ready_marker(&data) {
                                    let _ = event_tx.send(TerminalEvent::SshPromptReady);
                                }
                                processor.advance(&mut *term.lock(), &data);
                                let _ = notify_tx.send(());
                            }
                            Some(ChannelEvent::Eof) | Some(ChannelEvent::Close) | None => {
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
            if !shutdown {
                let _ = client.disconnect().await;
            }
            if let Some(tx) = on_disconnect {
                let _ = tx.send(());
            }
        });

        Ok(Self { command_tx })
    }
}

impl TerminalBackend for SshBackend {
    fn write(&self, data: Vec<u8>) {
        let _ = self.command_tx.send(SshCommand::Write(data));
    }

    fn resize(&self, size: TerminalSize) {
        tracing::info!(
            "SshBackend::resize: 发送 resize 命令到远程 PTY: {}x{}",
            size.cols,
            size.rows
        );
        let _ = self.command_tx.send(SshCommand::Resize(size));
    }

    fn shutdown(&self) {
        let _ = self.command_tx.send(SshCommand::Shutdown);
    }
}

#[cfg(test)]
mod tests {
    use super::contains_prompt_ready_marker;

    #[test]
    fn prompt_ready_marker_detection_matches_embedded_osc() {
        assert!(contains_prompt_ready_marker(
            b"hello\x1b]1337;OnetcliPromptReady=1\x07world"
        ));
        assert!(!contains_prompt_ready_marker(b"hello world"));
    }
}
