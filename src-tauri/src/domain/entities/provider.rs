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
    /// Monthly spend ceiling in USD for external providers (None = no limit).
    pub monthly_budget_usd: Option<f64>,
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
            monthly_budget_usd: None,
            has_api_key: false,
        }
    }
}

/// Rough cost estimate in USD. Character counts are converted at ~4 chars
/// per token; prices are indicative list prices per 1M tokens. Local
/// providers cost nothing.
pub fn estimate_cost_usd(
    kind: ProviderKind,
    model: &str,
    prompt_chars: usize,
    completion_chars: usize,
) -> f64 {
    if !kind.is_external() {
        return 0.0;
    }
    let model = model.to_ascii_lowercase();
    // (input, output) USD per 1M tokens
    let (input, output) = match kind {
        ProviderKind::OpenAi => {
            if model.contains("mini") || model.contains("nano") {
                (0.15, 0.60)
            } else {
                (2.50, 10.00)
            }
        }
        ProviderKind::Anthropic => {
            if model.contains("haiku") {
                (0.80, 4.00)
            } else if model.contains("opus") {
                (15.00, 75.00)
            } else {
                (3.00, 15.00)
            }
        }
        ProviderKind::Gemini => {
            if model.contains("flash") {
                (0.30, 2.50)
            } else {
                (1.25, 10.00)
            }
        }
        ProviderKind::Mistral => (2.00, 6.00),
        ProviderKind::XAi => (3.00, 15.00),
        ProviderKind::OpenAiCompatible => (0.50, 1.50),
        ProviderKind::Ollama => (0.0, 0.0),
    };
    let prompt_tokens = prompt_chars as f64 / 4.0;
    let completion_tokens = completion_chars as f64 / 4.0;
    (prompt_tokens * input + completion_tokens * output) / 1_000_000.0
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
