use serde::{Deserialize, Serialize};

/// Supported AI provider kinds. `Ollama` is the local default; everything else
/// is an optional BYOK external provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Ollama,
    OpenAi,
    Anthropic,
    Gemini,
    Mistral,
    XAi,
    /// Any OpenAI-compatible endpoint (LM Studio, vLLM, in-house gateways...).
    OpenAiCompatible,
}

impl ProviderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKind::Ollama => "ollama",
            ProviderKind::OpenAi => "openai",
            ProviderKind::Anthropic => "anthropic",
            ProviderKind::Gemini => "gemini",
            ProviderKind::Mistral => "mistral",
            ProviderKind::XAi => "x_ai",
            ProviderKind::OpenAiCompatible => "openai_compatible",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "ollama" => Some(ProviderKind::Ollama),
            "openai" => Some(ProviderKind::OpenAi),
            "anthropic" => Some(ProviderKind::Anthropic),
            "gemini" => Some(ProviderKind::Gemini),
            "mistral" => Some(ProviderKind::Mistral),
            "x_ai" => Some(ProviderKind::XAi),
            "openai_compatible" => Some(ProviderKind::OpenAiCompatible),
            _ => None,
        }
    }

    /// Local providers never send content off the machine.
    pub fn is_external(&self) -> bool {
        // OpenAI-compatible endpoints may be local (LM Studio) but we treat
        // them conservatively as external unless proven otherwise.
        !matches!(self, ProviderKind::Ollama)
    }
}

/// Per-provider configuration. The API key itself is NOT stored here — it
/// lives in the OS keychain, referenced by the provider kind.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub enabled: bool,
    pub base_url: String,
    pub default_model: String,
    /// Whether code snippets may be sent to this provider at all.
    pub allow_send_code: bool,
    /// True if an API key is registered in the secret store.
    pub has_api_key: bool,
}

impl ProviderConfig {
    pub fn default_for(kind: ProviderKind) -> Self {
        let (base_url, default_model) = match kind {
            ProviderKind::Ollama => ("http://localhost:11434", "qwen2.5-coder:14b"),
            ProviderKind::OpenAi => ("https://api.openai.com/v1", "gpt-4o-mini"),
            ProviderKind::Anthropic => ("https://api.anthropic.com", "claude-sonnet-5"),
            ProviderKind::Gemini => (
                "https://generativelanguage.googleapis.com",
                "gemini-2.5-flash",
            ),
            ProviderKind::Mistral => ("https://api.mistral.ai/v1", "mistral-large-latest"),
            ProviderKind::XAi => ("https://api.x.ai/v1", "grok-4"),
            ProviderKind::OpenAiCompatible => ("http://localhost:1234/v1", ""),
        };
        Self {
            kind,
            enabled: kind == ProviderKind::Ollama,
            base_url: base_url.to_string(),
            default_model: default_model.to_string(),
            allow_send_code: !kind.is_external(),
            has_api_key: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [ProviderKind; 7] = [
        ProviderKind::Ollama,
        ProviderKind::OpenAi,
        ProviderKind::Anthropic,
        ProviderKind::Gemini,
        ProviderKind::Mistral,
        ProviderKind::XAi,
        ProviderKind::OpenAiCompatible,
    ];

    #[test]
    fn kind_string_roundtrip() {
        for kind in ALL {
            assert_eq!(ProviderKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(ProviderKind::parse("unknown"), None);
    }

    #[test]
    fn only_ollama_is_local() {
        for kind in ALL {
            assert_eq!(kind.is_external(), kind != ProviderKind::Ollama);
        }
    }

    #[test]
    fn defaults_disable_code_sending_for_external_providers() {
        for kind in ALL {
            let config = ProviderConfig::default_for(kind);
            assert_eq!(config.allow_send_code, !kind.is_external());
            assert!(!config.base_url.is_empty());
        }
    }
}
