//! STDIO transport for MCP protocol.

use async_trait::async_trait;
use bytes::Bytes;
use std::io;

pub mod json_lines;
pub mod stdio;

/// Protocol transport trait.
///
/// MCP servers can be backed by different transports (stdio, HTTP+SSE, etc.).
/// This trait abstracts over the transport layer so the server logic is
/// transport-agnostic.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Read a single message from the transport.
    async fn read(&self) -> io::Result<Bytes>;

    /// Write a single message to the transport.
    async fn write(&self, msg: &[u8]) -> io::Result<()>;

    /// Close the transport.
    async fn close(&self) -> io::Result<()>;
}
