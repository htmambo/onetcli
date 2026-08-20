//! OmniHub LLM Provider
//!
//! 使用云端同步服务提供的 AI 代理能力。
//! 委托给 CloudApiClient 实现，支持 OpenAI 兼容的 /chat/completions 接口。

use anyhow::Result;
use async_trait::async_trait;
use llm_connector::types::{ChatRequest, ChatResponse};
use std::sync::Arc;

use super::connector::{ChatStream, LlmProvider};
use crate::cloud_sync::client::CloudApiClient;

/// OmniHub LLM Provider
///
/// 使用云端 API 作为 AI 代理，委托给 CloudApiClient 实现。
pub struct OmniHubLLMProvider {
    cloud_client: Arc<dyn CloudApiClient>,
}

impl OmniHubLLMProvider {
    /// 创建新的 OmniHub LLM Provider
    pub fn new(cloud_client: Arc<dyn CloudApiClient>) -> Self {
        Self { cloud_client }
    }
}

#[async_trait]
impl LlmProvider for OmniHubLLMProvider {
    async fn chat_full(&self, request: &ChatRequest) -> Result<ChatResponse> {
        let content = self
            .cloud_client
            .chat(request)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        // 云端接口仅返回正文文本，构造仅含 content 的响应
        Ok(ChatResponse {
            content,
            ..Default::default()
        })
    }

    async fn chat_stream(&self, request: &ChatRequest) -> Result<ChatStream> {
        self.cloud_client
            .chat_stream(request)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    async fn models(&self) -> Result<Vec<String>> {
        self.cloud_client
            .list_models()
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    fn provider_name(&self) -> &str {
        "omnihub"
    }

    fn supports_tools(&self) -> bool {
        // 云端 /chat 接口尚未实现（sync_server 返回 unsupported），tools 透传无从验证，
        // 按不支持处理；待云端落地 OpenAI 兼容 tools 后再打开。
        false
    }
}
