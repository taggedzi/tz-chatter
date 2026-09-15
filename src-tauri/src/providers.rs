use serde::{Deserialize, Serialize};
use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};
use url::Url;

pub const SETTINGS_SCHEMA_VERSION: u32 = 2;
const LEGACY_SETTINGS_SCHEMA_VERSION: u32 = 1;
const CREDENTIAL_SERVICE: &str = "tz-chatter";

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

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
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

impl fmt::Debug for ProviderConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderConfig")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("endpoint", &self.endpoint)
            .field("chat_model", &self.chat_model)
            .field("embedding_model", &self.embedding_model)
            .field(
                "has_bearer_token",
                &self
                    .bearer_token
                    .as_ref()
                    .is_some_and(|secret| !secret.is_empty()),
            )
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredProviderConfig {
    pub id: String,
    pub kind: ProviderKind,
    pub endpoint: String,
    pub chat_model: String,
    #[serde(default)]
    pub embedding_model: Option<String>,
    #[serde(default)]
    pub has_bearer_token: bool,
    // Accepted from save requests and legacy schema-1 files, but never serialized
    // back to disk or returned by the load command.
    #[serde(default, skip_serializing)]
    pub bearer_token: Option<String>,
}

impl fmt::Debug for StoredProviderConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoredProviderConfig")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("endpoint", &self.endpoint)
            .field("chat_model", &self.chat_model)
            .field("embedding_model", &self.embedding_model)
            .field("has_bearer_token", &self.has_bearer_token)
            .finish()
    }
}

impl StoredProviderConfig {
    pub fn runtime_config(&self) -> ProviderConfig {
        ProviderConfig {
            id: self.id.clone(),
            kind: self.kind.clone(),
            endpoint: self.endpoint.clone(),
            chat_model: self.chat_model.clone(),
            embedding_model: self.embedding_model.clone(),
            bearer_token: None,
        }
    }
}

impl From<&ProviderConfig> for StoredProviderConfig {
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
            bearer_token: config.bearer_token.clone(),
        }
    }
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
    pub providers: Vec<StoredProviderConfig>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatRequest {
    pub provider_id: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub cancellation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub json_mode: bool,
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

pub trait CredentialStore {
    fn get(&self, provider_id: &str) -> Result<Option<String>, ProviderError>;
    fn set(&self, provider_id: &str, secret: &str) -> Result<(), ProviderError>;
    fn delete(&self, provider_id: &str) -> Result<(), ProviderError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OsCredentialStore;

impl OsCredentialStore {
    fn entry(provider_id: &str) -> Result<keyring::Entry, ProviderError> {
        keyring::Entry::new(
            CREDENTIAL_SERVICE,
            &format!("provider:{provider_id}:bearer-token"),
        )
        .map_err(|error| credential_error("open", error))
    }
}

impl CredentialStore for OsCredentialStore {
    fn get(&self, provider_id: &str) -> Result<Option<String>, ProviderError> {
        match Self::entry(provider_id)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(credential_error("read", error)),
        }
    }

    fn set(&self, provider_id: &str, secret: &str) -> Result<(), ProviderError> {
        Self::entry(provider_id)?
            .set_password(secret)
            .map_err(|error| credential_error("write", error))
    }

    fn delete(&self, provider_id: &str) -> Result<(), ProviderError> {
        match Self::entry(provider_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(credential_error("delete", error)),
        }
    }
}

fn credential_error(action: &str, error: keyring::Error) -> ProviderError {
    ProviderError::Persistence(format!(
        "could not {action} the provider token in the operating-system credential store: {error}"
    ))
}

fn validate_stored_config(config: &StoredProviderConfig) -> Result<(), ProviderError> {
    let mut runtime = config.runtime_config();
    runtime.bearer_token = config.bearer_token.clone();
    validate_config(&runtime)
}

fn write_settings_file(path: &Path, settings: &ProviderSettings) -> Result<(), ProviderError> {
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

fn restore_credentials(
    credentials: &dyn CredentialStore,
    originals: &[(String, Option<String>)],
) -> Result<(), ProviderError> {
    for (provider_id, original) in originals.iter().rev() {
        match original {
            Some(secret) => credentials.set(provider_id, secret)?,
            None => credentials.delete(provider_id)?,
        }
    }
    Ok(())
}

fn rollback_error(
    credentials: &dyn CredentialStore,
    originals: &[(String, Option<String>)],
    error: ProviderError,
) -> ProviderError {
    match restore_credentials(credentials, originals) {
        Ok(()) => error,
        Err(rollback) => ProviderError::Persistence(format!(
            "{error}; credential rollback also failed: {rollback}"
        )),
    }
}

pub fn load_settings(path: &Path) -> Result<ProviderSettings, ProviderError> {
    load_settings_with_store(path, &OsCredentialStore)
}

pub fn load_settings_with_store(
    path: &Path,
    credentials: &dyn CredentialStore,
) -> Result<ProviderSettings, ProviderError> {
    if !path.exists() {
        return Ok(ProviderSettings::default());
    }
    let contents = fs::read_to_string(path)?;
    let mut settings: ProviderSettings = serde_json::from_str(&contents)
        .map_err(|error| ProviderError::Persistence(format!("invalid settings file: {error}")))?;
    if !matches!(
        settings.schema_version,
        LEGACY_SETTINGS_SCHEMA_VERSION | SETTINGS_SCHEMA_VERSION
    ) {
        return Err(ProviderError::Persistence(format!(
            "unsupported settings schema version {}",
            settings.schema_version
        )));
    }
    for config in &settings.providers {
        validate_stored_config(config)?;
    }

    let mut provider_ids = std::collections::HashSet::new();
    if settings
        .providers
        .iter()
        .any(|config| !provider_ids.insert(&config.id))
    {
        return Err(ProviderError::Persistence(
            "provider settings contain duplicate provider ids".into(),
        ));
    }

    if settings.schema_version == LEGACY_SETTINGS_SCHEMA_VERSION {
        for config in &mut settings.providers {
            if let Some(secret) = config
                .bearer_token
                .as_deref()
                .filter(|secret| !secret.is_empty())
            {
                credentials.set(&config.id, secret)?;
                config.has_bearer_token = true;
            }
            config.bearer_token = None;
        }
        settings.schema_version = SETTINGS_SCHEMA_VERSION;
        write_settings_file(path, &settings)?;
    } else if settings
        .providers
        .iter()
        .any(|config| config.bearer_token.is_some())
    {
        return Err(ProviderError::Persistence(
            "schema-2 provider settings may not contain plaintext bearer_token values".into(),
        ));
    }
    Ok(settings)
}

pub fn save_settings(path: &Path, settings: &ProviderSettings) -> Result<(), ProviderError> {
    save_settings_with_store(path, settings, &OsCredentialStore)
}

pub fn save_settings_with_store(
    path: &Path,
    settings: &ProviderSettings,
    credentials: &dyn CredentialStore,
) -> Result<(), ProviderError> {
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
        validate_stored_config(config)?;
    }
    let mut provider_ids = std::collections::HashSet::new();
    if settings
        .providers
        .iter()
        .any(|config| !provider_ids.insert(&config.id))
    {
        return Err(ProviderError::InvalidConfig(
            "provider ids must be unique".into(),
        ));
    }

    let previous = load_settings_with_store(path, credentials)?;
    let mut sanitized = settings.clone();
    let mut originals = Vec::new();
    for config in &mut sanitized.providers {
        if let Some(secret) = config
            .bearer_token
            .as_deref()
            .filter(|secret| !secret.is_empty())
        {
            let prior = match credentials.get(&config.id) {
                Ok(prior) => prior,
                Err(error) => return Err(rollback_error(credentials, &originals, error)),
            };
            if let Err(error) = credentials.set(&config.id, secret) {
                return Err(rollback_error(credentials, &originals, error));
            }
            originals.push((config.id.clone(), prior));
            config.has_bearer_token = true;
        } else if config.has_bearer_token {
            match credentials.get(&config.id) {
                Ok(Some(_)) => {}
                Ok(None) => {
                    return Err(rollback_error(
                        credentials,
                        &originals,
                        ProviderError::Persistence(format!(
                            "provider {} says a token is saved, but the operating-system credential store has no matching entry",
                            config.id
                        )),
                    ));
                }
                Err(error) => return Err(rollback_error(credentials, &originals, error)),
            }
        }
        config.bearer_token = None;
    }

    let removed_ids = previous
        .providers
        .iter()
        .filter(|old| old.has_bearer_token)
        .filter(|old| {
            !sanitized
                .providers
                .iter()
                .any(|new| new.id == old.id && new.has_bearer_token)
        })
        .map(|config| config.id.clone())
        .collect::<Vec<_>>();

    if let Err(error) = write_settings_file(path, &sanitized) {
        return Err(rollback_error(credentials, &originals, error));
    }
    for provider_id in removed_ids {
        let prior = match credentials.get(&provider_id) {
            Ok(prior) => prior,
            Err(error) => {
                let _ = write_settings_file(path, &previous);
                return Err(rollback_error(credentials, &originals, error));
            }
        };
        if let Err(error) = credentials.delete(&provider_id) {
            let _ = write_settings_file(path, &previous);
            return Err(rollback_error(credentials, &originals, error));
        }
        originals.push((provider_id, prior));
    }
    Ok(())
}

pub fn resolve_config(
    path: &Path,
    config: ProviderConfig,
) -> Result<ProviderConfig, ProviderError> {
    resolve_config_with_store(path, config, &OsCredentialStore)
}

pub fn resolve_config_with_store(
    path: &Path,
    mut config: ProviderConfig,
    credentials: &dyn CredentialStore,
) -> Result<ProviderConfig, ProviderError> {
    validate_config(&config)?;
    if config
        .bearer_token
        .as_ref()
        .is_some_and(|secret| !secret.is_empty())
    {
        return Ok(config);
    }

    let settings = load_settings_with_store(path, credentials)?;
    let Some(saved) = settings.providers.iter().find(|saved| {
        saved.id == config.id && saved.kind == config.kind && saved.endpoint == config.endpoint
    }) else {
        return Ok(config);
    };
    if saved.has_bearer_token {
        config.bearer_token = Some(credentials.get(&saved.id)?.ok_or_else(|| {
            ProviderError::Persistence(format!(
                "the saved token for provider {} is missing from the operating-system credential store",
                saved.id
            ))
        })?);
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[derive(Clone, Default)]
    struct MemoryCredentialStore {
        values: Arc<Mutex<HashMap<String, String>>>,
        fail_get: bool,
        fail_set: bool,
        fail_delete: bool,
    }

    impl CredentialStore for MemoryCredentialStore {
        fn get(&self, provider_id: &str) -> Result<Option<String>, ProviderError> {
            if self.fail_get {
                return Err(ProviderError::Persistence("credential read failed".into()));
            }
            Ok(self.values.lock().unwrap().get(provider_id).cloned())
        }

        fn set(&self, provider_id: &str, secret: &str) -> Result<(), ProviderError> {
            if self.fail_set {
                return Err(ProviderError::Persistence("credential write failed".into()));
            }
            self.values
                .lock()
                .unwrap()
                .insert(provider_id.to_owned(), secret.to_owned());
            Ok(())
        }

        fn delete(&self, provider_id: &str) -> Result<(), ProviderError> {
            if self.fail_delete {
                return Err(ProviderError::Persistence(
                    "credential delete failed".into(),
                ));
            }
            self.values.lock().unwrap().remove(provider_id);
            Ok(())
        }
    }

    fn root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "tz-chatter-provider-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

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

    fn stored_config(secret: Option<&str>, has_secret: bool) -> StoredProviderConfig {
        let mut config = StoredProviderConfig::from(&sample_config());
        config.bearer_token = secret.map(str::to_owned);
        config.has_bearer_token = has_secret;
        config
    }

    fn settings(config: StoredProviderConfig) -> ProviderSettings {
        ProviderSettings {
            schema_version: SETTINGS_SCHEMA_VERSION,
            active_provider_id: Some(config.id.clone()),
            providers: vec![config],
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
        assert!(!format!("{config:?}").contains("secret-value"));
        assert!(!format!("{:?}", StoredProviderConfig::from(&config)).contains("secret-value"));
    }

    #[test]
    fn settings_round_trip_uses_credential_store_and_never_serializes_secret() {
        let root = root("round-trip");
        let path = settings_path(&root);
        let credentials = MemoryCredentialStore::default();
        let requested = settings(stored_config(Some("secret-value"), false));
        assert_eq!(
            load_settings_with_store(&path, &credentials).unwrap(),
            ProviderSettings::default()
        );

        save_settings_with_store(&path, &requested, &credentials).unwrap();
        let contents = fs::read_to_string(&path).unwrap();
        assert!(!contents.contains("secret-value"));
        assert!(!contents.contains("\"bearer_token\""));
        let loaded = load_settings_with_store(&path, &credentials).unwrap();
        assert!(loaded.providers[0].has_bearer_token);
        assert_eq!(loaded.providers[0].bearer_token, None);
        assert_eq!(
            credentials.get("local-ollama").unwrap().as_deref(),
            Some("secret-value")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_plaintext_settings_migrate_only_after_secure_write() {
        let root = root("legacy-migration");
        let path = settings_path(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            &path,
            r#"{
  "schema_version": 1,
  "active_provider_id": "local-ollama",
  "providers": [{
    "id": "local-ollama",
    "kind": "ollama",
    "endpoint": "http://127.0.0.1:11434",
    "chat_model": "llama3.2",
    "embedding_model": null,
    "bearer_token": "legacy-secret"
  }]
}"#,
        )
        .unwrap();
        let failing = MemoryCredentialStore {
            fail_set: true,
            ..Default::default()
        };
        assert!(load_settings_with_store(&path, &failing).is_err());
        assert!(fs::read_to_string(&path).unwrap().contains("legacy-secret"));

        let credentials = MemoryCredentialStore::default();
        let loaded = load_settings_with_store(&path, &credentials).unwrap();
        assert_eq!(loaded.schema_version, SETTINGS_SCHEMA_VERSION);
        assert!(loaded.providers[0].has_bearer_token);
        assert_eq!(
            credentials.get("local-ollama").unwrap().as_deref(),
            Some("legacy-secret")
        );
        let migrated = fs::read_to_string(&path).unwrap();
        assert!(!migrated.contains("legacy-secret"));
        assert!(!migrated.contains("\"bearer_token\""));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn multiple_legacy_tokens_with_one_provider_id_are_not_silently_merged() {
        let root = root("duplicate-legacy");
        let path = settings_path(&root);
        fs::create_dir_all(&root).unwrap();
        let value = serde_json::json!({
            "schema_version": LEGACY_SETTINGS_SCHEMA_VERSION,
            "active_provider_id": "local-ollama",
            "providers": [
                {"id": "local-ollama", "kind": "ollama", "endpoint": "http://127.0.0.1:11434", "chat_model": "llama3.2", "bearer_token": "first-secret"},
                {"id": "local-ollama", "kind": "ollama", "endpoint": "http://127.0.0.1:11434", "chat_model": "llama3.2", "bearer_token": "second-secret"},
            ]
        });
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let credentials = MemoryCredentialStore::default();
        assert!(load_settings_with_store(&path, &credentials).is_err());
        assert_eq!(credentials.get("local-ollama").unwrap(), None);
        assert!(fs::read_to_string(&path).unwrap().contains("second-secret"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn saved_token_is_preserved_then_removed_explicitly() {
        let root = root("preserve-remove");
        let path = settings_path(&root);
        let credentials = MemoryCredentialStore::default();
        save_settings_with_store(
            &path,
            &settings(stored_config(Some("secret-value"), false)),
            &credentials,
        )
        .unwrap();

        save_settings_with_store(&path, &settings(stored_config(None, true)), &credentials)
            .unwrap();
        assert!(credentials.get("local-ollama").unwrap().is_some());

        save_settings_with_store(&path, &settings(stored_config(None, false)), &credentials)
            .unwrap();
        assert!(credentials.get("local-ollama").unwrap().is_none());
        assert!(
            !load_settings_with_store(&path, &credentials)
                .unwrap()
                .providers[0]
                .has_bearer_token
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_token_removal_restores_usable_settings_metadata() {
        let root = root("failed-remove");
        let path = settings_path(&root);
        let credentials = MemoryCredentialStore::default();
        save_settings_with_store(
            &path,
            &settings(stored_config(Some("secret-value"), false)),
            &credentials,
        )
        .unwrap();
        let failing = MemoryCredentialStore {
            values: credentials.values.clone(),
            fail_delete: true,
            ..Default::default()
        };

        assert!(
            save_settings_with_store(&path, &settings(stored_config(None, false)), &failing,)
                .is_err()
        );
        assert_eq!(
            credentials.get("local-ollama").unwrap().as_deref(),
            Some("secret-value")
        );
        assert!(
            load_settings_with_store(&path, &credentials)
                .unwrap()
                .providers[0]
                .has_bearer_token
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_later_token_write_restores_earlier_credential() {
        #[derive(Default)]
        struct FailSecondWrite {
            inner: MemoryCredentialStore,
        }

        impl CredentialStore for FailSecondWrite {
            fn get(&self, provider_id: &str) -> Result<Option<String>, ProviderError> {
                self.inner.get(provider_id)
            }

            fn set(&self, provider_id: &str, secret: &str) -> Result<(), ProviderError> {
                if provider_id == "second-provider" {
                    return Err(ProviderError::Persistence(
                        "second token write failed".into(),
                    ));
                }
                self.inner.set(provider_id, secret)
            }

            fn delete(&self, provider_id: &str) -> Result<(), ProviderError> {
                self.inner.delete(provider_id)
            }
        }

        let root = root("failed-later-write");
        let path = settings_path(&root);
        let credentials = FailSecondWrite::default();
        credentials
            .inner
            .set("local-ollama", "original-secret")
            .unwrap();
        let mut requested = settings(stored_config(Some("replacement-secret"), false));
        let mut second = stored_config(Some("second-secret"), false);
        second.id = "second-provider".into();
        requested.providers.push(second);

        assert!(save_settings_with_store(&path, &requested, &credentials).is_err());
        assert_eq!(
            credentials.inner.get("local-ollama").unwrap().as_deref(),
            Some("original-secret")
        );
        assert!(!path.exists());
        assert!(!root.exists());
    }

    #[test]
    fn resolution_uses_saved_token_only_for_the_saved_endpoint() {
        let root = root("resolve");
        let path = settings_path(&root);
        let credentials = MemoryCredentialStore::default();
        save_settings_with_store(
            &path,
            &settings(stored_config(Some("secret-value"), false)),
            &credentials,
        )
        .unwrap();

        let mut request = sample_config();
        request.bearer_token = None;
        let resolved = resolve_config_with_store(&path, request.clone(), &credentials).unwrap();
        assert_eq!(resolved.bearer_token.as_deref(), Some("secret-value"));

        request.endpoint = "https://example.invalid/v1".into();
        let changed_endpoint = resolve_config_with_store(&path, request, &credentials).unwrap();
        assert_eq!(changed_endpoint.bearer_token, None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_saved_credential_is_an_actionable_error() {
        let root = root("missing-secret");
        let path = settings_path(&root);
        let credentials = MemoryCredentialStore::default();
        write_settings_file(&path, &settings(stored_config(None, true))).unwrap();
        let mut request = sample_config();
        request.bearer_token = None;
        let error = resolve_config_with_store(&path, request, &credentials).unwrap_err();
        assert!(error
            .to_string()
            .contains("missing from the operating-system credential store"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "writes a short-lived test entry to the current user's OS credential store"]
    fn live_os_credential_store_round_trip() {
        let provider_id = format!("credential-smoke-{}", uuid::Uuid::new_v4());
        let credentials = OsCredentialStore;
        let result = (|| {
            credentials.set(&provider_id, "tz-chatter-smoke-secret")?;
            if credentials.get(&provider_id)?.as_deref() != Some("tz-chatter-smoke-secret") {
                return Err(ProviderError::Persistence(
                    "OS credential store returned a different smoke-test value".into(),
                ));
            }
            Ok(())
        })();
        let cleanup = credentials.delete(&provider_id);
        result.unwrap();
        cleanup.unwrap();
        assert_eq!(credentials.get(&provider_id).unwrap(), None);
    }

    #[test]
    fn rejects_unknown_active_provider() {
        let root = root("invalid-active");
        let path = settings_path(&root);
        let credentials = MemoryCredentialStore::default();
        let settings = ProviderSettings {
            schema_version: SETTINGS_SCHEMA_VERSION,
            active_provider_id: Some("missing".into()),
            providers: Vec::new(),
        };
        assert!(matches!(
            save_settings_with_store(&path, &settings, &credentials),
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
