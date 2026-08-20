use std::pin::Pin;

use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use llm_connector::LlmClient;
use llm_connector::types::{
    ChatRequest, ChatResponse, Message, MessageBlock, Role, StreamingResponse, Tool, ToolCall,
    ToolChoice,
};

use super::types::{ProviderConfig, ProviderType};

pub type ChatStream = Pin<Box<dyn Stream<Item = Result<StreamingResponse>> + Send>>;

const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
const ALIYUN_BASE_URL: &str = "https://dashscope.aliyuncs.com";
const ALIYUN_COMPATIBLE_BASE_URL: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";
const ZHIPU_BASE_URL: &str = "https://open.bigmodel.cn";
const OLLAMA_BASE_URL: &str = "http://localhost:11434";
const VOLCENGINE_BASE_URL: &str = "https://ark.cn-beijing.volces.com/api/v3";
const MOONSHOT_BASE_URL: &str = "https://api.moonshot.cn/v1";
const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com";
const GOOGLE_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

pub use llm_connector::types::{
    ChatRequest as LlmChatRequest, ChatResponse as LlmChatResponse, Message as LlmMessage,
    Role as LlmRole, Tool as LlmTool, ToolCall as LlmToolCall, ToolChoice as LlmToolChoice,
};

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// 非流式对话，仅返回正文文本。
    ///
    /// 默认实现基于 `chat_full` 取正文；不支持工具调用的 Provider 可仅实现 `chat`。
    /// 注意：tool_calls 会被丢弃，携带 tools 的请求不得走此方法。
    async fn chat(&self, request: &ChatRequest) -> Result<String> {
        let response = self.chat_full(request).await?;
        if response.content.is_empty() && response.has_tool_calls() {
            tracing::warn!("chat() 丢弃了响应中的 tool_calls，仅正文被返回");
        }
        Ok(response.content)
    }

    /// 非流式对话，返回完整响应（含 tool_calls 等）。
    async fn chat_full(&self, request: &ChatRequest) -> Result<ChatResponse>;

    async fn chat_stream(&self, request: &ChatRequest) -> Result<ChatStream>;

    async fn models(&self) -> Result<Vec<String>>;
    fn provider_name(&self) -> &str;

    /// 当前 Provider 是否支持原生工具调用（请求可携带 tools 且响应可解析 tool_calls）。
    ///
    /// 默认 false；Agent 循环必须在 `supports_tools() == true` 时才携带工具。
    fn supports_tools(&self) -> bool {
        false
    }
}

/// 按 Provider 类型判定工具调用支持矩阵（基于 llm-connector 1.1.14 序列化路径实测）。
///
/// - OpenAI 系（OpenAI/DeepSeek/Moonshot/Volcengine/Zhipu/Azure/OpenAICompatible）：全支持
/// - Aliyun：仅兼容模式（OpenAI 协议）支持
/// - Anthropic/Ollama：请求序列化不含 tools 字段
/// - Google：流式丢弃 functionCall，统一按不支持处理
fn provider_supports_tools(config: &ProviderConfig) -> bool {
    match config.provider_type {
        ProviderType::OpenAI
        | ProviderType::DeepSeek
        | ProviderType::Moonshot
        | ProviderType::Volcengine
        | ProviderType::Zhipu
        | ProviderType::AzureOpenAI
        | ProviderType::OpenAICompatible => true,
        ProviderType::Aliyun => aliyun_prefers_compatible_mode(config),
        ProviderType::Anthropic
        | ProviderType::Ollama
        | ProviderType::Google
        | ProviderType::OmniHub => false,
    }
}

pub struct LlmConnector {
    client: LlmClient,
    provider_type: ProviderType,
    supports_tools: bool,
}

impl LlmConnector {
    pub fn from_config(config: &ProviderConfig) -> Result<Self> {
        let client = match config.provider_type {
            ProviderType::OpenAI => {
                let api_key = config
                    .api_key
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("API key required for OpenAI"))?;
                LlmClient::openai(api_key, provider_base_url(config, OPENAI_BASE_URL))?
            }
            ProviderType::Anthropic => {
                let api_key = config
                    .api_key
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("API key required for Anthropic"))?;
                LlmClient::anthropic(api_key, provider_base_url(config, ANTHROPIC_BASE_URL))?
            }
            ProviderType::Aliyun => {
                let api_key = config
                    .api_key
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("API key required for Aliyun"))?;
                if aliyun_prefers_compatible_mode(config) {
                    let base_url = aliyun_base_url(config);
                    LlmClient::openai_compatible(api_key, base_url, &config.name)?
                } else if let Some(base_url) = &config.api_base {
                    LlmClient::aliyun_private(api_key, base_url)?
                } else {
                    LlmClient::aliyun(api_key, ALIYUN_BASE_URL)?
                }
            }
            ProviderType::Zhipu => {
                let api_key = config
                    .api_key
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("API key required for Zhipu"))?;
                LlmClient::zhipu(api_key, provider_base_url(config, ZHIPU_BASE_URL))?
            }
            ProviderType::Ollama => LlmClient::ollama(provider_base_url(config, OLLAMA_BASE_URL))?,
            ProviderType::Volcengine => {
                let api_key = config
                    .api_key
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("API key required for Volcengine"))?;
                LlmClient::volcengine(api_key, provider_base_url(config, VOLCENGINE_BASE_URL))?
            }
            ProviderType::Moonshot => {
                let api_key = config
                    .api_key
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("API key required for Moonshot"))?;
                LlmClient::moonshot(api_key, provider_base_url(config, MOONSHOT_BASE_URL))?
            }
            ProviderType::DeepSeek => {
                let api_key = config
                    .api_key
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("API key required for DeepSeek"))?;
                LlmClient::deepseek(api_key, provider_base_url(config, DEEPSEEK_BASE_URL))?
            }
            ProviderType::Google => {
                let api_key = config
                    .api_key
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("API key required for Google"))?;
                LlmClient::google(api_key, provider_base_url(config, GOOGLE_BASE_URL))?
            }
            ProviderType::AzureOpenAI => {
                let api_key = config
                    .api_key
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("API key required for Azure OpenAI"))?;
                let base_url = config
                    .api_base
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Base URL required for Azure OpenAI"))?;
                let api_version = config
                    .api_version
                    .as_deref()
                    .unwrap_or("2024-02-15-preview");
                LlmClient::azure_openai(api_key, base_url, api_version)?
            }
            ProviderType::OpenAICompatible => {
                let api_key = config
                    .api_key
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("API key required for OpenAI Compatible"))?;
                let base_url = config
                    .api_base
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Base URL required for OpenAI Compatible"))?;
                LlmClient::openai_compatible(api_key, base_url, &config.name)?
            }
            ProviderType::OmniHub => {
                // OmniHub 使用专门的 OmniHubLLMProvider，不通过 LlmConnector 创建
                anyhow::bail!(
                    "OmniHub provider should be created via ProviderManager, not LlmConnector"
                )
            }
        };

        Ok(Self {
            client,
            provider_type: config.provider_type,
            supports_tools: provider_supports_tools(config),
        })
    }

    pub fn build_request(&self, config: &ProviderConfig, messages: Vec<Message>) -> ChatRequest {
        let mut request = ChatRequest {
            model: config.model.clone(),
            messages,
            ..Default::default()
        };

        if let Some(max_tokens) = config.max_tokens {
            request.max_tokens = Some(max_tokens as u32);
        }

        if let Some(temperature) = config.temperature {
            request.temperature = Some(temperature);
        }

        // Anthropic 专属：thinking budget 仅对 Anthropic provider 生效
        if self.provider_type == ProviderType::Anthropic {
            if let Some(budget) = config.thinking_budget {
                // llm-connector 1.1.17+ 的 ChatRequest 支持 thinking_budget 字段
                request.thinking_budget = Some(budget as u32);
            }
        }

        request
    }

    /// 构建携带工具定义的请求（供 Agent 多轮工具调用使用）。
    ///
    /// Provider 不支持工具调用时直接报错（fail-fast，避免 tools 被静默丢弃）；
    /// `tools` 为空时退化为普通请求（空数组会被 OpenAI 系端点拒绝）。
    pub fn build_request_with_tools(
        &self,
        config: &ProviderConfig,
        messages: Vec<Message>,
        tools: Vec<Tool>,
        tool_choice: Option<ToolChoice>,
    ) -> Result<ChatRequest> {
        if !self.supports_tools && !tools.is_empty() {
            anyhow::bail!(
                "Provider {} 不支持工具调用（当前协议序列化不含 tools 字段）",
                self.provider_type.as_str()
            );
        }
        let mut request = self.build_request(config, messages);
        if !tools.is_empty() {
            request.tools = Some(tools);
            request.tool_choice = tool_choice;
        }
        Ok(request)
    }
}

fn provider_base_url<'a>(config: &'a ProviderConfig, default_base_url: &'static str) -> &'a str {
    config.api_base.as_deref().unwrap_or(default_base_url)
}

fn aliyun_base_url(config: &ProviderConfig) -> &str {
    config
        .api_base
        .as_deref()
        .unwrap_or(ALIYUN_COMPATIBLE_BASE_URL)
}

fn aliyun_prefers_compatible_mode(config: &ProviderConfig) -> bool {
    config.model.starts_with("qwen3.5-")
        || config
            .api_base
            .as_deref()
            .map(|base_url| base_url.contains("/compatible-mode/"))
            .unwrap_or(false)
}

#[async_trait]
impl LlmProvider for LlmConnector {
    async fn chat_full(&self, request: &ChatRequest) -> Result<ChatResponse> {
        let response = self.client.chat(request).await?;
        Ok(response)
    }

    async fn chat_stream(&self, request: &ChatRequest) -> Result<ChatStream> {
        let stream = self.client.chat_stream(request).await?;
        Ok(Box::pin(futures::stream::StreamExt::map(
            stream,
            |result| result.map_err(|e| anyhow::anyhow!("{}", e)),
        )))
    }

    async fn models(&self) -> Result<Vec<String>> {
        let models = self.client.models().await?;
        Ok(models)
    }

    fn provider_name(&self) -> &str {
        self.provider_type.as_str()
    }

    fn supports_tools(&self) -> bool {
        self.supports_tools
    }
}

pub fn create_message(role: Role, content: impl Into<String>) -> Message {
    Message::text(role, content)
}

pub fn user_message(content: impl Into<String>) -> Message {
    create_message(Role::User, content)
}

pub fn assistant_message(content: impl Into<String>) -> Message {
    create_message(Role::Assistant, content)
}

pub fn system_message(content: impl Into<String>) -> Message {
    create_message(Role::System, content)
}

/// 构造工具结果消息（Role::Tool），供 Agent 循环回灌工具执行结果。
pub fn tool_message(content: impl Into<String>, tool_call_id: impl Into<String>) -> Message {
    Message::tool(content, tool_call_id)
}

/// 构造携带 tool_calls 的 assistant 消息，可附带文本正文。
///
/// 传入 `Some("")` 时会保留空文本块（序列化为 `"content": ""`），
/// 以兼容要求 assistant tool_calls 消息的 content 必须为字符串的严格提供方；
/// 传 `None` 则不携带正文。
pub fn assistant_tool_calls_message(tool_calls: Vec<ToolCall>, content: Option<String>) -> Message {
    let mut message = Message::assistant_with_tool_calls(tool_calls);
    if let Some(text) = content {
        message.content = vec![MessageBlock::text(text)];
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_base_url_prefers_configured_value() {
        let config = ProviderConfig {
            api_base: Some("https://custom.example.com".to_string()),
            ..Default::default()
        };

        assert_eq!(
            provider_base_url(&config, OPENAI_BASE_URL),
            "https://custom.example.com"
        );
    }

    #[test]
    fn provider_base_url_uses_default_when_config_missing() {
        let config = ProviderConfig::default();

        assert_eq!(provider_base_url(&config, OLLAMA_BASE_URL), OLLAMA_BASE_URL);
    }

    #[test]
    fn aliyun_prefers_compatible_mode_for_qwen35_models() {
        let config = ProviderConfig {
            provider_type: ProviderType::Aliyun,
            model: "qwen3.5-plus".to_string(),
            ..Default::default()
        };

        assert!(aliyun_prefers_compatible_mode(&config));
        assert_eq!(aliyun_base_url(&config), ALIYUN_COMPATIBLE_BASE_URL);
    }

    #[test]
    fn aliyun_prefers_compatible_mode_for_explicit_compatible_base_url() {
        let config = ProviderConfig {
            provider_type: ProviderType::Aliyun,
            api_base: Some("https://dashscope.aliyuncs.com/compatible-mode/v1".to_string()),
            model: "qwen-plus".to_string(),
            ..Default::default()
        };

        assert!(aliyun_prefers_compatible_mode(&config));
        assert_eq!(
            aliyun_base_url(&config),
            "https://dashscope.aliyuncs.com/compatible-mode/v1"
        );
    }

    #[test]
    fn aliyun_keeps_private_protocol_for_non_compatible_models() {
        let config = ProviderConfig {
            provider_type: ProviderType::Aliyun,
            api_base: Some("https://dashscope.aliyuncs.com".to_string()),
            model: "qwen-plus".to_string(),
            ..Default::default()
        };

        assert!(!aliyun_prefers_compatible_mode(&config));
    }

    #[test]
    fn supports_tools_matrix_matches_llm_connector_capabilities() {
        let tool_capable = [
            ProviderType::OpenAI,
            ProviderType::DeepSeek,
            ProviderType::Moonshot,
            ProviderType::Volcengine,
            ProviderType::Zhipu,
            ProviderType::AzureOpenAI,
            ProviderType::OpenAICompatible,
        ];
        for provider_type in tool_capable {
            let config = ProviderConfig {
                provider_type,
                ..Default::default()
            };
            assert!(
                provider_supports_tools(&config),
                "{provider_type:?} 应支持工具调用"
            );
        }

        let tool_incapable = [
            ProviderType::Anthropic,
            ProviderType::Ollama,
            ProviderType::Google,
            ProviderType::OmniHub,
        ];
        for provider_type in tool_incapable {
            let config = ProviderConfig {
                provider_type,
                ..Default::default()
            };
            assert!(
                !provider_supports_tools(&config),
                "{provider_type:?} 不应支持工具调用"
            );
        }
    }

    #[test]
    fn supports_tools_aliyun_only_in_compatible_mode() {
        let compatible = ProviderConfig {
            provider_type: ProviderType::Aliyun,
            model: "qwen3.5-plus".to_string(),
            ..Default::default()
        };
        assert!(provider_supports_tools(&compatible));

        let private_protocol = ProviderConfig {
            provider_type: ProviderType::Aliyun,
            api_base: Some("https://dashscope.aliyuncs.com".to_string()),
            model: "qwen-plus".to_string(),
            ..Default::default()
        };
        assert!(!provider_supports_tools(&private_protocol));
    }

    #[test]
    fn build_request_with_tools_serializes_tools_and_tool_choice() {
        let connector = LlmConnector {
            client: LlmClient::ollama(OLLAMA_BASE_URL).expect("创建测试客户端失败"),
            provider_type: ProviderType::OpenAI,
            supports_tools: true,
        };
        let config = ProviderConfig {
            provider_type: ProviderType::OpenAI,
            model: "gpt-4o".to_string(),
            ..Default::default()
        };
        let tools = vec![Tool::function(
            "read_terminal_output",
            Some("读取终端输出".to_string()),
            serde_json::json!({"type": "object", "properties": {}}),
        )];

        let request = connector
            .build_request_with_tools(
                &config,
                vec![user_message("hi")],
                tools,
                Some(ToolChoice::auto()),
            )
            .expect("构建带工具请求失败");

        let payload = serde_json::to_value(&request).expect("序列化请求失败");
        assert_eq!(payload["tools"][0]["type"], "function");
        assert_eq!(
            payload["tools"][0]["function"]["name"],
            "read_terminal_output"
        );
        assert_eq!(payload["tool_choice"], "auto");
    }

    #[test]
    fn build_request_with_tools_rejects_unsupported_provider() {
        let connector = LlmConnector {
            client: LlmClient::ollama(OLLAMA_BASE_URL).expect("创建测试客户端失败"),
            provider_type: ProviderType::Anthropic,
            supports_tools: false,
        };
        let config = ProviderConfig {
            provider_type: ProviderType::Anthropic,
            model: "claude-sonnet-4".to_string(),
            thinking_budget: Some(1024),
            ..Default::default()
        };
        let tools = vec![Tool::function(
            "t",
            None,
            serde_json::json!({"type": "object"}),
        )];

        let result =
            connector.build_request_with_tools(&config, vec![user_message("hi")], tools, None);

        assert!(result.is_err());
    }

    #[test]
    fn build_request_with_tools_degrades_to_plain_request_when_tools_empty() {
        let connector = LlmConnector {
            client: LlmClient::ollama(OLLAMA_BASE_URL).expect("创建测试客户端失败"),
            provider_type: ProviderType::Anthropic,
            supports_tools: false,
        };
        let config = ProviderConfig {
            provider_type: ProviderType::Anthropic,
            model: "claude-sonnet-4".to_string(),
            thinking_budget: Some(1024),
            ..Default::default()
        };

        let request = connector
            .build_request_with_tools(&config, vec![user_message("hi")], vec![], None)
            .expect("空 tools 应退化为普通请求");

        assert_eq!(request.thinking_budget, Some(1024));
        assert!(request.tools.is_none());
    }

    #[test]
    fn tool_message_carries_role_and_call_id() {
        let message = tool_message("执行结果", "call-1");

        assert_eq!(message.role, Role::Tool);
        assert_eq!(message.tool_call_id.as_deref(), Some("call-1"));
    }

    #[test]
    fn assistant_tool_calls_message_attaches_optional_content() {
        let tool_call = ToolCall {
            id: "call-1".to_string(),
            call_type: "function".to_string(),
            ..Default::default()
        };

        let with_content =
            assistant_tool_calls_message(vec![tool_call.clone()], Some("说明".to_string()));
        assert_eq!(with_content.role, Role::Assistant);
        assert_eq!(with_content.tool_calls.as_ref().map(Vec::len), Some(1));
        assert!(!with_content.content.is_empty());

        let without_content = assistant_tool_calls_message(vec![tool_call.clone()], None);
        assert!(without_content.content.is_empty());

        // 空字符串保留为空文本块（序列化为 "content": ""），兼容严格提供方
        let empty_content = assistant_tool_calls_message(vec![tool_call], Some(String::new()));
        assert_eq!(empty_content.content.len(), 1);
        assert!(empty_content.content[0].is_text());
    }

    #[test]
    fn assistant_tool_calls_message_serializes_arguments_as_json_string() {
        // OpenAI 规范要求 function.arguments 是 JSON 字符串而非对象
        let tool_call = ToolCall {
            id: "call-1".to_string(),
            call_type: "function".to_string(),
            function: llm_connector::types::FunctionCall {
                name: "write_to_terminal".to_string(),
                arguments: "{\"command\":\"ls\"}".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        let message = assistant_tool_calls_message(vec![tool_call], None);
        let payload = serde_json::to_value(&message).expect("序列化消息失败");

        assert_eq!(payload["role"], "assistant");
        assert_eq!(payload["tool_calls"][0]["id"], "call-1");
        assert!(
            payload["tool_calls"][0]["function"]["arguments"].is_string(),
            "arguments 必须序列化为 JSON 字符串"
        );
    }
}
