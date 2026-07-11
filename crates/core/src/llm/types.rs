use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderType {
    OpenAI,
    Anthropic,
    Aliyun,
    Zhipu,
    Ollama,
    Volcengine,
    Moonshot,
    DeepSeek,
    Google,
    AzureOpenAI,
    OpenAICompatible,
    OmniHub,
}

impl ProviderType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderType::OpenAI => "openai",
            ProviderType::Anthropic => "anthropic",
            ProviderType::Aliyun => "aliyun",
            ProviderType::Zhipu => "zhipu",
            ProviderType::Ollama => "ollama",
            ProviderType::Volcengine => "volcengine",
            ProviderType::Moonshot => "moonshot",
            ProviderType::DeepSeek => "deepseek",
            ProviderType::Google => "google",
            ProviderType::AzureOpenAI => "azure_openai",
            ProviderType::OpenAICompatible => "openai_compatible",
            ProviderType::OmniHub => "onet_cli",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "openai" => Some(ProviderType::OpenAI),
            "anthropic" => Some(ProviderType::Anthropic),
            "aliyun" => Some(ProviderType::Aliyun),
            "zhipu" => Some(ProviderType::Zhipu),
            "ollama" => Some(ProviderType::Ollama),
            "volcengine" => Some(ProviderType::Volcengine),
            "moonshot" => Some(ProviderType::Moonshot),
            "deepseek" => Some(ProviderType::DeepSeek),
            "google" => Some(ProviderType::Google),
            "azure_openai" => Some(ProviderType::AzureOpenAI),
            "openai_compatible" => Some(ProviderType::OpenAICompatible),
            "onet_cli" => Some(ProviderType::OmniHub),
            _ => None,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ProviderType::OpenAI => "OpenAI",
            ProviderType::Anthropic => "Anthropic",
            ProviderType::Aliyun => "Aliyun (DashScope)",
            ProviderType::Zhipu => "Zhipu (GLM)",
            ProviderType::Ollama => "Ollama",
            ProviderType::Volcengine => "Volcengine",
            ProviderType::Moonshot => "Moonshot",
            ProviderType::DeepSeek => "DeepSeek",
            ProviderType::Google => "Google (Gemini)",
            ProviderType::AzureOpenAI => "Azure OpenAI",
            ProviderType::OpenAICompatible => "OpenAI Compatible",
            ProviderType::OmniHub => "OmniHub",
        }
    }

    pub fn all() -> Vec<Self> {
        vec![
            ProviderType::OpenAI,
            ProviderType::Anthropic,
            ProviderType::Aliyun,
            ProviderType::Zhipu,
            ProviderType::Ollama,
            ProviderType::Volcengine,
            ProviderType::Moonshot,
            ProviderType::DeepSeek,
            ProviderType::Google,
            ProviderType::AzureOpenAI,
            ProviderType::OpenAICompatible,
            ProviderType::OmniHub,
        ]
    }

    pub fn requires_api_key(&self) -> bool {
        !matches!(self, ProviderType::Ollama | ProviderType::OmniHub)
    }

    /// 是否为内置 provider（不需要用户配置）
    pub fn is_builtin(&self) -> bool {
        matches!(self, ProviderType::OmniHub)
    }

    /// 返回用户可配置的 provider 类型列表（不包含内置类型）
    pub fn user_configurable() -> Vec<Self> {
        Self::all()
            .into_iter()
            .filter(|p| !p.is_builtin())
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: i64,
    pub name: String,
    pub provider_type: ProviderType,
    pub api_key: Option<String>,
    pub api_base: Option<String>,
    pub api_version: Option<String>,
    pub model: String,
    pub models: Vec<String>,
    pub max_tokens: Option<i32>,
    pub temperature: Option<f32>,
    /// Anthropic 专属：thinking budget（最大 thinking token 数）
    pub thinking_budget: Option<i32>,
    pub enabled: bool,
    pub is_default: bool,
    pub cloud_id: Option<String>,
    pub last_synced_at: Option<i64>,
    pub sync_enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            provider_type: ProviderType::OpenAI,
            api_key: None,
            api_base: None,
            api_version: None,
            model: String::new(),
            models: Vec::new(),
            max_tokens: None,
            temperature: None,
            thinking_budget: None,
            enabled: true,
            is_default: false,
            cloud_id: None,
            last_synced_at: None,
            sync_enabled: true,
            created_at: 0,
            updated_at: 0,
        }
    }
}

impl ProviderConfig {
    /// 是否为内置 provider
    pub fn is_builtin(&self) -> bool {
        self.provider_type.is_builtin()
    }
}

// ===== 云同步支持 =====

use crate::cloud_sync::sync_type::SyncableItem;

impl SyncableItem for ProviderConfig {
    fn local_id(&self) -> Option<i64> {
        Some(self.id)
    }

    fn set_local_id(&mut self, id: Option<i64>) {
        self.id = id.unwrap_or(0);
    }

    fn item_name(&self) -> &str {
        &self.name
    }

    fn cloud_id(&self) -> Option<&str> {
        self.cloud_id.as_deref()
    }

    fn set_cloud_id(&mut self, cloud_id: Option<String>) {
        self.cloud_id = cloud_id;
    }

    fn updated_at(&self) -> Option<i64> {
        Some(self.updated_at)
    }

    fn last_synced_at(&self) -> Option<i64> {
        self.last_synced_at
    }

    // 使用简单时间戳比较（与 Certificate 一致）
    fn uses_sync_state(&self) -> bool {
        false
    }

    /// 关闭同步的提供商不应被上传或被云端匹配覆盖。
    fn sync_enabled(&self) -> bool {
        self.sync_enabled
    }
}

#[cfg(test)]
mod tests {
    use super::ProviderConfig;
    use crate::cloud_sync::sync_type::SyncableItem;

    /// 回归用例：`sync_enabled = false` 的提供商在同步计算时应被排除。
    /// 否则关闭同步的本地条目会被云端同名项覆盖。
    #[test]
    fn sync_disabled_provider_excluded_from_sync() {
        let mut item = ProviderConfig::default();
        assert!(item.sync_enabled, "默认应启用同步");

        item.sync_enabled = false;
        assert!(
            !item.sync_enabled(),
            "关闭同步的提供商应当不参与同步"
        );
    }
}
