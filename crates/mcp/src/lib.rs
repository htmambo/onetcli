//! MCP (Model Context Protocol) crate for onetcli.
//!
//! Provides JSON-RPC 2.0 protocol implementation, STDIO transport,
//! and MCP server infrastructure.

pub mod errors;
pub mod protocol;
pub mod server;
pub mod transport;
pub mod types;

// Re-export commonly used types.
pub use errors::{McpError, McpErrorCode};
pub use protocol::{JsonRpcMessage, JsonRpcRequest, JsonRpcResponse, JsonRpcResponseResult};
pub use server::McpServer;
pub use transport::Transport;
pub use types::{
    CallToolResult, GetPromptResult, Implementation, InitializeResult, ListPromptsResult,
    ListResourcesResult, ListToolsResult, ReadResourceResult, Resource, ResourceTemplate,
    ServerCapabilities,
};
