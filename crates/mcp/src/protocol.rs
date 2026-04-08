//! JSON-RPC 2.0 protocol types.

use serde::{Deserialize, Serialize};

/// JSON-RPC 2.0 version string.
pub const JSONRPC_VERSION: &str = "2.0";

// ---------------------------------------------------------------------------
// Core types (defined first so they can be referenced by later structs)
// ---------------------------------------------------------------------------

/// JSON-RPC error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), data: None }
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// JSON-RPC notification (request without id).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Request / Response
// ---------------------------------------------------------------------------

/// Request identifier: either a number or a string.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    String(String),
}

/// JSON-RPC request object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RequestId>,
}

/// JSON-RPC response object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RequestId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    pub fn success(id: RequestId, result: serde_json::Value) -> Self {
        Self { jsonrpc: JSONRPC_VERSION.to_string(), id: Some(id), result: Some(result), error: None }
    }

    pub fn error(id: RequestId, error: JsonRpcError) -> Self {
        Self { jsonrpc: JSONRPC_VERSION.to_string(), id: Some(id), result: None, error: Some(error) }
    }

    pub fn id(&self) -> Option<&RequestId> { self.id.as_ref() }
}

// ---------------------------------------------------------------------------
// Message envelope
// ---------------------------------------------------------------------------

/// JSON-RPC message: request/response union for parsing incoming stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    Request(JsonRpcRequest),
    Response(JsonRpcResponse),
    Notification(JsonRpcNotification),
}

/// Result type for JSON-RPC responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcResponseResult {
    Success(serde_json::Value),
    Error(JsonRpcError),
}

/// Parse a JSON-RPC 2.0 version string, returning the trimmed string if valid.
pub fn parse_version(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed == JSONRPC_VERSION { Some(trimmed.to_string()) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_deserialize() {
        let json = r#"{"jsonrpc":"2.0","method":"tools/list","id":1}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.method, "tools/list");
        assert_eq!(req.id, Some(RequestId::Number(1)));
    }

    #[test]
    fn test_response_success() {
        let resp = JsonRpcResponse::success(RequestId::Number(1), serde_json::json!({"tools": []}));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""result""#));
        assert!(!json.contains(r#""error""#));
    }

    #[test]
    fn test_response_error() {
        let resp = JsonRpcResponse::error(RequestId::Number(1), JsonRpcError::new(-32600, "Invalid Request"));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""error":"#));
    }
}
