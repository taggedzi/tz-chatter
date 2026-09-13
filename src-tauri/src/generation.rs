use crate::{
    prompt::PromptBudget,
    providers::{validate_model_name, ProviderConfig},
    storage::{self, Vault},
};
use serde::{Deserialize, Serialize};
use std::{fmt, fs, path::PathBuf};

pub const GENERATION_SCHEMA_VERSION: u32 = 1;
pub const GENERATION_RELATIVE: &str = ".tz-chatter/generation.json";
pub const EXTRACTION_TEMPERATURE: f64 = 0.0;
pub const EXTRACTION_MAX_TOKENS: u32 = 1024;
pub const MIN_TEMPERATURE: f64 = 0.0;
pub const MAX_TEMPERATURE: f64 = 2.0;
pub const MIN_MAX_TOKENS: u32 = 16;
pub const MAX_MAX_TOKENS: u32 = 2048;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CharacterGeneration {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

impl Default for CharacterGeneration {
    fn default() -> Self {
        Self {
            schema_version: GENERATION_SCHEMA_VERSION,
            chat_model: None,
            temperature: None,
            max_tokens: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedGeneration {
    pub chat_model: String,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenerationError {
    Invalid(String),
    Persistence(String),
}

impl fmt::Display for GenerationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => write!(formatter, "{message}"),
            Self::Persistence(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for GenerationError {}

impl From<storage::StorageError> for GenerationError {
    fn from(error: storage::StorageError) -> Self {
        Self::Persistence(error.to_string())
    }
}

pub fn generation_path(vault: &Vault) -> PathBuf {
    vault.root().join(GENERATION_RELATIVE)
}

pub fn load(vault: &Vault) -> Result<CharacterGeneration, GenerationError> {
    let path = generation_path(vault);
    if !path.exists() {
        return Ok(CharacterGeneration::default());
    }
    let bytes = fs::read(&path).map_err(|error| GenerationError::Persistence(error.to_string()))?;
    let generation: CharacterGeneration = serde_json::from_slice(&bytes).map_err(|error| {
        GenerationError::Invalid(format!("invalid generation settings: {error}"))
    })?;
    validate(&generation)?;
    Ok(normalize(generation))
}

pub fn save(vault: &Vault, generation: &CharacterGeneration) -> Result<(), GenerationError> {
    let normalized = normalize(generation.clone());
    validate(&normalized)?;
    let path = generation_path(vault);
    let bytes = serde_json::to_vec_pretty(&normalized)
        .map_err(|error| GenerationError::Persistence(error.to_string()))?;
    storage::atomic_write(&path, bytes)?;
    Ok(())
}

pub fn resolve_chat(
    provider: &ProviderConfig,
    generation: &CharacterGeneration,
) -> Result<ResolvedGeneration, GenerationError> {
    let settings = normalize(generation.clone());
    validate(&settings)?;
    let chat_model = settings
        .chat_model
        .filter(|model| !model.is_empty())
        .unwrap_or_else(|| provider.chat_model.clone());
    validate_model_name(&chat_model, "chat_model")
        .map_err(|error| GenerationError::Invalid(error.to_string()))?;
    Ok(ResolvedGeneration {
        chat_model,
        temperature: settings.temperature,
        max_tokens: settings.max_tokens,
    })
}

pub fn resolve_extraction(
    provider: &ProviderConfig,
    generation: &CharacterGeneration,
) -> Result<ResolvedGeneration, GenerationError> {
    let mut resolved = resolve_chat(provider, generation)?;
    resolved.temperature = Some(EXTRACTION_TEMPERATURE);
    resolved.max_tokens = Some(EXTRACTION_MAX_TOKENS);
    Ok(resolved)
}

pub fn prompt_budget(resolved: &ResolvedGeneration) -> PromptBudget {
    let default = PromptBudget::default();
    match resolved.max_tokens {
        Some(max_tokens) => PromptBudget {
            context_tokens: default.context_tokens,
            reserved_output_tokens: (max_tokens as usize)
                .clamp(1, default.context_tokens.saturating_sub(256)),
        },
        None => default,
    }
}

fn normalize(mut generation: CharacterGeneration) -> CharacterGeneration {
    generation.schema_version = GENERATION_SCHEMA_VERSION;
    generation.chat_model = generation
        .chat_model
        .map(|model| model.trim().to_owned())
        .filter(|model| !model.is_empty());
    generation
}

fn validate(generation: &CharacterGeneration) -> Result<(), GenerationError> {
    if generation.schema_version != GENERATION_SCHEMA_VERSION {
        return Err(GenerationError::Invalid(format!(
            "unsupported generation schema version {}",
            generation.schema_version
        )));
    }
    if let Some(model) = &generation.chat_model {
        validate_model_name(model, "chat_model")
            .map_err(|error| GenerationError::Invalid(error.to_string()))?;
    }
    if let Some(temperature) = generation.temperature {
        if !temperature.is_finite() || !(MIN_TEMPERATURE..=MAX_TEMPERATURE).contains(&temperature) {
            return Err(GenerationError::Invalid(format!(
                "temperature must be between {MIN_TEMPERATURE} and {MAX_TEMPERATURE}"
            )));
        }
    }
    if let Some(max_tokens) = generation.max_tokens {
        if !(MIN_MAX_TOKENS..=MAX_MAX_TOKENS).contains(&max_tokens) {
            return Err(GenerationError::Invalid(format!(
                "max_tokens must be between {MIN_MAX_TOKENS} and {MAX_MAX_TOKENS}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ProviderKind;
    use crate::storage::{new_stable_id, CharacterDefinition};

    fn root(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("tz-chatter-generation-{label}-{}", new_stable_id()))
    }

    fn provider() -> ProviderConfig {
        ProviderConfig {
            id: "ollama-local".into(),
            kind: ProviderKind::Ollama,
            endpoint: "http://127.0.0.1:11434".into(),
            chat_model: "llama3.2:latest".into(),
            embedding_model: None,
            bearer_token: None,
        }
    }

    #[test]
    fn missing_file_inherits_provider_chat_model() {
        let root = root("missing");
        let vault = Vault::create(&root).unwrap();
        let loaded = load(&vault).unwrap();
        let resolved = resolve_chat(&provider(), &loaded).unwrap();
        assert_eq!(resolved.chat_model, "llama3.2:latest");
        assert_eq!(resolved.temperature, None);
        assert_eq!(resolved.max_tokens, None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn save_round_trips_overrides_without_changing_character_identity() {
        let root = root("round-trip");
        let vault = Vault::create(&root).unwrap();
        let character = CharacterDefinition::new("lyra", "Lyra", "You are Lyra.");
        vault.save_character(&character).unwrap();
        let before = fs::read(vault.root().join("character.md")).unwrap();
        save(
            &vault,
            &CharacterGeneration {
                schema_version: 1,
                chat_model: Some("  qwen2.5:7b  ".into()),
                temperature: Some(0.8),
                max_tokens: Some(256),
            },
        )
        .unwrap();
        let loaded = load(&vault).unwrap();
        assert_eq!(loaded.chat_model.as_deref(), Some("qwen2.5:7b"));
        assert_eq!(loaded.temperature, Some(0.8));
        assert_eq!(loaded.max_tokens, Some(256));
        assert_eq!(fs::read(vault.root().join("character.md")).unwrap(), before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn whitespace_chat_model_inherits_provider_default() {
        let resolved = resolve_chat(
            &provider(),
            &CharacterGeneration {
                schema_version: 1,
                chat_model: Some("   ".into()),
                temperature: Some(0.2),
                max_tokens: None,
            },
        )
        .unwrap();
        assert_eq!(resolved.chat_model, "llama3.2:latest");
        assert_eq!(resolved.temperature, Some(0.2));
    }

    #[test]
    fn extraction_keeps_character_model_and_ignores_sampling() {
        let resolved = resolve_extraction(
            &provider(),
            &CharacterGeneration {
                schema_version: 1,
                chat_model: Some("lyra-voice".into()),
                temperature: Some(0.9),
                max_tokens: Some(128),
            },
        )
        .unwrap();
        assert_eq!(resolved.chat_model, "lyra-voice");
        assert_eq!(resolved.temperature, Some(EXTRACTION_TEMPERATURE));
        assert_eq!(resolved.max_tokens, Some(EXTRACTION_MAX_TOKENS));
    }

    #[test]
    fn rejects_temperature_and_max_tokens_outside_range() {
        let vault = Vault::create(root("invalid")).unwrap();
        assert!(save(
            &vault,
            &CharacterGeneration {
                schema_version: 1,
                chat_model: None,
                temperature: Some(2.5),
                max_tokens: None,
            },
        )
        .is_err());
        assert!(save(
            &vault,
            &CharacterGeneration {
                schema_version: 1,
                chat_model: None,
                temperature: None,
                max_tokens: Some(8),
            },
        )
        .is_err());
        fs::remove_dir_all(vault.root()).ok();
    }

    #[test]
    fn prompt_budget_reserves_character_max_tokens() {
        let budget = prompt_budget(&ResolvedGeneration {
            chat_model: "x".into(),
            temperature: None,
            max_tokens: Some(256),
        });
        assert_eq!(budget.reserved_output_tokens, 256);
        assert_eq!(
            prompt_budget(&ResolvedGeneration {
                chat_model: "x".into(),
                temperature: None,
                max_tokens: None,
            })
            .reserved_output_tokens,
            PromptBudget::default().reserved_output_tokens
        );
    }
}
