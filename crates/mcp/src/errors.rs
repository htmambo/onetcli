//! MCP protocol error types.

use serde::{Deserialize, Serialize};
use std::fmt;

/// MCP error codes (JSON-RPC -32000 to -32099 reserved for server use).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpErrorCode {
    /// Invalid JSON was received.
    ParseError = -32700,
    /// The JSON sent is not a valid request object.
    InvalidRequest = -32600,
    /// The method does not exist or is not available.
    MethodNotFound = -32601,
    /// Invalid method parameter(s).
    InvalidParams = -32602,
    /// Internal JSON-RPC error.
    InternalError = -32603,
    /// Server error (fallback).
    ServerError = -32000,
    /// Connection closed.
    ConnectionClosed = -32001,
    /// Invalid protocol version.
    InvalidProtocolVersion = -32002,
    /// Capability not supported.
    CapabilityNotSupported = -32003,
}

impl fmt::Display for McpErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            McpErrorCode::ParseError => write!(f, "Parse error"),
            McpErrorCode::InvalidRequest => write!(f, "Invalid request"),
            McpErrorCode::MethodNotFound => write!(f, "Method not found"),
            McpErrorCode::InvalidParams => write!(f, "Invalid params"),
            McpErrorCode::InternalError => write!(f, "Internal error"),
            McpErrorCode::ServerError => write!(f, "Server error"),
            McpErrorCode::ConnectionClosed => write!(f, "Connection closed"),
            McpErrorCode::InvalidProtocolVersion => write!(f, "Invalid protocol version"),
            McpErrorCode::CapabilityNotSupported => write!(f, "Capability not supported"),
        }
    }
}

impl McpErrorCode {
    pub fn code(&self) -> i32 { *self as i32 }
}

/// MCP protocol error with code, message, and optional data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpError {
    pub code: McpErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl fmt::Display for McpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code.code(), self.message)
    }
}

impl std::error::Error for McpError {}

impl McpError {
    pub fn new(code: McpErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), data: None }
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn parse_error(msg: impl Into<String>) -> Self {
        Self::new(McpErrorCode::ParseError, msg)
    }

    pub fn invalid_request(msg: impl Into<String>) -> Self {
        Self::new(McpErrorCode::InvalidRequest, msg)
    }

    pub fn method_not_found(method: &str) -> Self {
        Self::new(McpErrorCode::MethodNotFound, format!("method not found: {method}"))
    }

    pub fn invalid_params(msg: impl Into<String>) -> Self {
        Self::new(McpErrorCode::InvalidParams, msg)
    }

    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self::new(McpErrorCode::InternalError, msg)
    }

    pub fn server_error(msg: impl Into<String>) -> Self {
        Self::new(McpErrorCode::ServerError, msg)
    }

    pub fn connection_closed() -> Self {
        Self::new(McpErrorCode::ConnectionClosed, "connection closed")
    }

    pub fn invalid_protocol_version(version: &str) -> Self {
        Self::new(
            McpErrorCode::InvalidProtocolVersion,
            format!("invalid protocol version: {version}"),
        )
    }

    pub fn capability_not_supported(capability: &str) -> Self {
        Self::new(
            McpErrorCode::CapabilityNotSupported,
            format!("capability not supported: {capability}"),
        )
    }
}

impl From<serde_json::Error> for McpError {
    fn from(e: serde_json::Error) -> Self {
        McpError::parse_error(e.to_string())
    }
}

impl From<std::io::Error> for McpError {
    fn from(e: std::io::Error) -> Self {
        McpError::server_error(e.to_string())
    }
}
