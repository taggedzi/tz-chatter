use serde::{Deserialize, Serialize};
use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};
use url::Url;

pub const SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Ollama,
    LmStudio,
    OpenAiCompatible,
}

impl ProviderKind {
    pub fn uses_openai_compatible_http(&self) -> bool {
        matches!(self, Self::LmStudio | Self::OpenAiCompatible)
    }

    pub fn wire_label(&self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::LmStudio => "lm_studio",
            Self::OpenAiCompatible => "openai_compatible",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub health: bool,
    pub model_discovery: bool,
    pub chat_streaming: bool,
    pub cancellation: bool,
    pub embeddings: bool,
}

impl ProviderCapabilities {
    pub fn baseline(kind: &ProviderKind) -> Self {
        match kind {
            ProviderKind::Ollama => Self {
                health: true,
                model_discovery: true,
                chat_streaming: true,
                cancellation: true,
                embeddings: true,
            },
            // LM Studio and other compatible servers share the OpenAI HTTP transport.
            // Embeddings are negotiated from the server rather than assumed identical.
            ProviderKind::LmStudio | ProviderKind::OpenAiCompatible => Self {
                health: true,
                model_discovery: true,
                chat_streaming: true,
                cancellation: true,
                embeddings: true,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderConfig {
    pub id: String,
    pub kind: ProviderKind,
    pub endpoint: String,
    pub chat_model: String,
    #[serde(default)]
    pub embedding_model: Option<String>,
    #[serde(default)]
    pub bearer_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RedactedProviderConfig {
    pub id: String,
    pub kind: ProviderKind,
    pub endpoint: String,
    pub chat_model: String,
    pub embedding_model: Option<String>,
    pub has_bearer_token: bool,
}

impl From<&ProviderConfig> for RedactedProviderConfig {
    fn from(config: &ProviderConfig) -> Self {
        Self {
            id: config.id.clone(),
            kind: config.kind.clone(),
            endpoint: config.endpoint.clone(),
            chat_model: config.chat_model.clone(),
            embedding_model: config.embedding_model.clone(),
            has_bearer_token: config
                .bearer_token
                .as_ref()
                .is_some_and(|token| !token.is_empty()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderSettings {
    pub schema_version: u32,
    pub active_provider_id: Option<String>,
    pub providers: Vec<ProviderConfig>,
}

impl Default for ProviderSettings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            active_provider_id: None,
            providers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthRequest {
    pub provider_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthResponse {
    pub provider_id: String,
    pub reachable: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelDescriptor {
    pub id: String,
    pub kind: String,
    pub supports_chat: bool,
    pub supports_embeddings: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscoveryResponse {
    pub provider_id: String,
    pub models: Vec<ModelDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatRequest {
    pub provider_id: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub cancellation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum ChatStreamEvent {
    Started { cancellation_id: String },
    Delta { text: String },
    Completed { finish_reason: Option<String> },
    Cancelled,
    Failed { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmbeddingRequest {
    pub provider_id: String,
    pub model: String,
    pub input: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmbeddingResponse {
    pub model: String,
    pub dimensions: usize,
    pub vectors: Vec<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CancellationRequest {
    pub cancellation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProviderError {
    InvalidConfig(String),
    UnsupportedCapability {
        capability: String,
        provider: ProviderKind,
    },
    NotFound(String),
    Unavailable(String),
    Cancelled,
    Persistence(String),
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(message) => write!(f, "invalid provider configuration: {message}"),
            Self::UnsupportedCapability {
                capability,
                provider,
            } => {
                write!(f, "provider {provider:?} does not support {capability}")
            }
            Self::NotFound(message) => write!(f, "not found: {message}"),
            Self::Unavailable(message) => write!(f, "provider unavailable: {message}"),
            Self::Cancelled => write!(f, "request cancelled"),
            Self::Persistence(message) => write!(f, "settings persistence failed: {message}"),
        }
    }
}

impl std::error::Error for ProviderError {}

impl From<io::Error> for ProviderError {
    fn from(error: io::Error) -> Self {
        Self::Persistence(error.to_string())
    }
}

impl From<ProviderError> for String {
    fn from(error: ProviderError) -> Self {
        error.to_string()
    }
}

pub fn validate_config(config: &ProviderConfig) -> Result<(), ProviderError> {
    if config.id.is_empty() || config.id.len() > 64 {
        return Err(ProviderError::InvalidConfig(
            "id must be between 1 and 64 characters".into(),
        ));
    }
    if !config
        .id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(ProviderError::InvalidConfig(
            "id may contain only letters, numbers, '-' and '_'".into(),
        ));
    }
    validate_endpoint(&config.endpoint)?;
    validate_model_name(&config.chat_model, "chat_model")?;
    if let Some(model) = &config.embedding_model {
        validate_model_name(model, "embedding_model")?;
    }
    if config
        .bearer_token
        .as_ref()
        .is_some_and(|token| token.contains(['\r', '\n']))
    {
        return Err(ProviderError::InvalidConfig(
            "bearer_token may not contain line breaks".into(),
        ));
    }
    Ok(())
}

pub fn validate_endpoint(endpoint: &str) -> Result<Url, ProviderError> {
    let parsed = Url::parse(endpoint)
        .map_err(|_| ProviderError::InvalidConfig("endpoint must be a valid URL".into()))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(ProviderError::InvalidConfig(
            "endpoint must use http or https and include a host".into(),
        ));
    }
    if parsed.username() != ""
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(ProviderError::InvalidConfig(
            "endpoint may not contain credentials, query parameters, or fragments".into(),
        ));
    }
    Ok(parsed)
}

pub fn validate_model_name(model: &str, field: &str) -> Result<(), ProviderError> {
    if model.trim().is_empty() || model.len() > 256 {
        return Err(ProviderError::InvalidConfig(format!(
            "{field} must be between 1 and 256 characters"
        )));
    }
    Ok(())
}

pub fn require_capability(
    kind: &ProviderKind,
    capabilities: &ProviderCapabilities,
    capability: &str,
) -> Result<(), ProviderError> {
    let supported = match capability {
        "health" => capabilities.health,
        "model_discovery" => capabilities.model_discovery,
        "chat_streaming" => capabilities.chat_streaming,
        "cancellation" => capabilities.cancellation,
        "embeddings" => capabilities.embeddings,
        _ => false,
    };
    if supported {
        Ok(())
    } else {
        Err(ProviderError::UnsupportedCapability {
            capability: capability.into(),
            provider: kind.clone(),
        })
    }
}

pub fn settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join("provider-settings.json")
}

pub fn load_settings(path: &Path) -> Result<ProviderSettings, ProviderError> {
    if !path.exists() {
        return Ok(ProviderSettings::default());
    }
    let contents = fs::read_to_string(path)?;
    let settings: ProviderSettings = serde_json::from_str(&contents)
        .map_err(|error| ProviderError::Persistence(format!("invalid settings file: {error}")))?;
    if settings.schema_version != SETTINGS_SCHEMA_VERSION {
        return Err(ProviderError::Persistence(format!(
            "unsupported settings schema version {}",
            settings.schema_version
        )));
    }
    for config in &settings.providers {
        validate_config(config)?;
    }
    Ok(settings)
}

pub fn save_settings(path: &Path, settings: &ProviderSettings) -> Result<(), ProviderError> {
    if settings.schema_version != SETTINGS_SCHEMA_VERSION {
        return Err(ProviderError::Persistence(
            "settings schema version does not match the application".into(),
        ));
    }
    if let Some(active_id) = &settings.active_provider_id {
        if !settings
            .providers
            .iter()
            .any(|provider| &provider.id == active_id)
        {
            return Err(ProviderError::InvalidConfig(
                "active_provider_id must refer to a configured provider".into(),
            ));
        }
    }
    for config in &settings.providers {
        validate_config(config)?;
    }

    let parent = path
        .parent()
        .ok_or_else(|| ProviderError::Persistence("settings path has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension("json.tmp");
    let contents = serde_json::to_vec_pretty(settings)
        .map_err(|error| ProviderError::Persistence(error.to_string()))?;
    fs::write(&temporary, contents)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> ProviderConfig {
        ProviderConfig {
            id: "local-ollama".into(),
            kind: ProviderKind::Ollama,
            endpoint: "http://127.0.0.1:11434".into(),
            chat_model: "llama3.2".into(),
            embedding_model: Some("nomic-embed-text".into()),
            bearer_token: Some("secret-value".into()),
        }
    }

    #[test]
    fn rejects_invalid_endpoint_and_credentials() {
        let mut config = sample_config();
        config.endpoint = "ftp://localhost".into();
        assert!(matches!(
            validate_config(&config),
            Err(ProviderError::InvalidConfig(_))
        ));
        config.endpoint = "http://user:password@localhost".into();
        assert!(matches!(
            validate_config(&config),
            Err(ProviderError::InvalidConfig(_))
        ));
    }

    #[test]
    fn reports_compatible_embedding_transport_capability() {
        let kind = ProviderKind::OpenAiCompatible;
        let capabilities = ProviderCapabilities::baseline(&kind);
        assert!(require_capability(&kind, &capabilities, "embeddings").is_ok());
        assert!(require_capability(&kind, &capabilities, "chat_streaming").is_ok());
    }

    #[test]
    fn lm_studio_kind_round_trips_and_reports_chat_capabilities() {
        let kind = ProviderKind::LmStudio;
        assert_eq!(serde_json::to_value(&kind).unwrap(), "lm_studio");
        assert!(kind.uses_openai_compatible_http());
        let capabilities = ProviderCapabilities::baseline(&kind);
        assert!(require_capability(&kind, &capabilities, "model_discovery").is_ok());
        assert!(require_capability(&kind, &capabilities, "chat_streaming").is_ok());
        assert!(require_capability(&kind, &capabilities, "embeddings").is_ok());
        let config = ProviderConfig {
            id: "lm-studio-local".into(),
            kind: ProviderKind::LmStudio,
            endpoint: "http://127.0.0.1:1234/v1".into(),
            chat_model: "local-model".into(),
            embedding_model: None,
            bearer_token: None,
        };
        validate_config(&config).unwrap();
        let restored: ProviderKind = serde_json::from_str("\"lm_studio\"").unwrap();
        assert_eq!(restored, ProviderKind::LmStudio);
        let compatible: ProviderKind = serde_json::from_str("\"open_ai_compatible\"").unwrap();
        assert_eq!(compatible, ProviderKind::OpenAiCompatible);
    }

    #[test]
    fn redaction_never_contains_the_bearer_token() {
        let config = sample_config();
        let redacted = serde_json::to_string(&RedactedProviderConfig::from(&config)).unwrap();
        assert!(!redacted.contains("secret-value"));
        assert!(redacted.contains("has_bearer_token"));
    }

    #[test]
    fn settings_round_trip_and_missing_file_are_supported() {
        let root =
            std::env::temp_dir().join(format!("tz-chatter-provider-test-{}", std::process::id()));
        let path = settings_path(&root);
        let settings = ProviderSettings {
            schema_version: SETTINGS_SCHEMA_VERSION,
            active_provider_id: Some("local-ollama".into()),
            providers: vec![sample_config()],
        };
        assert_eq!(load_settings(&path).unwrap(), ProviderSettings::default());
        save_settings(&path, &settings).unwrap();
        assert_eq!(load_settings(&path).unwrap(), settings);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_unknown_active_provider() {
        let root = std::env::temp_dir().join(format!(
            "tz-chatter-provider-test-invalid-{}",
            std::process::id()
        ));
        let path = settings_path(&root);
        let settings = ProviderSettings {
            schema_version: SETTINGS_SCHEMA_VERSION,
            active_provider_id: Some("missing".into()),
            providers: Vec::new(),
        };
        assert!(matches!(
            save_settings(&path, &settings),
            Err(ProviderError::InvalidConfig(_))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unavailable_health_and_cancellation_are_explicit_contract_values() {
        let health = HealthResponse {
            provider_id: "local-ollama".into(),
            reachable: false,
            detail: Some("connection refused".into()),
        };
        let health_json = serde_json::to_string(&health).unwrap();
        assert!(health_json.contains("\"reachable\":false"));

        let cancelled = serde_json::to_string(&ChatStreamEvent::Cancelled).unwrap();
        assert!(cancelled.contains("cancelled"));
        assert_eq!(ProviderError::Cancelled.to_string(), "request cancelled");
    }
}
