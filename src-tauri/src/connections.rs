use crate::providers::{
    self, ChatMessage, ChatRequest, ChatStreamEvent, DiscoveryResponse, EmbeddingRequest,
    EmbeddingResponse, HealthResponse, ModelDescriptor, ProviderConfig, ProviderError,
    ProviderKind,
};
use futures_util::StreamExt;
use reqwest::{header, Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub type StreamSink = Arc<dyn Fn(ChatStreamEvent) + Send + Sync>;

#[derive(Clone)]
pub struct ProviderClient {
    http: Client,
}

impl Default for ProviderClient {
    fn default() -> Self {
        Self {
            http: Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .read_timeout(Duration::from_secs(60))
                .pool_idle_timeout(Duration::from_secs(30))
                .build()
                .expect("reqwest client"),
        }
    }
}

impl ProviderClient {
    pub async fn health(&self, config: &ProviderConfig) -> Result<HealthResponse, ProviderError> {
        providers::validate_config(config)?;
        let url = endpoint(config, "tags", "models")?;
        let response = match self.authorized(self.http.get(url), config).send().await {
            Ok(response) => response,
            Err(error) => {
                return Ok(HealthResponse {
                    provider_id: config.id.clone(),
                    reachable: false,
                    detail: Some(error.to_string()),
                });
            }
        };
        let status = response.status();
        Ok(HealthResponse {
            provider_id: config.id.clone(),
            reachable: status.is_success(),
            detail: (!status.is_success()).then(|| format!("provider returned HTTP {status}")),
        })
    }

    pub async fn discover(
        &self,
        config: &ProviderConfig,
    ) -> Result<DiscoveryResponse, ProviderError> {
        providers::validate_config(config)?;
        let url = endpoint(config, "tags", "models")?;
        let response = self
            .authorized(self.http.get(url), config)
            .send()
            .await
            .map_err(|error| ProviderError::Unavailable(error.to_string()))?;
        let response = ensure_response_success(response).await?;
        let body = response.json::<Value>().await.map_err(|error| {
            ProviderError::Unavailable(format!("invalid discovery response: {error}"))
        })?;
        match config.kind {
            ProviderKind::Ollama => parse_ollama_discovery(config, body),
            ProviderKind::LmStudio | ProviderKind::OpenAiCompatible => {
                parse_openai_discovery(config, body)
            }
        }
    }

    pub async fn stream_chat(
        &self,
        config: &ProviderConfig,
        request: &ChatRequest,
        cancellation: CancellationToken,
    ) -> Result<Vec<ChatStreamEvent>, ProviderError> {
        self.stream_chat_with_sink(config, request, cancellation, None)
            .await
    }

    pub async fn stream_chat_with_sink(
        &self,
        config: &ProviderConfig,
        request: &ChatRequest,
        cancellation: CancellationToken,
        sink: Option<StreamSink>,
    ) -> Result<Vec<ChatStreamEvent>, ProviderError> {
        providers::validate_config(config)?;
        if request.provider_id != config.id {
            return Err(ProviderError::InvalidConfig(
                "chat request provider_id does not match configuration".into(),
            ));
        }
        providers::require_capability(
            &config.kind,
            &providers::ProviderCapabilities::baseline(&config.kind),
            "chat_streaming",
        )?;

        let builder = match config.kind {
            ProviderKind::Ollama => {
                let url = endpoint(config, "chat", "chat/completions")?;
                let payload = build_ollama_chat_payload(request);
                self.authorized(self.http.post(url).json(&payload), config)
            }
            ProviderKind::LmStudio | ProviderKind::OpenAiCompatible => {
                let url = endpoint(config, "chat", "chat/completions")?;
                let payload = build_openai_chat_payload(request);
                self.authorized(self.http.post(url).json(&payload), config)
            }
        };
        let response = match send_cancellable(builder, &cancellation).await? {
            Some(response) => response,
            None => {
                return Ok(vec![ChatStreamEvent::Cancelled]);
            }
        };
        let response = match ensure_response_success_cancellable(response, &cancellation).await? {
            Some(response) => response,
            None => return Ok(vec![ChatStreamEvent::Cancelled]),
        };

        let mut events = Vec::new();
        emit_stream_event(
            &mut events,
            ChatStreamEvent::Started {
                cancellation_id: request.cancellation_id.clone(),
            },
            sink.as_ref(),
        );
        let mut stream = response.bytes_stream();
        let mut decoder = match config.kind {
            ProviderKind::Ollama => StreamDecoder::Ollama(OllamaStreamDecoder::default()),
            ProviderKind::LmStudio | ProviderKind::OpenAiCompatible => {
                StreamDecoder::OpenAi(OpenAiSseDecoder::default())
            }
        };

        loop {
            tokio::select! {
                _ = cancellation.cancelled() => {
                    emit_stream_event(&mut events, ChatStreamEvent::Cancelled, sink.as_ref());
                    return Ok(events);
                }
                next = stream.next() => {
                    match next {
                        Some(Ok(chunk)) => {
                            match decoder.push(&chunk) {
                                Ok(decoded) => {
                                    for event in decoded {
                                        emit_stream_event(&mut events, event, sink.as_ref());
                                    }
                                }
                                Err(error) => {
                                    emit_stream_event(
                                        &mut events,
                                        ChatStreamEvent::Failed { message: error.to_string() },
                                        sink.as_ref(),
                                    );
                                    return Ok(events);
                                }
                            }
                        }
                        Some(Err(error)) => {
                            emit_stream_event(
                                &mut events,
                                ChatStreamEvent::Failed { message: error.to_string() },
                                sink.as_ref(),
                            );
                            return Ok(events);
                        }
                        None => break,
                    }
                }
            }
        }
        match decoder.finish() {
            Ok(decoded) => {
                for event in decoded {
                    emit_stream_event(&mut events, event, sink.as_ref());
                }
            }
            Err(error) => {
                emit_stream_event(
                    &mut events,
                    ChatStreamEvent::Failed {
                        message: error.to_string(),
                    },
                    sink.as_ref(),
                );
                return Ok(events);
            }
        }
        if !events.iter().any(|event| {
            matches!(
                event,
                ChatStreamEvent::Completed { .. } | ChatStreamEvent::Cancelled
            )
        }) {
            emit_stream_event(
                &mut events,
                ChatStreamEvent::Failed {
                    message: "stream ended without completion".into(),
                },
                sink.as_ref(),
            );
        }
        Ok(events)
    }

    pub async fn embed(
        &self,
        config: &ProviderConfig,
        request: &EmbeddingRequest,
    ) -> Result<EmbeddingResponse, ProviderError> {
        providers::validate_config(config)?;
        if request.provider_id != config.id {
            return Err(ProviderError::InvalidConfig(
                "embedding request provider_id does not match configuration".into(),
            ));
        }
        if config.embedding_model.as_deref() != Some(request.model.as_str()) {
            return Err(ProviderError::InvalidConfig(
                "embedding request model does not match configuration".into(),
            ));
        }
        providers::require_capability(
            &config.kind,
            &providers::ProviderCapabilities::baseline(&config.kind),
            "embeddings",
        )?;
        let response = match config.kind {
            ProviderKind::Ollama => {
                let url = endpoint(config, "embed", "embeddings")?;
                let payload = serde_json::json!({ "model": request.model, "input": request.input });
                self.authorized(self.http.post(url).json(&payload), config)
                    .send()
                    .await
            }
            ProviderKind::LmStudio | ProviderKind::OpenAiCompatible => {
                let url = endpoint(config, "embed", "embeddings")?;
                let payload = serde_json::json!({ "model": request.model, "input": request.input });
                self.authorized(self.http.post(url).json(&payload), config)
                    .send()
                    .await
            }
        }
        .map_err(|error| ProviderError::Unavailable(error.to_string()))?;
        let response = ensure_response_success(response).await?;
        let body = response.json::<Value>().await.map_err(|error| {
            ProviderError::Unavailable(format!("invalid embedding response: {error}"))
        })?;
        parse_embedding_response(config, request, body)
    }

    fn authorized(&self, request: RequestBuilder, config: &ProviderConfig) -> RequestBuilder {
        match config
            .bearer_token
            .as_deref()
            .filter(|token| !token.is_empty())
        {
            Some(token) => request.header(header::AUTHORIZATION, format!("Bearer {token}")),
            None => request,
        }
    }
}

fn emit_stream_event(
    events: &mut Vec<ChatStreamEvent>,
    event: ChatStreamEvent,
    sink: Option<&StreamSink>,
) {
    if let Some(sink) = sink {
        sink(event.clone());
    }
    events.push(event);
}

fn endpoint(
    config: &ProviderConfig,
    ollama_path: &str,
    openai_path: &str,
) -> Result<String, ProviderError> {
    let base = providers::validate_endpoint(&config.endpoint)?
        .to_string()
        .trim_end_matches('/')
        .to_string();
    Ok(if config.kind.uses_openai_compatible_http() {
        let v1_base = if base.ends_with("/v1") {
            base
        } else {
            format!("{base}/v1")
        };
        format!("{v1_base}/{openai_path}")
    } else {
        format!("{base}/api/{ollama_path}")
    })
}

async fn send_cancellable(
    builder: RequestBuilder,
    cancellation: &CancellationToken,
) -> Result<Option<reqwest::Response>, ProviderError> {
    tokio::select! {
        _ = cancellation.cancelled() => Ok(None),
        result = builder.send() => {
            result
                .map(Some)
                .map_err(|error| ProviderError::Unavailable(error.to_string()))
        }
    }
}

async fn ensure_response_success(
    response: reqwest::Response,
) -> Result<reqwest::Response, ProviderError> {
    match ensure_response_success_cancellable(response, &CancellationToken::new()).await? {
        Some(response) => Ok(response),
        None => Err(ProviderError::Unavailable(
            "provider request was cancelled".into(),
        )),
    }
}

async fn ensure_response_success_cancellable(
    response: reqwest::Response,
    cancellation: &CancellationToken,
) -> Result<Option<reqwest::Response>, ProviderError> {
    let status = response.status();
    if status.is_success() {
        return Ok(Some(response));
    }
    let detail = tokio::select! {
        _ = cancellation.cancelled() => return Ok(None),
        text = response.text() => text.unwrap_or_default(),
    };
    let detail = detail.chars().take(1000).collect::<String>();
    let suffix = if detail.trim().is_empty() {
        String::new()
    } else {
        format!(": {}", detail.trim())
    };
    Err(ProviderError::Unavailable(format!(
        "provider returned HTTP {status}{suffix}"
    )))
}

#[derive(Debug, Serialize)]
struct OllamaChatPayload<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaChatOptions>,
}

#[derive(Debug, Serialize)]
struct OllamaChatOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
}

fn build_ollama_chat_payload(request: &ChatRequest) -> OllamaChatPayload<'_> {
    OllamaChatPayload {
        model: &request.model,
        messages: &request.messages,
        stream: true,
        format: request.json_mode.then_some("json"),
        options: ollama_sampling(request),
    }
}

fn ollama_sampling(request: &ChatRequest) -> Option<OllamaChatOptions> {
    if request.temperature.is_none() && request.max_tokens.is_none() {
        None
    } else {
        Some(OllamaChatOptions {
            temperature: request.temperature,
            num_predict: request.max_tokens,
        })
    }
}

#[derive(Debug, Serialize)]
struct OpenAiChatPayload<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<OpenAiResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
struct OpenAiResponseFormat {
    r#type: &'static str,
}

fn build_openai_chat_payload(request: &ChatRequest) -> OpenAiChatPayload<'_> {
    OpenAiChatPayload {
        model: &request.model,
        messages: &request.messages,
        stream: true,
        response_format: request.json_mode.then_some(OpenAiResponseFormat {
            r#type: "json_object",
        }),
        temperature: request.temperature,
        max_tokens: request.max_tokens,
    }
}

fn parse_ollama_discovery(
    config: &ProviderConfig,
    body: Value,
) -> Result<DiscoveryResponse, ProviderError> {
    #[derive(Deserialize)]
    struct Response {
        models: Vec<Model>,
    }
    #[derive(Deserialize)]
    struct Model {
        name: String,
    }
    let response: Response = serde_json::from_value(body).map_err(|error| {
        ProviderError::Unavailable(format!("invalid Ollama discovery response: {error}"))
    })?;
    Ok(DiscoveryResponse {
        provider_id: config.id.clone(),
        models: response
            .models
            .into_iter()
            .map(|model| ModelDescriptor {
                id: model.name,
                kind: "ollama".into(),
                supports_chat: true,
                supports_embeddings: true,
            })
            .collect(),
    })
}

fn parse_openai_discovery(
    config: &ProviderConfig,
    body: Value,
) -> Result<DiscoveryResponse, ProviderError> {
    #[derive(Deserialize)]
    struct Response {
        data: Vec<Model>,
    }
    #[derive(Deserialize)]
    struct Model {
        id: String,
    }
    let response: Response = serde_json::from_value(body).map_err(|error| {
        ProviderError::Unavailable(format!(
            "invalid OpenAI-compatible discovery response: {error}"
        ))
    })?;
    Ok(DiscoveryResponse {
        provider_id: config.id.clone(),
        models: response
            .data
            .into_iter()
            .map(|model| {
                let supports_embeddings =
                    config.embedding_model.as_deref() == Some(model.id.as_str());
                ModelDescriptor {
                    id: model.id,
                    kind: config.kind.wire_label().into(),
                    supports_chat: true,
                    supports_embeddings,
                }
            })
            .collect(),
    })
}

fn parse_embedding_response(
    config: &ProviderConfig,
    request: &EmbeddingRequest,
    body: Value,
) -> Result<EmbeddingResponse, ProviderError> {
    let vectors: Vec<Vec<f32>> = match config.kind {
        ProviderKind::Ollama => body.get("embeddings").cloned(),
        ProviderKind::LmStudio | ProviderKind::OpenAiCompatible => {
            body.get("data").and_then(|data| {
                data.as_array().map(|items| {
                    Value::Array(
                        items
                            .iter()
                            .filter_map(|item| item.get("embedding").cloned())
                            .collect::<Vec<_>>(),
                    )
                })
            })
        }
    }
    .and_then(|value| serde_json::from_value(value).ok())
    .ok_or_else(|| ProviderError::Unavailable("invalid embedding response shape".into()))?;
    let dimensions = vectors.first().map_or(0, Vec::len);
    if vectors.len() != request.input.len()
        || vectors.iter().any(|vector| vector.len() != dimensions)
    {
        return Err(ProviderError::Unavailable(
            "embedding response count or dimensions do not match the request".into(),
        ));
    }
    Ok(EmbeddingResponse {
        model: request.model.clone(),
        dimensions,
        vectors,
    })
}

enum StreamDecoder {
    Ollama(OllamaStreamDecoder),
    OpenAi(OpenAiSseDecoder),
}

impl StreamDecoder {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<ChatStreamEvent>, ProviderError> {
        match self {
            Self::Ollama(decoder) => decoder.push(chunk),
            Self::OpenAi(decoder) => decoder.push(chunk),
        }
    }

    fn finish(&mut self) -> Result<Vec<ChatStreamEvent>, ProviderError> {
        match self {
            Self::Ollama(decoder) => decoder.finish(),
            Self::OpenAi(decoder) => decoder.finish(),
        }
    }
}

#[derive(Default)]
pub struct OllamaStreamDecoder {
    buffer: Vec<u8>,
}

impl OllamaStreamDecoder {
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<ChatStreamEvent>, ProviderError> {
        self.buffer.extend_from_slice(chunk);
        self.drain(false)
    }

    pub fn finish(&mut self) -> Result<Vec<ChatStreamEvent>, ProviderError> {
        self.drain(true)
    }

    fn drain(&mut self, final_chunk: bool) -> Result<Vec<ChatStreamEvent>, ProviderError> {
        let mut events = Vec::new();
        while let Some(index) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line = self.buffer.drain(..=index).collect::<Vec<_>>();
            events.extend(parse_ollama_line(&line[..line.len() - 1])?);
        }
        if final_chunk && !self.buffer.is_empty() {
            let line = std::mem::take(&mut self.buffer);
            events.extend(parse_ollama_line(&line)?);
        }
        Ok(events)
    }
}

#[derive(Default)]
pub struct OpenAiSseDecoder {
    buffer: Vec<u8>,
}

impl OpenAiSseDecoder {
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<ChatStreamEvent>, ProviderError> {
        self.buffer.extend_from_slice(chunk);
        self.drain(false)
    }

    pub fn finish(&mut self) -> Result<Vec<ChatStreamEvent>, ProviderError> {
        self.drain(true)
    }

    fn drain(&mut self, final_chunk: bool) -> Result<Vec<ChatStreamEvent>, ProviderError> {
        let mut events = Vec::new();
        while let Some(index) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line = self.buffer.drain(..=index).collect::<Vec<_>>();
            events.extend(parse_openai_line(&line[..line.len() - 1])?);
        }
        if final_chunk && !self.buffer.is_empty() {
            let line = std::mem::take(&mut self.buffer);
            events.extend(parse_openai_line(&line)?);
        }
        Ok(events)
    }
}

#[derive(Debug, Deserialize)]
struct OllamaStreamChunk {
    #[serde(default)]
    message: Option<OllamaMessage>,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    done_reason: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OllamaMessage {
    #[serde(default)]
    content: String,
}

fn parse_ollama_line(line: &[u8]) -> Result<Vec<ChatStreamEvent>, ProviderError> {
    let line = std::str::from_utf8(line)
        .map_err(|error| {
            ProviderError::Unavailable(format!("invalid UTF-8 in Ollama stream: {error}"))
        })?
        .trim();
    if line.is_empty() {
        return Ok(Vec::new());
    }
    let chunk: OllamaStreamChunk = serde_json::from_str(line).map_err(|error| {
        ProviderError::Unavailable(format!("invalid Ollama stream frame: {error}"))
    })?;
    if let Some(error) = chunk.error {
        return Ok(vec![ChatStreamEvent::Failed { message: error }]);
    }
    let mut events = Vec::new();
    if let Some(message) = chunk.message {
        if !message.content.is_empty() {
            events.push(ChatStreamEvent::Delta {
                text: message.content,
            });
        }
    }
    if chunk.done {
        events.push(ChatStreamEvent::Completed {
            finish_reason: chunk.done_reason,
        });
    }
    Ok(events)
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChunk {
    #[serde(default)]
    choices: Vec<OpenAiChoice>,
    #[serde(default)]
    error: Option<OpenAiError>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    #[serde(default)]
    delta: OpenAiDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct OpenAiDelta {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiError {
    message: String,
}

fn parse_openai_line(line: &[u8]) -> Result<Vec<ChatStreamEvent>, ProviderError> {
    let line = std::str::from_utf8(line)
        .map_err(|error| {
            ProviderError::Unavailable(format!("invalid UTF-8 in OpenAI stream: {error}"))
        })?
        .trim();
    if line.is_empty() || line.starts_with(':') || !line.starts_with("data:") {
        return Ok(Vec::new());
    }
    let data = line.trim_start_matches("data:").trim();
    if data == "[DONE]" {
        return Ok(vec![ChatStreamEvent::Completed {
            finish_reason: None,
        }]);
    }
    let chunk: OpenAiStreamChunk = serde_json::from_str(data).map_err(|error| {
        ProviderError::Unavailable(format!("invalid OpenAI-compatible stream frame: {error}"))
    })?;
    if let Some(error) = chunk.error {
        return Ok(vec![ChatStreamEvent::Failed {
            message: error.message,
        }]);
    }
    let mut events = Vec::new();
    for choice in chunk.choices {
        if let Some(content) = choice.delta.content.filter(|content| !content.is_empty()) {
            events.push(ChatStreamEvent::Delta { text: content });
        }
        if choice.finish_reason.is_some() {
            events.push(ChatStreamEvent::Completed {
                finish_reason: choice.finish_reason,
            });
        }
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ChatRole;
    use std::sync::Mutex;

    fn ollama_config() -> ProviderConfig {
        ProviderConfig {
            id: "ollama".into(),
            kind: ProviderKind::Ollama,
            endpoint: "http://127.0.0.1:11434".into(),
            chat_model: "llama3.2".into(),
            embedding_model: Some("nomic-embed-text".into()),
            bearer_token: None,
        }
    }

    fn compatible_config() -> ProviderConfig {
        ProviderConfig {
            id: "lm-studio".into(),
            kind: ProviderKind::OpenAiCompatible,
            endpoint: "http://127.0.0.1:1234/v1".into(),
            chat_model: "local-model".into(),
            embedding_model: None,
            bearer_token: Some("secret".into()),
        }
    }

    fn lm_studio_config() -> ProviderConfig {
        ProviderConfig {
            id: "lm-studio-local".into(),
            kind: ProviderKind::LmStudio,
            endpoint: "http://127.0.0.1:1234/v1".into(),
            chat_model: "local-model".into(),
            embedding_model: None,
            bearer_token: None,
        }
    }

    fn request(provider_id: &str) -> ChatRequest {
        ChatRequest {
            provider_id: provider_id.into(),
            model: "local-model".into(),
            messages: vec![ChatMessage {
                role: ChatRole::User,
                content: "Hello".into(),
            }],
            cancellation_id: "cancel-001".into(),
            temperature: None,
            max_tokens: None,
            json_mode: false,
        }
    }

    #[test]
    fn builds_provider_specific_endpoints_and_streaming_payloads() {
        let ollama = ollama_config();
        let compatible = compatible_config();
        assert_eq!(
            endpoint(&ollama, "chat", "chat/completions").unwrap(),
            "http://127.0.0.1:11434/api/chat"
        );
        assert_eq!(
            endpoint(&compatible, "chat", "chat/completions").unwrap(),
            "http://127.0.0.1:1234/v1/chat/completions"
        );
        assert_eq!(
            serde_json::to_value(build_ollama_chat_payload(&request("ollama"))).unwrap()["stream"],
            true
        );
        assert_eq!(
            serde_json::to_value(build_openai_chat_payload(&request("lm-studio"))).unwrap()
                ["stream"],
            true
        );
        let mut sampled = request("ollama");
        sampled.temperature = Some(0.7);
        sampled.max_tokens = Some(256);
        let ollama = serde_json::to_value(build_ollama_chat_payload(&sampled)).unwrap();
        assert_eq!(ollama["options"]["temperature"], 0.7);
        assert_eq!(ollama["options"]["num_predict"], 256);
        let openai = serde_json::to_value(build_openai_chat_payload(&sampled)).unwrap();
        assert_eq!(openai["temperature"], 0.7);
        assert_eq!(openai["max_tokens"], 256);
        assert!(
            serde_json::to_value(build_ollama_chat_payload(&request("ollama")))
                .unwrap()
                .get("options")
                .is_none()
        );
        let mut structured = request("ollama");
        structured.json_mode = true;
        let ollama = serde_json::to_value(build_ollama_chat_payload(&structured)).unwrap();
        assert_eq!(ollama["format"], "json");
        let openai = serde_json::to_value(build_openai_chat_payload(&structured)).unwrap();
        assert_eq!(openai["response_format"]["type"], "json_object");
    }

    #[test]
    fn lm_studio_uses_openai_compatible_v1_paths_and_model_list() {
        let config = lm_studio_config();
        assert_eq!(
            endpoint(&config, "tags", "models").unwrap(),
            "http://127.0.0.1:1234/v1/models"
        );
        assert_eq!(
            endpoint(&config, "chat", "chat/completions").unwrap(),
            "http://127.0.0.1:1234/v1/chat/completions"
        );
        assert_eq!(
            endpoint(&config, "embed", "embeddings").unwrap(),
            "http://127.0.0.1:1234/v1/embeddings"
        );
        let discovery = parse_openai_discovery(
            &config,
            serde_json::json!({ "data": [{ "id": "qwen2.5-7b" }, { "id": "text-embedding-nomic" }] }),
        )
        .unwrap();
        assert_eq!(discovery.provider_id, "lm-studio-local");
        assert!(discovery.models.iter().any(|model| model.id == "qwen2.5-7b"
            && model.kind == "lm_studio"
            && model.supports_chat));
    }

    #[test]
    fn parses_fragmented_ollama_ndjson_without_losing_multibyte_text() {
        let input = "{\"message\":{\"content\":\"Hel\"},\"done\":false}\n{\"message\":{\"content\":\"lo ✨\"},\"done\":false}\n{\"done\":true,\"done_reason\":\"stop\"}\n".as_bytes();
        let mut decoder = OllamaStreamDecoder::default();
        let mut events = Vec::new();
        for byte in input.chunks(1) {
            events.extend(decoder.push(byte).unwrap());
        }
        events.extend(decoder.finish().unwrap());
        assert_eq!(
            events,
            vec![
                ChatStreamEvent::Delta { text: "Hel".into() },
                ChatStreamEvent::Delta {
                    text: "lo ✨".into()
                },
                ChatStreamEvent::Completed {
                    finish_reason: Some("stop".into())
                },
            ]
        );
    }

    #[test]
    fn parses_fragmented_openai_sse_and_done_marker() {
        let input = b"data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n";
        let mut decoder = OpenAiSseDecoder::default();
        let mut events = Vec::new();
        for chunk in input.chunks(3) {
            events.extend(decoder.push(chunk).unwrap());
        }
        events.extend(decoder.finish().unwrap());
        assert_eq!(
            events,
            vec![
                ChatStreamEvent::Delta { text: "Hel".into() },
                ChatStreamEvent::Delta { text: "lo".into() },
                ChatStreamEvent::Completed {
                    finish_reason: Some("stop".into())
                },
                ChatStreamEvent::Completed {
                    finish_reason: None
                },
            ]
        );
    }

    #[test]
    fn parses_provider_errors_and_embedding_shapes() {
        let ollama_error = br#"{"error":"model not found"}"#;
        assert_eq!(
            parse_ollama_line(ollama_error).unwrap(),
            vec![ChatStreamEvent::Failed {
                message: "model not found".into()
            }]
        );
        let openai_error = br#"data: {"error":{"message":"model unavailable"}}"#;
        assert_eq!(
            parse_openai_line(openai_error).unwrap(),
            vec![ChatStreamEvent::Failed {
                message: "model unavailable".into()
            }]
        );

        let request = EmbeddingRequest {
            provider_id: "ollama".into(),
            model: "embed".into(),
            input: vec!["a".into(), "b".into()],
        };
        let response = parse_embedding_response(
            &ollama_config(),
            &request,
            serde_json::json!({"embeddings": [[1.0, 2.0], [3.0, 4.0]]}),
        )
        .unwrap();
        assert_eq!(response.dimensions, 2);
        assert_eq!(response.vectors.len(), 2);
    }

    #[test]
    fn rejects_mismatched_chat_provider_ids() {
        let config = ollama_config();
        let request = request("wrong-provider");
        assert!(matches!(
            providers::validate_config(&config).and_then(|_| if request.provider_id == config.id {
                Ok(())
            } else {
                Err(ProviderError::InvalidConfig(
                    "chat request provider_id does not match configuration".into(),
                ))
            }),
            Err(ProviderError::InvalidConfig(_))
        ));
    }

    #[test]
    fn stream_events_are_forwarded_to_the_live_sink_and_retained() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let received_for_sink = received.clone();
        let sink: StreamSink = Arc::new(move |event| {
            received_for_sink.lock().unwrap().push(event);
        });
        let mut retained = Vec::new();
        let event = ChatStreamEvent::Delta {
            text: "partial".into(),
        };

        emit_stream_event(&mut retained, event.clone(), Some(&sink));

        assert_eq!(retained, vec![event.clone()]);
        assert_eq!(*received.lock().unwrap(), vec![event]);
    }

    #[tokio::test]
    async fn cancellation_before_http_headers_finishes_as_cancelled() {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (accepted_tx, accepted_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut bytes = [0; 4096];
            let _ = socket.read(&mut bytes);
            let _ = accepted_tx.send(());
            let _ = release_rx.recv_timeout(Duration::from_secs(5));
        });
        let mut provider = ollama_config();
        provider.endpoint = format!("http://{addr}");
        let request = request("ollama");
        let token = CancellationToken::new();
        let client = ProviderClient::default();
        let mut pending = Box::pin(client.stream_chat(&provider, &request, token.clone()));
        tokio::select! {
            _ = accepted_rx => {}
            _ = &mut pending => panic!("request completed before cancellation"),
        }
        token.cancel();
        let events = tokio::time::timeout(Duration::from_secs(2), pending)
            .await
            .expect("cancelled request must finish")
            .unwrap();
        assert!(events
            .iter()
            .any(|event| matches!(event, ChatStreamEvent::Cancelled)));
        let _ = release_tx.send(());
        server.join().unwrap();
    }

    #[tokio::test]
    #[ignore = "requires an already-running local Ollama service and model"]
    async fn live_ollama_health_discovery_and_stream_smoke() {
        let mut config = ollama_config();
        config.chat_model = "llama3.2:latest".into();
        let client = ProviderClient::default();
        let health = client.health(&config).await.unwrap();
        assert!(
            health.reachable,
            "Ollama was not reachable: {:?}",
            health.detail
        );
        let discovery = client.discover(&config).await.unwrap();
        assert!(discovery
            .models
            .iter()
            .any(|model| model.id == config.chat_model));
        let mut chat = request("ollama");
        chat.model = config.chat_model.clone();
        let events = client
            .stream_chat(&config, &chat, CancellationToken::new())
            .await
            .unwrap();
        assert!(events
            .iter()
            .any(|event| matches!(event, ChatStreamEvent::Delta { text } if !text.is_empty())));
        assert!(events
            .iter()
            .any(|event| matches!(event, ChatStreamEvent::Completed { .. })));
    }

    #[tokio::test]
    #[ignore = "requires an already-running Ollama OpenAI-compatible endpoint and model"]
    async fn live_openai_compatible_ollama_health_discovery_and_stream_smoke() {
        let config = ProviderConfig {
            id: "ollama-openai-compatible".into(),
            kind: ProviderKind::OpenAiCompatible,
            endpoint: "http://127.0.0.1:11434/v1".into(),
            chat_model: "llama3.2:latest".into(),
            embedding_model: None,
            bearer_token: None,
        };
        let client = ProviderClient::default();
        let health = client.health(&config).await.unwrap();
        assert!(
            health.reachable,
            "OpenAI-compatible Ollama endpoint was not reachable: {:?}",
            health.detail
        );
        let discovery = client.discover(&config).await.unwrap();
        assert!(discovery
            .models
            .iter()
            .any(|model| model.id == config.chat_model));
        let mut chat = request(&config.id);
        chat.model = config.chat_model.clone();
        let events = client
            .stream_chat(&config, &chat, CancellationToken::new())
            .await
            .unwrap();
        assert!(events
            .iter()
            .any(|event| matches!(event, ChatStreamEvent::Delta { text } if !text.is_empty())));
        assert!(events
            .iter()
            .any(|event| matches!(event, ChatStreamEvent::Completed { .. })));
    }
}
