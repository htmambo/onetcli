//! MCP server implementation skeleton.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::errors::McpError;
use crate::protocol::{
    JsonRpcError, JsonRpcMessage, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse,
    JsonRpcResponseResult, RequestId,
};
use crate::transport::Transport;
use crate::types::{
    CallToolParams, CallToolResult, GetPromptParams, GetPromptResult, InitializeParams,
    InitializeResult, ListPromptsResult, ListResourceTemplatesResult, ListResourcesResult,
    ListToolsResult, LoggingMessageNotification, ReadResourceParams, ReadResourceResult,
    RootsListChangedNotification, ServerCapabilities,
};

// ---------------------------------------------------------------------------
// Protocol version
// ---------------------------------------------------------------------------

/// Latest stable MCP protocol version.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

// ---------------------------------------------------------------------------
// Server handler trait
// ---------------------------------------------------------------------------

/// MCP server handler trait.
///
/// Each method corresponds to an MCP protocol request.
/// Implement this trait to provide custom tool/resource/prompt logic.
#[async_trait]
pub trait McpHandler: Send + Sync {
    /// Called during the initialize handshake.
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult, McpError> {
        let _ = params;
        Ok(InitializeResult {
            protocol_version: PROTOCOL_VERSION.to_string(),
            capabilities: ServerCapabilities::default(),
            server_info: crate::types::Implementation {
                name: "onetcli-mcp".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        })
    }

    /// List available tools.
    async fn list_tools(&self) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult {
            tools: vec![],
            next_cursor: None,
        })
    }

    /// Call a named tool with arguments.
    async fn call_tool(&self, params: CallToolParams) -> Result<CallToolResult, McpError> {
        Err(McpError::method_not_found(&format!("tool:{}", params.name)))
    }

    /// List registered resources.
    async fn list_resources(&self) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![],
            next_cursor: None,
        })
    }

    /// List resource templates.
    async fn list_resource_templates(&self) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult {
            resource_templates: vec![],
            next_cursor: None,
        })
    }

    /// Read a resource by URI.
    async fn read_resource(
        &self,
        params: ReadResourceParams,
    ) -> Result<ReadResourceResult, McpError> {
        Err(McpError::method_not_found(&format!(
            "resource:{}",
            params.uri
        )))
    }

    /// List available prompts.
    async fn list_prompts(&self) -> Result<ListPromptsResult, McpError> {
        Ok(ListPromptsResult {
            prompts: vec![],
            next_cursor: None,
        })
    }

    /// Get a prompt by name and arguments.
    async fn get_prompt(&self, params: GetPromptParams) -> Result<GetPromptResult, McpError> {
        Err(McpError::method_not_found(&format!(
            "prompt:{}",
            params.name
        )))
    }

    /// Handle an incoming logging message notification.
    async fn handle_logging_message(&self, _notification: LoggingMessageNotification) {}

    /// Handle roots list changed notification.
    async fn handle_roots_changed(&self, _notification: RootsListChangedNotification) {}
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

/// MCP server that processes JSON-RPC requests over a Transport.
pub struct McpServer<T: Transport> {
    transport: Arc<T>,
    handler: Arc<dyn McpHandler>,
    protocol_version: RwLock<String>,
}

impl<T: Transport> McpServer<T> {
    pub fn new(transport: T, handler: Arc<dyn McpHandler>) -> Self {
        Self {
            transport: Arc::new(transport),
            handler,
            protocol_version: RwLock::new(String::new()),
        }
    }

    /// Run the server: read messages, dispatch, write responses.
    /// Returns `Ok(())` when the transport is closed, `Err` on fatal errors.
    pub async fn run(&self) -> Result<(), McpError> {
        loop {
            let raw = match self.transport.read().await {
                Ok(b) => b,
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
                Err(e) => return Err(McpError::server_error(e.to_string())),
            };

            let msg: JsonRpcMessage = match serde_json::from_slice(&raw) {
                Ok(m) => m,
                Err(e) => {
                    // Parse error: respond with error, then continue.
                    let resp = JsonRpcResponse::error(
                        RequestId::Number(0),
                        JsonRpcError::new(-32700, e.to_string()),
                    );
                    let _ = self.write_response(resp).await;
                    continue;
                }
            };

            match msg {
                JsonRpcMessage::Request(req) => {
                    let resp = self.dispatch_request(req).await;
                    // Notifications have no id and must not be responded to.
                    if resp.id().is_some() {
                        let _ = self.write_response(resp).await;
                    }
                }
                JsonRpcMessage::Notification(notif) => {
                    self.dispatch_notification(notif).await;
                }
                JsonRpcMessage::Response(_) => {
                    // Servers SHOULD NOT respond to responses.
                }
            }
        }
    }

    /// Dispatch a request and return the response to send.
    async fn dispatch_request(&self, req: JsonRpcRequest) -> JsonRpcResponse {
        let id = req.id.clone().unwrap_or(RequestId::Number(0));

        let result = match req.method.as_str() {
            "initialize" => {
                let params: Result<InitializeParams, _> =
                    serde_json::from_value(req.params.unwrap_or_default());
                match params {
                    Ok(p) => match self.handler.initialize(p).await {
                        Ok(r) => {
                            *self.protocol_version.write().await = r.protocol_version.clone();
                            JsonRpcResponseResult::Success(serde_json::to_value(r).unwrap())
                        }
                        Err(e) => JsonRpcResponseResult::Error(JsonRpcError::new(
                            e.code.code(),
                            e.message.clone(),
                        )),
                    },
                    Err(e) => {
                        JsonRpcResponseResult::Error(JsonRpcError::new(-32602, e.to_string()))
                    }
                }
            }
            "tools/list" => match self.handler.list_tools().await {
                Ok(r) => JsonRpcResponseResult::Success(serde_json::to_value(r).unwrap()),
                Err(e) => JsonRpcResponseResult::Error(JsonRpcError::new(
                    e.code.code(),
                    e.message.clone(),
                )),
            },
            "tools/call" => {
                let params: Result<CallToolParams, _> =
                    serde_json::from_value(req.params.unwrap_or_default());
                match params {
                    Ok(p) => match self.handler.call_tool(p).await {
                        Ok(r) => JsonRpcResponseResult::Success(serde_json::to_value(r).unwrap()),
                        Err(e) => JsonRpcResponseResult::Error(JsonRpcError::new(
                            e.code.code(),
                            e.message.clone(),
                        )),
                    },
                    Err(e) => {
                        JsonRpcResponseResult::Error(JsonRpcError::new(-32602, e.to_string()))
                    }
                }
            }
            "resources/list" => match self.handler.list_resources().await {
                Ok(r) => JsonRpcResponseResult::Success(serde_json::to_value(r).unwrap()),
                Err(e) => JsonRpcResponseResult::Error(JsonRpcError::new(
                    e.code.code(),
                    e.message.clone(),
                )),
            },
            "resources/templates/list" => match self.handler.list_resource_templates().await {
                Ok(r) => JsonRpcResponseResult::Success(serde_json::to_value(r).unwrap()),
                Err(e) => JsonRpcResponseResult::Error(JsonRpcError::new(
                    e.code.code(),
                    e.message.clone(),
                )),
            },
            "resources/read" => {
                let params: Result<ReadResourceParams, _> =
                    serde_json::from_value(req.params.unwrap_or_default());
                match params {
                    Ok(p) => match self.handler.read_resource(p).await {
                        Ok(r) => JsonRpcResponseResult::Success(serde_json::to_value(r).unwrap()),
                        Err(e) => JsonRpcResponseResult::Error(JsonRpcError::new(
                            e.code.code(),
                            e.message.clone(),
                        )),
                    },
                    Err(e) => {
                        JsonRpcResponseResult::Error(JsonRpcError::new(-32602, e.to_string()))
                    }
                }
            }
            "prompts/list" => match self.handler.list_prompts().await {
                Ok(r) => JsonRpcResponseResult::Success(serde_json::to_value(r).unwrap()),
                Err(e) => JsonRpcResponseResult::Error(JsonRpcError::new(
                    e.code.code(),
                    e.message.clone(),
                )),
            },
            "prompts/get" => {
                let params: Result<GetPromptParams, _> =
                    serde_json::from_value(req.params.unwrap_or_default());
                match params {
                    Ok(p) => match self.handler.get_prompt(p).await {
                        Ok(r) => JsonRpcResponseResult::Success(serde_json::to_value(r).unwrap()),
                        Err(e) => JsonRpcResponseResult::Error(JsonRpcError::new(
                            e.code.code(),
                            e.message.clone(),
                        )),
                    },
                    Err(e) => {
                        JsonRpcResponseResult::Error(JsonRpcError::new(-32602, e.to_string()))
                    }
                }
            }
            other => JsonRpcResponseResult::Error(JsonRpcError::new(
                -32601,
                format!("method not found: {other}"),
            )),
        };

        match result {
            JsonRpcResponseResult::Success(v) => JsonRpcResponse::success(id, v),
            JsonRpcResponseResult::Error(e) => JsonRpcResponse::error(id, e),
        }
    }

    /// Dispatch a notification (no response expected).
    async fn dispatch_notification(&self, notif: JsonRpcNotification) {
        match notif.method.as_str() {
            "notifications/cancelled" => {}
            "notifications/message" => {
                if let Ok(msg) = serde_json::from_value::<LoggingMessageNotification>(
                    notif.params.unwrap_or_default(),
                ) {
                    self.handler.handle_logging_message(msg).await;
                }
            }
            "notifications/roots_changed" => {
                self.handler
                    .handle_roots_changed(RootsListChangedNotification {})
                    .await;
            }
            _ => {}
        }
    }

    async fn write_response(&self, resp: JsonRpcResponse) -> Result<(), McpError> {
        let bytes = serde_json::to_vec(&resp)?;
        self.transport.write(&bytes).await?;
        Ok(())
    }
}
