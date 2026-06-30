use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub model: String,
    pub providers: HashMap<String, ProviderConfig>,
    pub max_turns: u32,
    pub max_tokens_per_turn: u32,
    pub temperature: Option<f32>,
    pub system_prompt: Option<String>,
    pub safe_mode: bool,
    #[serde(default)]
    pub require_approval: bool,
    #[serde(default)]
    pub mcp: Vec<McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub api_key: String,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        let mut providers = HashMap::new();
        providers.insert(
            "deepseek".into(),
            ProviderConfig {
                api_key: std::env::var("DEEPSEEK_API_KEY").unwrap_or_default(),
                base_url: None,
            },
        );
        providers.insert(
            "anthropic".into(),
            ProviderConfig {
                api_key: std::env::var("ANTHROPIC_API_KEY").unwrap_or_default(),
                base_url: None,
            },
        );
        providers.insert(
            "openai".into(),
            ProviderConfig {
                api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
                base_url: None,
            },
        );
        providers.insert(
            "openrouter".into(),
            ProviderConfig {
                api_key: std::env::var("OPENROUTER_API_KEY").unwrap_or_default(),
                base_url: None,
            },
        );
        providers.insert(
            "ollama".into(),
            ProviderConfig {
                api_key: "".into(),
                base_url: Some("http://localhost:11434".into()),
            },
        );

        Self {
            model: "deepseek/deepseek-v4-pro".into(),
            providers,
            max_turns: 100,
            max_tokens_per_turn: 8192,
            temperature: None,
            system_prompt: None,
            safe_mode: false,
            require_approval: false,
            mcp: Vec::new(),
        }
    }
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let config_dir = crate::default_config_dir()?;
        let config_path = config_dir.join("config.toml");

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let mut config: Config = toml::from_str(&content)?;

            for (_, provider) in config.providers.iter_mut() {
                if provider.api_key.starts_with("$") {
                    let var = &provider.api_key[1..];
                    provider.api_key = std::env::var(var).unwrap_or_default();
                }
            }

            if config.model == "deepseek/deepseek-chat" {
                config.model = "deepseek/deepseek-v4-pro".into();
                std::fs::write(&config_path, toml::to_string_pretty(&config)?)?;
            }

            Ok(config)
        } else {
            let config = Config::default();
            let content = toml::to_string_pretty(&config)?;
            std::fs::write(&config_path, content)?;
            Ok(config)
        }
    }

    pub fn resolve_provider<'a>(
        &'a self,
        model_id: &'a str,
    ) -> Option<(&'a ProviderConfig, &'a str)> {
        if let Some((provider_name, model)) = model_id.split_once('/') {
            let config = self.providers.get(provider_name)?;
            Some((config, model))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_model() {
        let config = Config::default();
        assert_eq!(config.model, "deepseek/deepseek-v4-pro");
    }

    #[test]
    fn test_default_config_max_turns() {
        let config = Config::default();
        assert_eq!(config.max_turns, 100);
    }

    #[test]
    fn test_default_config_has_providers() {
        let config = Config::default();
        assert!(config.providers.contains_key("deepseek"));
        assert!(config.providers.contains_key("anthropic"));
        assert!(config.providers.contains_key("openai"));
    }

    #[test]
    fn test_resolve_provider_valid() {
        let mut config = Config::default();
        config.providers.insert(
            "test".into(),
            ProviderConfig {
                api_key: "k1".into(),
                base_url: Some("http://localhost".into()),
            },
        );
        let result = config.resolve_provider("test/model-v1");
        assert!(result.is_some());
        let (pc, model) = result.unwrap();
        assert_eq!(pc.api_key, "k1");
        assert_eq!(pc.base_url.as_deref(), Some("http://localhost"));
        assert_eq!(model, "model-v1");
    }

    #[test]
    fn test_resolve_provider_invalid_format() {
        let config = Config::default();
        assert!(config.resolve_provider("no-slash").is_none());
    }

    #[test]
    fn test_resolve_provider_unknown() {
        let config = Config::default();
        assert!(config.resolve_provider("nonexistent/model").is_none());
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.model, config.model);
        assert_eq!(parsed.max_turns, config.max_turns);
    }
}
