//! STDIO transport implementation using JSON lines over stdin/stdout.

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use std::io;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

use super::Transport;

/// STDIO transport.
///
/// Messages are delimited by newlines (JSON lines protocol).
/// Stdin is used for reading, stdout for writing.
/// Multiple writes are serialized via a mutex since stdout is not thread-safe.
pub struct StdioTransport {
    reader: Mutex<tokio::io::BufReader<tokio::io::Stdin>>,
    writer: Mutex<tokio::io::Stdout>,
}

impl StdioTransport {
    pub fn new() -> Self {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        Self {
            reader: Mutex::new(tokio::io::BufReader::new(stdin)),
            writer: Mutex::new(stdout),
        }
    }
}

impl Default for StdioTransport {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Transport for StdioTransport {
    async fn read(&self) -> io::Result<Bytes> {
        let mut guard = self.reader.lock().await;
        let mut line = String::new();
        guard.read_line(&mut line).await?;
        let line = line.trim();
        if line.is_empty() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "empty line"));
        }
        let mut buf = BytesMut::with_capacity(line.len());
        buf.extend_from_slice(line.as_bytes());
        Ok(buf.freeze())
    }

    async fn write(&self, msg: &[u8]) -> io::Result<()> {
        let mut guard = self.writer.lock().await;
        guard.write_all(msg).await?;
        guard.write_all(b"\n").await?;
        guard.flush().await
    }

    async fn close(&self) -> io::Result<()> {
        let mut guard = self.writer.lock().await;
        guard.flush().await
    }
}
