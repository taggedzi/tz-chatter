use crate::{
    connections::{ProviderClient, StreamSink},
    embeddings::{chunks_from_memories, EmbeddingIndex, EmbeddingSpace},
    extraction::ExtractionQueue,
    initiative::{
        classify_initiative_response, initiative_is_stale, render_initiative_event,
        select_open_topics, InitiativeModelChoice, InitiativeRequest,
    },
    memory::MemoryStore,
    prompt::{build_prompt_with_memories, PromptBudget},
    providers::{ChatRequest, ChatStreamEvent, EmbeddingRequest, ProviderConfig, ProviderError},
    retrieval::{
        hybrid_retrieve, retrieve, retrieve_with_report, RetrievalBudget, RetrievalReport,
    },
    storage::{
        CharacterDefinition, OperationalState, StorageError, TranscriptDocument, TranscriptTurn,
        TurnRole, TurnStatus, Vault,
    },
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;

#[async_trait]
pub trait ChatTransport: Send + Sync {
    async fn stream_chat(
        &self,
        config: &ProviderConfig,
        request: &ChatRequest,
        cancellation: CancellationToken,
    ) -> Result<Vec<ChatStreamEvent>, ProviderError>;

    async fn stream_chat_with_sink(
        &self,
        config: &ProviderConfig,
        request: &ChatRequest,
        cancellation: CancellationToken,
        sink: Option<StreamSink>,
    ) -> Result<Vec<ChatStreamEvent>, ProviderError> {
        let events = self.stream_chat(config, request, cancellation).await?;
        if let Some(sink) = sink {
            for event in &events {
                sink(event.clone());
            }
        }
        Ok(events)
    }
}

#[async_trait]
impl ChatTransport for ProviderClient {
    async fn stream_chat(
        &self,
        config: &ProviderConfig,
        request: &ChatRequest,
        cancellation: CancellationToken,
    ) -> Result<Vec<ChatStreamEvent>, ProviderError> {
        ProviderClient::stream_chat(self, config, request, cancellation).await
    }

    async fn stream_chat_with_sink(
        &self,
        config: &ProviderConfig,
        request: &ChatRequest,
        cancellation: CancellationToken,
        sink: Option<StreamSink>,
    ) -> Result<Vec<ChatStreamEvent>, ProviderError> {
        ProviderClient::stream_chat_with_sink(self, config, request, cancellation, sink).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestSnapshot {
    pub character: CharacterDefinition,
    pub provider: ProviderConfig,
    pub session_id: String,
    pub user_turn_id: String,
    pub user_content: String,
    #[serde(default)]
    pub use_hybrid_retrieval: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversationOutcome {
    pub transcript: TranscriptDocument,
    pub assistant_turn: TranscriptTurn,
    pub retrieved_memories: Vec<crate::retrieval::RetrievedMemory>,
    pub context_inspection: ContextInspection,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ContextInspection {
    pub retrieval_mode: String,
    pub fallback_reason: Option<String>,
    pub estimated_input_tokens: usize,
    pub input_token_limit: usize,
    pub reserved_output_tokens: usize,
    pub omitted_turns: usize,
    pub selected_memory_tokens: usize,
    pub candidate_memories: usize,
    pub omitted_memories: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversationResume {
    pub character: CharacterDefinition,
    pub transcript: Option<TranscriptDocument>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InitiativeOutcome {
    pub transcript: TranscriptDocument,
    pub initiative_turn: Option<TranscriptTurn>,
    pub assistant_turn: Option<TranscriptTurn>,
    pub retrieved_memories: Vec<crate::retrieval::RetrievedMemory>,
    pub choice: InitiativeModelChoice,
    pub status: InitiativeDeliveryStatus,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum InitiativeDeliveryStatus {
    Delivered,
    Silenced,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationError {
    Invalid(String),
    DuplicateUserTurn(String),
    Storage(String),
}

impl std::fmt::Display for ConversationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) => write!(f, "invalid conversation request: {message}"),
            Self::DuplicateUserTurn(id) => write!(
                f,
                "user turn already exists without a retry operation: {id}"
            ),
            Self::Storage(message) => write!(f, "conversation storage failed: {message}"),
        }
    }
}

impl std::error::Error for ConversationError {}

impl From<StorageError> for ConversationError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<ConversationError> for String {
    fn from(error: ConversationError) -> Self {
        error.to_string()
    }
}

pub struct ConversationService {
    transport: Arc<dyn ChatTransport>,
}

impl ConversationService {
    pub fn new(transport: Arc<dyn ChatTransport>) -> Self {
        Self { transport }
    }

    pub fn with_provider_client() -> Self {
        Self::new(Arc::new(ProviderClient::default()))
    }

    pub fn resume(
        vault: &Vault,
        expected_character_id: Option<&str>,
    ) -> Result<ConversationResume, ConversationError> {
        let character = vault.load_character()?;
        if let Some(expected) = expected_character_id {
            if character.id != expected {
                return Err(ConversationError::Invalid(format!(
                    "vault character {} does not match expected character {expected}",
                    character.id
                )));
            }
        }
        let state = vault.load_state()?;
        let transcript = state
            .last_opened_session_id
            .filter(|session_id| {
                vault
                    .transcript_path(session_id)
                    .map(|path| path.exists())
                    .unwrap_or(false)
            })
            .map(|session_id| vault.load_transcript(&session_id))
            .transpose()?;
        if let Some(transcript) = &transcript {
            if transcript.character_id != character.id {
                return Err(ConversationError::Invalid(
                    "last opened session belongs to a different character".into(),
                ));
            }
        }
        Ok(ConversationResume {
            character,
            transcript,
        })
    }

    pub async fn send(
        &self,
        vault: &Vault,
        snapshot: RequestSnapshot,
        cancellation: CancellationToken,
    ) -> Result<ConversationOutcome, ConversationError> {
        self.send_streaming(vault, snapshot, cancellation, None)
            .await
    }

    pub async fn send_streaming(
        &self,
        vault: &Vault,
        snapshot: RequestSnapshot,
        cancellation: CancellationToken,
        sink: Option<StreamSink>,
    ) -> Result<ConversationOutcome, ConversationError> {
        validate_snapshot(&snapshot)?;
        let mut transcript = load_or_create_transcript(vault, &snapshot)?;
        if let Some(existing_index) = transcript
            .turns
            .iter()
            .position(|turn| turn.id == snapshot.user_turn_id)
        {
            let existing = &transcript.turns[existing_index];
            if existing.role != TurnRole::User || existing.content != snapshot.user_content {
                return Err(ConversationError::Invalid(format!(
                    "user turn id {} is already used for different data",
                    snapshot.user_turn_id
                )));
            }
            if let Some(reply) = transcript.turns[existing_index + 1..]
                .iter()
                .find(|turn| turn.role == TurnRole::Assistant)
                .cloned()
            {
                return Ok(ConversationOutcome {
                    transcript,
                    assistant_turn: reply,
                    retrieved_memories: Vec::new(),
                    context_inspection: ContextInspection::default(),
                });
            }
            return Err(ConversationError::DuplicateUserTurn(snapshot.user_turn_id));
        }

        transcript.turns.push(TranscriptTurn {
            id: snapshot.user_turn_id.clone(),
            timestamp: now_timestamp(),
            role: TurnRole::User,
            status: TurnStatus::Complete,
            content: snapshot.user_content.clone(),
        });
        save_transcript(vault, &mut transcript)?;
        remember_session(vault, &snapshot.session_id)?;
        self.run_reply(vault, snapshot, transcript, cancellation, sink)
            .await
    }

    pub async fn retry(
        &self,
        vault: &Vault,
        snapshot: RequestSnapshot,
        cancellation: CancellationToken,
    ) -> Result<ConversationOutcome, ConversationError> {
        self.retry_streaming(vault, snapshot, cancellation, None)
            .await
    }

    pub async fn retry_streaming(
        &self,
        vault: &Vault,
        snapshot: RequestSnapshot,
        cancellation: CancellationToken,
        sink: Option<StreamSink>,
    ) -> Result<ConversationOutcome, ConversationError> {
        validate_snapshot(&snapshot)?;
        let mut transcript = load_or_create_transcript(vault, &snapshot)?;
        let user_index = transcript
            .turns
            .iter()
            .position(|turn| turn.id == snapshot.user_turn_id && turn.role == TurnRole::User)
            .ok_or_else(|| {
                ConversationError::Invalid(format!(
                    "cannot retry missing user turn {}",
                    snapshot.user_turn_id
                ))
            })?;
        if transcript.turns[user_index].content != snapshot.user_content {
            return Err(ConversationError::Invalid(
                "retry content does not match the persisted user turn".into(),
            ));
        }
        transcript.turns.truncate(user_index + 1);
        save_transcript(vault, &mut transcript)?;
        remember_session(vault, &snapshot.session_id)?;
        self.run_reply(vault, snapshot, transcript, cancellation, sink)
            .await
    }

    pub async fn send_initiative(
        &self,
        vault: &Vault,
        request: InitiativeRequest,
        current_generation: u64,
        latest_user_activity_at: Option<i64>,
        cancellation: CancellationToken,
    ) -> Result<InitiativeOutcome, ConversationError> {
        validate_initiative_request(&request)?;
        let transcript = load_or_create_initiative_transcript(vault, &request)?;
        if initiative_is_stale(&request, current_generation, latest_user_activity_at) {
            return Ok(initiative_outcome(
                transcript,
                Vec::new(),
                InitiativeModelChoice::Silence,
                InitiativeDeliveryStatus::Cancelled,
            ));
        }
        let topic_context = if request.topic_context.trim().is_empty() {
            select_open_topics(vault, &request.character.id, 4)
                .map_err(|error| ConversationError::Storage(error.to_string()))?
        } else {
            request.topic_context.clone()
        };
        let memory_context = if local_memory_endpoint(&request.provider.endpoint) {
            let mut store = MemoryStore::open(vault, &request.character.id)
                .map_err(|error| ConversationError::Storage(error.to_string()))?;
            retrieve(&mut store, &topic_context, RetrievalBudget::default())
                .map_err(|error| ConversationError::Storage(error.to_string()))?
        } else {
            Vec::new()
        };

        let event = render_initiative_event(&topic_context);
        let prompt = build_prompt_with_memories(
            &request.character,
            None,
            &transcript,
            &event,
            &memory_context,
            PromptBudget::default(),
        )
        .map_err(|error| ConversationError::Invalid(error.to_string()))?;
        let events = match self
            .transport
            .stream_chat(
                &request.provider,
                &ChatRequest {
                    provider_id: request.provider.id.clone(),
                    model: request.provider.chat_model.clone(),
                    messages: prompt.messages,
                    cancellation_id: format!("initiative-{}", request.request_id),
                },
                cancellation,
            )
            .await
        {
            Ok(events) => events,
            Err(_) => {
                return Ok(initiative_outcome(
                    transcript,
                    memory_context,
                    InitiativeModelChoice::Silence,
                    InitiativeDeliveryStatus::Failed,
                ));
            }
        };
        let mut content = String::new();
        let mut completed = false;
        let mut cancelled = false;
        let mut failed = false;
        for event in events {
            match event {
                ChatStreamEvent::Delta { text } => content.push_str(&text),
                ChatStreamEvent::Completed { .. } => completed = true,
                ChatStreamEvent::Cancelled => cancelled = true,
                ChatStreamEvent::Failed { .. } => failed = true,
                ChatStreamEvent::Started { .. } => {}
            }
        }
        if cancelled || failed || !completed {
            return Ok(initiative_outcome(
                transcript,
                memory_context,
                InitiativeModelChoice::Silence,
                if cancelled {
                    InitiativeDeliveryStatus::Cancelled
                } else {
                    InitiativeDeliveryStatus::Failed
                },
            ));
        }
        let choice = classify_initiative_response(&content);
        if choice == InitiativeModelChoice::Silence {
            return Ok(initiative_outcome(
                transcript,
                memory_context,
                choice,
                InitiativeDeliveryStatus::Silenced,
            ));
        }

        let mut transcript = transcript;
        let initiative_turn = TranscriptTurn {
            id: format!("initiative-{}", request.request_id),
            timestamp: now_timestamp(),
            role: TurnRole::Initiative,
            status: TurnStatus::Complete,
            content: content.clone(),
        };
        transcript.turns.push(initiative_turn.clone());
        save_transcript(vault, &mut transcript)?;
        remember_session(vault, &request.session_id)?;
        Ok(InitiativeOutcome {
            transcript,
            initiative_turn: Some(initiative_turn),
            assistant_turn: None,
            retrieved_memories: memory_context,
            choice,
            status: InitiativeDeliveryStatus::Delivered,
        })
    }

    async fn run_reply(
        &self,
        vault: &Vault,
        snapshot: RequestSnapshot,
        mut transcript: TranscriptDocument,
        cancellation: CancellationToken,
        sink: Option<StreamSink>,
    ) -> Result<ConversationOutcome, ConversationError> {
        let retrieval_budget = RetrievalBudget::default();
        let (retrieval_report, retrieval_mode, mut fallback_reason) =
            if local_memory_endpoint(&snapshot.provider.endpoint) {
                let mut store = MemoryStore::open(vault, &snapshot.character.id)
                    .map_err(|error| ConversationError::Storage(error.to_string()))?;
                let lexical =
                    retrieve_with_report(&mut store, &snapshot.user_content, retrieval_budget)
                        .map_err(|error| ConversationError::Storage(error.to_string()))?;
                if snapshot.use_hybrid_retrieval {
                    match build_hybrid_retrieval(
                        vault,
                        &snapshot,
                        &mut store,
                        retrieval_budget,
                        lexical.candidate_count,
                        &cancellation,
                    )
                    .await
                    {
                        Ok(report) => (report, "hybrid".to_owned(), None),
                        Err(reason) => (lexical, "lexical".to_owned(), Some(reason)),
                    }
                } else {
                    (lexical, "lexical".to_owned(), None)
                }
            } else {
                (
                    RetrievalReport {
                        selected: Vec::new(),
                        candidate_count: 0,
                        omitted_count: 0,
                    },
                    "disabled".to_owned(),
                    Some("Memory disclosure is disabled for non-loopback endpoints.".into()),
                )
            };
        let memory_context = retrieval_report.selected;
        let mut prompt_budget = PromptBudget::default();
        let mut prompt = build_prompt_with_memories(
            &snapshot.character,
            None,
            &transcript,
            &snapshot.user_content,
            &memory_context,
            prompt_budget,
        )
        .map_err(|error| ConversationError::Invalid(error.to_string()))?;
        let mut request = ChatRequest {
            provider_id: snapshot.provider.id.clone(),
            model: snapshot.provider.chat_model.clone(),
            messages: prompt.messages.clone(),
            cancellation_id: format!("reply-{}", snapshot.user_turn_id),
        };
        let events = match self
            .transport
            .stream_chat_with_sink(
                &snapshot.provider,
                &request,
                cancellation.clone(),
                sink.clone(),
            )
            .await
        {
            Ok(events) => events,
            Err(error) if is_context_limit_error(&error) => {
                prompt_budget = PromptBudget {
                    context_tokens: 2048,
                    reserved_output_tokens: 512,
                };
                match build_prompt_with_memories(
                    &snapshot.character,
                    None,
                    &transcript,
                    &snapshot.user_content,
                    &memory_context,
                    prompt_budget,
                ) {
                    Ok(reduced) => {
                        prompt = reduced;
                        request.messages = prompt.messages.clone();
                        append_fallback_reason(
                            &mut fallback_reason,
                            "Provider context limit detected; retried with a reduced prompt.",
                        );
                        self.transport
                            .stream_chat_with_sink(
                                &snapshot.provider,
                                &request,
                                cancellation,
                                sink,
                            )
                            .await
                            .unwrap_or_else(|retry_error| {
                                vec![ChatStreamEvent::Failed {
                                    message: retry_error.to_string(),
                                }]
                            })
                    }
                    Err(prompt_error) => vec![ChatStreamEvent::Failed {
                        message: format!(
                            "provider rejected the context and a reduced prompt could not be built: {prompt_error}"
                        ),
                    }],
                }
            }
            Err(error) => vec![ChatStreamEvent::Failed {
                message: error.to_string(),
            }],
        };
        let mut content = String::new();
        let mut status = TurnStatus::Failed;
        for event in events {
            match event {
                ChatStreamEvent::Delta { text } => content.push_str(&text),
                ChatStreamEvent::Completed { .. } => status = TurnStatus::Complete,
                ChatStreamEvent::Cancelled => status = TurnStatus::Interrupted,
                ChatStreamEvent::Failed { .. } => status = TurnStatus::Failed,
                ChatStreamEvent::Started { .. } => {}
            }
        }
        let assistant_turn = TranscriptTurn {
            id: format!("reply-{}", snapshot.user_turn_id),
            timestamp: now_timestamp(),
            role: TurnRole::Assistant,
            status,
            content,
        };
        transcript.turns.push(assistant_turn.clone());
        save_transcript(vault, &mut transcript)?;
        if assistant_turn.status == TurnStatus::Complete {
            enqueue_extraction(vault, &transcript, &assistant_turn.id);
        }
        Ok(ConversationOutcome {
            transcript,
            assistant_turn,
            context_inspection: ContextInspection {
                retrieval_mode,
                fallback_reason,
                estimated_input_tokens: prompt.estimated_input_tokens,
                input_token_limit: prompt_budget
                    .context_tokens
                    .saturating_sub(prompt_budget.reserved_output_tokens),
                reserved_output_tokens: prompt.reserved_output_tokens,
                omitted_turns: prompt.omitted_turns,
                selected_memory_tokens: memory_context
                    .iter()
                    .map(|memory| memory.estimated_tokens)
                    .sum(),
                candidate_memories: retrieval_report.candidate_count,
                omitted_memories: retrieval_report.omitted_count,
            },
            retrieved_memories: memory_context,
        })
    }
}

fn is_context_limit_error(error: &ProviderError) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("context")
        && (message.contains("length")
            || message.contains("limit")
            || message.contains("maximum")
            || message.contains("token")
            || message.contains("window"))
}

fn append_fallback_reason(existing: &mut Option<String>, reason: &str) {
    if let Some(existing) = existing {
        existing.push(' ');
        existing.push_str(reason);
    } else {
        *existing = Some(reason.to_owned());
    }
}

async fn build_hybrid_retrieval(
    vault: &Vault,
    snapshot: &RequestSnapshot,
    store: &mut MemoryStore,
    budget: RetrievalBudget,
    lexical_candidate_count: usize,
    cancellation: &CancellationToken,
) -> Result<RetrievalReport, String> {
    let model = snapshot
        .provider
        .embedding_model
        .as_deref()
        .filter(|model| !model.trim().is_empty())
        .ok_or_else(|| {
            "Hybrid retrieval requested without an embedding model; lexical fallback used."
                .to_owned()
        })?;
    let client = ProviderClient::default();
    let query_request = EmbeddingRequest {
        provider_id: snapshot.provider.id.clone(),
        model: model.to_owned(),
        input: vec![snapshot.user_content.clone()],
    };
    let query_response = tokio::select! {
        _ = cancellation.cancelled() => return Err("Embedding request was cancelled; lexical fallback used.".into()),
        response = client.embed(&snapshot.provider, &query_request) => response,
    }
    .map_err(|error| format!("Embedding provider unavailable ({error}); lexical fallback used."))?;
    let query_vector = query_response.vectors.first().cloned().ok_or_else(|| {
        "Embedding provider returned no query vector; lexical fallback used.".to_owned()
    })?;
    let memories = store.list().map_err(|error| {
        format!("Could not load memories for embeddings ({error}); lexical fallback used.")
    })?;
    let chunks = chunks_from_memories(&memories);
    let space = EmbeddingSpace::new(&snapshot.provider.id, model, query_response.dimensions);
    let index = EmbeddingIndex::open(vault, &snapshot.character.id, space).map_err(|error| {
        format!("Could not open the embedding index ({error}); lexical fallback used.")
    })?;
    if index.needs_rebuild(&chunks).map_err(|error| {
        format!("Could not inspect the embedding index ({error}); lexical fallback used.")
    })? {
        let vectors = if chunks.is_empty() {
            Vec::new()
        } else {
            let request = EmbeddingRequest {
                provider_id: snapshot.provider.id.clone(),
                model: model.to_owned(),
                input: chunks.iter().map(|chunk| chunk.content.clone()).collect(),
            };
            tokio::select! {
                _ = cancellation.cancelled() => return Err("Embedding rebuild was cancelled; lexical fallback used.".into()),
                response = client.embed(&snapshot.provider, &request) => response,
            }
            .map_err(|error| format!("Embedding rebuild failed ({error}); lexical fallback used."))?
            .vectors
        };
        index
            .rebuild_precomputed(&chunks, &vectors)
            .map_err(|error| {
                format!("Embedding rebuild failed ({error}); lexical fallback used.")
            })?;
    }
    let semantic = index
        .search(&query_vector, budget.max_results.saturating_mul(8))
        .map_err(|error| format!("Semantic search failed ({error}); lexical fallback used."))?;
    let selected = hybrid_retrieve(store, &snapshot.user_content, &semantic, &[], budget)
        .map_err(|error| format!("Hybrid ranking failed ({error}); lexical fallback used."))?;
    let candidate_count = semantic
        .len()
        .max(lexical_candidate_count)
        .max(selected.len());
    Ok(RetrievalReport {
        omitted_count: candidate_count.saturating_sub(selected.len()),
        candidate_count,
        selected,
    })
}

fn validate_snapshot(snapshot: &RequestSnapshot) -> Result<(), ConversationError> {
    if snapshot.character.id.is_empty()
        || snapshot.provider.id.is_empty()
        || snapshot.session_id.is_empty()
        || snapshot.user_turn_id.is_empty()
        || snapshot.user_content.trim().is_empty()
    {
        return Err(ConversationError::Invalid(
            "character, provider, session, user turn, and content are required".into(),
        ));
    }
    if snapshot.character.id != snapshot.character.id.trim() {
        return Err(ConversationError::Invalid(
            "character id contains surrounding whitespace".into(),
        ));
    }
    Ok(())
}

fn validate_initiative_request(request: &InitiativeRequest) -> Result<(), ConversationError> {
    if request.character.id.is_empty()
        || request.provider.id.is_empty()
        || request.session_id.is_empty()
        || request.request_id.is_empty()
    {
        return Err(ConversationError::Invalid(
            "character, provider, session, and initiative request ids are required".into(),
        ));
    }
    if request.character.id != request.character.id.trim() {
        return Err(ConversationError::Invalid(
            "character id contains surrounding whitespace".into(),
        ));
    }
    Ok(())
}

fn initiative_outcome(
    transcript: TranscriptDocument,
    retrieved_memories: Vec<crate::retrieval::RetrievedMemory>,
    choice: InitiativeModelChoice,
    status: InitiativeDeliveryStatus,
) -> InitiativeOutcome {
    InitiativeOutcome {
        transcript,
        initiative_turn: None,
        assistant_turn: None,
        retrieved_memories,
        choice,
        status,
    }
}

fn local_memory_endpoint(endpoint: &str) -> bool {
    url::Url::parse(endpoint)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]" | "::1"))
}

fn load_or_create_transcript(
    vault: &Vault,
    snapshot: &RequestSnapshot,
) -> Result<TranscriptDocument, ConversationError> {
    let path = vault.transcript_path(&snapshot.session_id)?;
    if path.exists() {
        let transcript = vault.load_transcript(&snapshot.session_id)?;
        if transcript.character_id != snapshot.character.id {
            return Err(ConversationError::Invalid(
                "session belongs to a different character".into(),
            ));
        }
        Ok(transcript)
    } else {
        let mut transcript = TranscriptDocument::new(&snapshot.session_id, &snapshot.character.id);
        transcript.created_at = now_timestamp();
        Ok(transcript)
    }
}

fn load_or_create_initiative_transcript(
    vault: &Vault,
    request: &InitiativeRequest,
) -> Result<TranscriptDocument, ConversationError> {
    let path = vault.transcript_path(&request.session_id)?;
    if path.exists() {
        let transcript = vault.load_transcript(&request.session_id)?;
        if transcript.character_id != request.character.id {
            return Err(ConversationError::Invalid(
                "session belongs to a different character".into(),
            ));
        }
        Ok(transcript)
    } else {
        let mut transcript = TranscriptDocument::new(&request.session_id, &request.character.id);
        transcript.created_at = now_timestamp();
        Ok(transcript)
    }
}

fn save_transcript(
    vault: &Vault,
    transcript: &mut TranscriptDocument,
) -> Result<(), ConversationError> {
    transcript.updated_at = now_timestamp();
    vault.save_transcript(transcript)?;
    Ok(())
}

fn remember_session(vault: &Vault, session_id: &str) -> Result<(), ConversationError> {
    let mut state: OperationalState = vault.load_state()?;
    state.last_opened_session_id = Some(session_id.to_owned());
    vault.save_state(&state)?;
    Ok(())
}

fn enqueue_extraction(vault: &Vault, transcript: &TranscriptDocument, assistant_turn_id: &str) {
    let Ok(mut queue) = ExtractionQueue::open(vault, &transcript.character_id) else {
        return;
    };
    let _ = queue.enqueue_transcript(transcript, assistant_turn_id, 100);
}

fn now_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ProviderKind;
    use std::{fs, sync::Mutex};

    struct FakeTransport {
        responses: Mutex<Vec<Result<Vec<ChatStreamEvent>, ProviderError>>>,
        requests: Mutex<Vec<ChatRequest>>,
    }

    impl FakeTransport {
        fn new(responses: Vec<Result<Vec<ChatStreamEvent>, ProviderError>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                requests: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl ChatTransport for FakeTransport {
        async fn stream_chat(
            &self,
            _config: &ProviderConfig,
            request: &ChatRequest,
            _cancellation: CancellationToken,
        ) -> Result<Vec<ChatStreamEvent>, ProviderError> {
            self.requests.lock().unwrap().push(request.clone());
            self.responses.lock().unwrap().remove(0)
        }
    }

    fn snapshot(
        session_id: &str,
        character_id: &str,
        user_turn_id: &str,
        content: &str,
    ) -> RequestSnapshot {
        RequestSnapshot {
            character: CharacterDefinition::new(
                character_id,
                character_id,
                format!("You are {character_id}."),
            ),
            provider: ProviderConfig {
                id: "fake".into(),
                kind: ProviderKind::Ollama,
                endpoint: "http://127.0.0.1:11434".into(),
                chat_model: "fake-model".into(),
                embedding_model: None,
                bearer_token: None,
            },
            session_id: session_id.into(),
            user_turn_id: user_turn_id.into(),
            user_content: content.into(),
            use_hybrid_retrieval: false,
        }
    }

    fn root(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "tz-chatter-conversation-{label}-{}",
            crate::storage::new_stable_id()
        ))
    }

    #[tokio::test]
    async fn persists_complete_turns_and_is_idempotent_on_retry_delivery() {
        let root = root("complete");
        let vault = Vault::create(&root).unwrap();
        let fake = Arc::new(FakeTransport::new(vec![Ok(vec![
            ChatStreamEvent::Started {
                cancellation_id: "reply-user-1".into(),
            },
            ChatStreamEvent::Delta {
                text: "Hello back".into(),
            },
            ChatStreamEvent::Completed {
                finish_reason: Some("stop".into()),
            },
        ])]));
        let service = ConversationService::new(fake.clone());
        let request = snapshot("session-1", "lyra", "user-1", "Hello");
        let first = service
            .send(&vault, request.clone(), CancellationToken::new())
            .await
            .unwrap();
        let second = service
            .send(&vault, request, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(first.assistant_turn.status, TurnStatus::Complete);
        assert_eq!(second.assistant_turn.id, first.assistant_turn.id);
        assert_eq!(
            second
                .transcript
                .turns
                .iter()
                .filter(|turn| turn.role == TurnRole::User)
                .count(),
            1
        );
        assert_eq!(fake.requests.lock().unwrap().len(), 1);
        let captured = fake.requests.lock().unwrap()[0].clone();
        assert!(captured.messages[0].content.contains("You are lyra."));
        assert_eq!(
            captured
                .messages
                .iter()
                .filter(|message| message.content == "Hello")
                .count(),
            1
        );
        assert_eq!(vault.load_transcript("session-1").unwrap().turns.len(), 2);
        assert_eq!(
            vault
                .load_state()
                .unwrap()
                .last_opened_session_id
                .as_deref(),
            Some("session-1")
        );
        let mut extraction_queue = ExtractionQueue::open(&vault, "lyra").unwrap();
        let extraction_job = extraction_queue.claim_next().unwrap().unwrap();
        assert_eq!(extraction_job.session_id, "session-1");
        assert_eq!(
            extraction_job.source_turn_ids,
            vec!["user-1", "reply-user-1"]
        );
        drop(extraction_queue);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resumes_the_last_opened_session_and_rejects_a_different_expected_character() {
        let root = root("resume");
        let vault = Vault::create(&root).unwrap();
        let character = CharacterDefinition::new("lyra", "Lyra", "You are Lyra.");
        vault.save_character(&character).unwrap();
        let mut transcript = TranscriptDocument::new("session-1", "lyra");
        transcript.turns.push(TranscriptTurn {
            id: "user-1".into(),
            timestamp: "1".into(),
            role: TurnRole::User,
            status: TurnStatus::Complete,
            content: "Hello".into(),
        });
        vault.save_transcript(&transcript).unwrap();
        vault
            .save_state(&OperationalState {
                last_opened_session_id: Some("session-1".into()),
                ..OperationalState::default()
            })
            .unwrap();

        let resumed = ConversationService::resume(&vault, None).unwrap();
        assert_eq!(resumed.character, character);
        assert_eq!(resumed.transcript.unwrap(), transcript);
        assert!(matches!(
            ConversationService::resume(&vault, Some("nova")),
            Err(ConversationError::Invalid(message)) if message.contains("does not match")
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn failed_and_cancelled_replies_are_persisted_with_status_and_retry_keeps_one_user_turn()
    {
        let root = root("retry");
        let vault = Vault::create(&root).unwrap();
        let fake = Arc::new(FakeTransport::new(vec![
            Ok(vec![
                ChatStreamEvent::Started {
                    cancellation_id: "reply-user-1".into(),
                },
                ChatStreamEvent::Failed {
                    message: "offline".into(),
                },
            ]),
            Ok(vec![
                ChatStreamEvent::Started {
                    cancellation_id: "reply-user-1".into(),
                },
                ChatStreamEvent::Delta {
                    text: "Recovered".into(),
                },
                ChatStreamEvent::Completed {
                    finish_reason: None,
                },
            ]),
        ]));
        let service = ConversationService::new(fake);
        let request = snapshot("session-1", "lyra", "user-1", "Try again");
        let failed = service
            .send(&vault, request.clone(), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(failed.assistant_turn.status, TurnStatus::Failed);
        let recovered = service
            .retry(&vault, request, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(recovered.assistant_turn.status, TurnStatus::Complete);
        assert_eq!(
            recovered
                .transcript
                .turns
                .iter()
                .filter(|turn| turn.role == TurnRole::User)
                .count(),
            1
        );
        assert_eq!(
            recovered
                .transcript
                .turns
                .iter()
                .filter(|turn| turn.role == TurnRole::Assistant)
                .count(),
            1
        );
        fs::remove_dir_all(root).unwrap();

        let cancel_root = std::env::temp_dir().join(format!(
            "tz-chatter-conversation-cancel-{}",
            crate::storage::new_stable_id()
        ));
        let vault = Vault::create(&cancel_root).unwrap();
        let service = ConversationService::new(Arc::new(FakeTransport::new(vec![Ok(vec![
            ChatStreamEvent::Cancelled,
        ])])));
        let cancelled = service
            .send(
                &vault,
                snapshot("session-2", "lyra", "user-2", "Stop"),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(cancelled.assistant_turn.status, TurnStatus::Interrupted);
        fs::remove_dir_all(cancel_root).unwrap();
    }

    #[tokio::test]
    async fn provider_context_limit_retries_once_with_a_smaller_prompt() {
        let root = root("context-recovery");
        let vault = Vault::create(&root).unwrap();
        let mut history = TranscriptDocument::new("session", "lyra");
        for index in 0..40 {
            history.turns.push(TranscriptTurn {
                id: format!("history-user-{index}"),
                timestamp: index.to_string(),
                role: TurnRole::User,
                status: TurnStatus::Complete,
                content: format!(
                    "Earlier user message {index} with extra filler text to consume the prompt budget."
                ),
            });
            history.turns.push(TranscriptTurn {
                id: format!("history-assistant-{index}"),
                timestamp: format!("a{index}"),
                role: TurnRole::Assistant,
                status: TurnStatus::Complete,
                content: format!(
                    "Earlier assistant reply {index} with extra filler text to consume the prompt budget."
                ),
            });
        }
        vault.save_transcript(&history).unwrap();
        let fake = Arc::new(FakeTransport::new(vec![
            Err(ProviderError::Unavailable(
                "maximum context length token limit exceeded".into(),
            )),
            Ok(vec![
                ChatStreamEvent::Delta {
                    text: "Recovered with less history".into(),
                },
                ChatStreamEvent::Completed {
                    finish_reason: Some("stop".into()),
                },
            ]),
        ]));
        let service = ConversationService::new(fake.clone());
        let outcome = service
            .send(
                &vault,
                snapshot("session", "lyra", "user", "Please continue"),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(outcome.assistant_turn.status, TurnStatus::Complete);
        let requests = fake.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        let first_chars: usize = requests[0]
            .messages
            .iter()
            .map(|message| message.content.len())
            .sum();
        let second_chars: usize = requests[1]
            .messages
            .iter()
            .map(|message| message.content.len())
            .sum();
        assert!(
            second_chars < first_chars,
            "retry prompt should be smaller than the rejected prompt ({second_chars} >= {first_chars})"
        );
        assert!(outcome
            .context_inspection
            .fallback_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("reduced prompt")));
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn send_streaming_emits_partial_text_before_completion() {
        let root = root("stream-events");
        let vault = Vault::create(&root).unwrap();
        let fake = Arc::new(FakeTransport::new(vec![Ok(vec![
            ChatStreamEvent::Started {
                cancellation_id: "reply-user".into(),
            },
            ChatStreamEvent::Delta {
                text: "Partial ".into(),
            },
            ChatStreamEvent::Delta {
                text: "reply".into(),
            },
            ChatStreamEvent::Completed {
                finish_reason: Some("stop".into()),
            },
        ])]));
        let service = ConversationService::new(fake);
        let received = Arc::new(Mutex::new(Vec::new()));
        let sink_events = received.clone();
        let sink: StreamSink = Arc::new(move |event| sink_events.lock().unwrap().push(event));
        let outcome = service
            .send_streaming(
                &vault,
                snapshot("session", "lyra", "user", "Hello"),
                CancellationToken::new(),
                Some(sink),
            )
            .await
            .unwrap();
        let events = received.lock().unwrap().clone();
        assert!(matches!(
            events.first(),
            Some(ChatStreamEvent::Started { .. })
        ));
        assert!(events.iter().any(|event| matches!(
            event,
            ChatStreamEvent::Delta { text } if text == "Partial "
        )));
        assert_eq!(outcome.assistant_turn.content, "Partial reply");
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn send_rejects_an_unloaded_character_snapshot() {
        let root = root("unloaded-character");
        let vault = Vault::create(&root).unwrap();
        let fake = Arc::new(FakeTransport::new(Vec::new()));
        let service = ConversationService::new(fake);
        let mut request = snapshot("session", "lyra", "user", "Hello");
        request.character.id.clear();
        let error = service
            .send(&vault, request, CancellationToken::new())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("required"));
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn character_snapshot_keeps_replies_in_their_own_vaults() {
        let root_a = root("character-a");
        let root_b = root("character-b");
        let vault_a = Vault::create(&root_a).unwrap();
        let vault_b = Vault::create(&root_b).unwrap();
        let fake = Arc::new(FakeTransport::new(vec![
            Ok(vec![
                ChatStreamEvent::Delta { text: "A".into() },
                ChatStreamEvent::Completed {
                    finish_reason: None,
                },
            ]),
            Ok(vec![
                ChatStreamEvent::Delta { text: "B".into() },
                ChatStreamEvent::Completed {
                    finish_reason: None,
                },
            ]),
        ]));
        let service = ConversationService::new(fake);
        service
            .send(
                &vault_a,
                snapshot("session", "lyra", "user-a", "A").clone(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        service
            .send(
                &vault_b,
                snapshot("session", "nova", "user-b", "B").clone(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(
            vault_a.load_transcript("session").unwrap().character_id,
            "lyra"
        );
        assert_eq!(
            vault_b.load_transcript("session").unwrap().character_id,
            "nova"
        );
        assert_eq!(
            vault_a.load_transcript("session").unwrap().turns[1].content,
            "A"
        );
        assert_eq!(
            vault_b.load_transcript("session").unwrap().turns[1].content,
            "B"
        );
        fs::remove_dir_all(root_a).unwrap();
        fs::remove_dir_all(root_b).unwrap();
    }

    #[tokio::test]
    async fn refuses_to_use_a_session_belonging_to_another_character() {
        let root = root("isolation");
        let vault = Vault::create(&root).unwrap();
        let existing = snapshot("session", "lyra", "user-1", "hello");
        vault
            .save_transcript(&TranscriptDocument {
                schema_version: crate::storage::STORAGE_SCHEMA_VERSION,
                session_id: "session".into(),
                character_id: "lyra".into(),
                created_at: "1".into(),
                updated_at: "1".into(),
                turns: vec![TranscriptTurn {
                    id: "user-1".into(),
                    timestamp: "1".into(),
                    role: TurnRole::User,
                    status: TurnStatus::Complete,
                    content: existing.user_content.clone(),
                }],
            })
            .unwrap();
        let service = ConversationService::new(Arc::new(FakeTransport::new(Vec::new())));
        let error = service
            .send(
                &vault,
                snapshot("session", "nova", "user-2", "not for Lyra"),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(error, ConversationError::Invalid(message) if message.contains("different character"))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn opening_a_missing_vault_does_not_create_it() {
        let root = root("missing");
        assert!(Vault::open(&root).is_err());
        assert!(!root.exists());
    }

    fn initiative_request(session_id: &str, request_id: &str) -> InitiativeRequest {
        let user_snapshot = snapshot(session_id, "lyra", "unused", "unused");
        InitiativeRequest {
            character: user_snapshot.character,
            provider: user_snapshot.provider,
            session_id: session_id.into(),
            request_id: request_id.into(),
            topic_context: "Ask about the garden project".into(),
            generation: 1,
            started_at: 100,
        }
    }

    #[tokio::test]
    async fn initiative_delivery_is_labeled_and_does_not_create_a_fake_user_turn() {
        let root = root("initiative-delivery");
        let vault = Vault::create(&root).unwrap();
        let fake = Arc::new(FakeTransport::new(vec![
            Ok(vec![
                ChatStreamEvent::Delta {
                    text: "Want to revisit the garden project?".into(),
                },
                ChatStreamEvent::Completed {
                    finish_reason: None,
                },
            ]),
            Ok(vec![
                ChatStreamEvent::Delta {
                    text: "SILENCE".into(),
                },
                ChatStreamEvent::Completed {
                    finish_reason: None,
                },
            ]),
        ]));
        let service = ConversationService::new(fake.clone());

        let delivered = service
            .send_initiative(
                &vault,
                initiative_request("initiative-session", "request-1"),
                1,
                None,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(delivered.status, InitiativeDeliveryStatus::Delivered);
        assert_eq!(delivered.choice, InitiativeModelChoice::Message);
        assert_eq!(delivered.transcript.turns.len(), 1);
        assert_eq!(delivered.transcript.turns[0].role, TurnRole::Initiative);
        assert!(!delivered
            .transcript
            .turns
            .iter()
            .any(|turn| turn.role == TurnRole::User));

        let silenced = service
            .send_initiative(
                &vault,
                initiative_request("silence-session", "request-2"),
                1,
                None,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(silenced.status, InitiativeDeliveryStatus::Silenced);
        assert_eq!(silenced.choice, InitiativeModelChoice::Silence);
        assert!(silenced.transcript.turns.is_empty());
        assert_eq!(fake.requests.lock().unwrap().len(), 2);
        assert!(fake.requests.lock().unwrap()[0]
            .messages
            .last()
            .unwrap()
            .content
            .contains("[INTERNAL INITIATIVE EVENT]"));
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn stale_and_cancelled_initiative_work_is_not_persisted() {
        let root = root("initiative-cancel");
        let vault = Vault::create(&root).unwrap();
        let fake = Arc::new(FakeTransport::new(vec![Ok(vec![
            ChatStreamEvent::Cancelled,
        ])]));
        let service = ConversationService::new(fake.clone());
        let request = initiative_request("stale-session", "request-1");
        let stale = service
            .send_initiative(&vault, request.clone(), 2, None, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(stale.status, InitiativeDeliveryStatus::Cancelled);
        assert!(stale.transcript.turns.is_empty());
        assert!(fake.requests.lock().unwrap().is_empty());

        let cancelled = service
            .send_initiative(&vault, request, 1, Some(101), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(cancelled.status, InitiativeDeliveryStatus::Cancelled);
        assert!(cancelled.transcript.turns.is_empty());
        assert!(fake.requests.lock().unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
