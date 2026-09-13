use crate::{
    connections::{ProviderClient, StreamSink},
    embeddings::{chunks_from_memories, EmbeddingIndex, CHUNKING_VERSION, EMBEDDING_INDEX_VERSION},
    extraction::ExtractionQueue,
    initiative::{
        classify_initiative_response, initiative_is_stale, render_initiative_event,
        select_open_topics, InitiativeModelChoice, InitiativeRequest,
    },
    memory::MemoryStore,
    prompt::{build_prompt_with_memories, PromptBudget, PromptLayers},
    providers::{
        ChatRequest, ChatStreamEvent, EmbeddingRequest, EmbeddingResponse, ProviderConfig,
        ProviderError,
    },
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

    async fn embed(
        &self,
        config: &ProviderConfig,
        _request: &EmbeddingRequest,
    ) -> Result<EmbeddingResponse, ProviderError> {
        Err(ProviderError::UnsupportedCapability {
            capability: "embeddings".into(),
            provider: config.kind.clone(),
        })
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

    async fn embed(
        &self,
        config: &ProviderConfig,
        request: &EmbeddingRequest,
    ) -> Result<EmbeddingResponse, ProviderError> {
        ProviderClient::embed(self, config, request).await
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
    #[serde(default)]
    pub application_prompt: String,
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SessionSummary {
    pub session_id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub turn_count: usize,
    pub preview: String,
    pub archived: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

impl From<TranscriptDocument> for SessionSummary {
    fn from(transcript: TranscriptDocument) -> Self {
        let preview = transcript
            .turns
            .iter()
            .rev()
            .find(|turn| !turn.content.trim().is_empty())
            .map(|turn| truncate_preview(turn.content.trim()))
            .unwrap_or_default();
        Self {
            title: session_display_title(&transcript),
            session_id: transcript.session_id,
            created_at: transcript.created_at,
            updated_at: transcript.updated_at,
            turn_count: transcript.turns.len(),
            preview,
            archived: transcript.archived,
            snippet: None,
        }
    }
}

fn truncate_preview(content: &str) -> String {
    let flattened = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = flattened.chars();
    let shortened: String = chars.by_ref().take(80).collect();
    if chars.next().is_some() {
        format!("{shortened}…")
    } else {
        shortened
    }
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

pub(crate) const EMBEDDING_INDEX_REBUILDING: &str =
    "Embedding index is rebuilding; lexical fallback used.";
pub(crate) const HYBRID_WITHOUT_EMBEDDING_MODEL: &str =
    "Hybrid retrieval requested without an embedding model; lexical fallback used.";

pub(crate) fn embedding_model_configured(provider: &ProviderConfig) -> bool {
    provider
        .embedding_model
        .as_deref()
        .is_some_and(|model| !model.trim().is_empty())
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

    pub fn list_sessions(vault: &Vault) -> Result<Vec<SessionSummary>, ConversationError> {
        Self::list_sessions_filtered(vault, false)
    }

    pub fn list_sessions_filtered(
        vault: &Vault,
        include_archived: bool,
    ) -> Result<Vec<SessionSummary>, ConversationError> {
        let character = vault.load_character()?;
        let mut sessions: Vec<SessionSummary> = vault
            .list_transcripts()?
            .into_iter()
            .filter(|transcript| transcript.character_id == character.id)
            .filter(|transcript| include_archived || !transcript.archived)
            .map(SessionSummary::from)
            .collect();
        sort_sessions(&mut sessions);
        Ok(sessions)
    }

    pub fn search_sessions(
        vault: &Vault,
        query: &str,
    ) -> Result<Vec<SessionSummary>, ConversationError> {
        let needle = query.trim();
        if needle.is_empty() {
            return Self::list_sessions_filtered(vault, true);
        }
        let character = vault.load_character()?;
        let mut sessions: Vec<SessionSummary> = vault
            .list_transcripts()?
            .into_iter()
            .filter(|transcript| transcript.character_id == character.id)
            .filter_map(|transcript| {
                let snippet = session_search_hit(&transcript, needle)?;
                let mut summary = SessionSummary::from(transcript);
                summary.snippet = Some(snippet);
                Some(summary)
            })
            .collect();
        sort_sessions(&mut sessions);
        Ok(sessions)
    }

    pub fn rename_session(
        vault: &Vault,
        session_id: &str,
        title: &str,
    ) -> Result<TranscriptDocument, ConversationError> {
        let mut transcript = owned_session_transcript(vault, session_id)?;
        transcript.title = truncate_preview(title.trim());
        save_transcript(vault, &mut transcript)?;
        Ok(transcript)
    }

    pub fn set_session_archived(
        vault: &Vault,
        session_id: &str,
        archived: bool,
    ) -> Result<TranscriptDocument, ConversationError> {
        let mut transcript = owned_session_transcript(vault, session_id)?;
        transcript.archived = archived;
        save_transcript(vault, &mut transcript)?;
        Ok(transcript)
    }

    pub fn open_session(
        vault: &Vault,
        session_id: &str,
    ) -> Result<ConversationResume, ConversationError> {
        let character = vault.load_character()?;
        let path = vault.transcript_path(session_id)?;
        let transcript = if path.exists() {
            let loaded = vault.load_transcript(session_id)?;
            if loaded.character_id != character.id {
                return Err(ConversationError::Invalid(
                    "session belongs to a different character".into(),
                ));
            }
            loaded
        } else {
            let timestamp = now_timestamp();
            let mut started = TranscriptDocument::new(session_id, character.id.clone());
            started.created_at = timestamp.clone();
            started.updated_at = timestamp;
            started
        };
        remember_session(vault, session_id)?;
        Ok(ConversationResume {
            character,
            transcript: Some(transcript),
        })
    }

    pub fn start_session(vault: &Vault) -> Result<ConversationResume, ConversationError> {
        let character = vault.load_character()?;
        let session_id = crate::storage::new_stable_id();
        remember_session(vault, &session_id)?;
        let timestamp = now_timestamp();
        let mut transcript = TranscriptDocument::new(session_id, character.id.clone());
        transcript.created_at = timestamp.clone();
        transcript.updated_at = timestamp;
        Ok(ConversationResume {
            character,
            transcript: Some(transcript),
        })
    }

    pub fn set_session_local(
        vault: &Vault,
        session_id: &str,
        selection: crate::scene::SessionLocalSelection,
    ) -> Result<TranscriptDocument, ConversationError> {
        crate::scene::validate_selection(vault, &selection)
            .map_err(|error| ConversationError::Invalid(error.to_string()))?;
        let mut resume = Self::open_session(vault, session_id)?;
        let mut transcript = resume.transcript.take().ok_or_else(|| {
            ConversationError::Invalid("session transcript is missing after open".into())
        })?;
        crate::scene::apply_selection(&mut transcript, &selection);
        transcript.updated_at = now_timestamp();
        vault.save_transcript(&transcript)?;
        Ok(transcript)
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
            let skip = ExtractionQueue::open(vault, &request.character.id)
                .ok()
                .and_then(|queue| queue.superseded_targets().ok())
                .unwrap_or_default();
            retrieve(
                &mut store,
                &topic_context,
                RetrievalBudget::default(),
                &skip,
            )
            .map_err(|error| ConversationError::Storage(error.to_string()))?
        } else {
            Vec::new()
        };

        let event = render_initiative_event(&topic_context);
        let application_prompt = request.application_prompt.trim();
        let (persona, scene) = prompt_layers(vault, &transcript)?;
        let prompt = build_prompt_with_memories(
            &request.character,
            PromptLayers {
                application_prompt: if application_prompt.is_empty() {
                    None
                } else {
                    Some(application_prompt)
                },
                user_persona: persona.as_deref(),
                scene_context: scene.as_deref(),
            },
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
                let skip = ExtractionQueue::open(vault, &snapshot.character.id)
                    .ok()
                    .and_then(|queue| queue.superseded_targets().ok())
                    .unwrap_or_default();
                let prefer_hybrid =
                    snapshot.use_hybrid_retrieval && embedding_model_configured(&snapshot.provider);
                let lexical = retrieve_with_report(
                    &mut store,
                    &snapshot.user_content,
                    retrieval_budget,
                    &skip,
                )
                .map_err(|error| ConversationError::Storage(error.to_string()))?;
                if prefer_hybrid {
                    match build_hybrid_retrieval(
                        self.transport.as_ref(),
                        vault,
                        &snapshot,
                        &mut store,
                        retrieval_budget,
                        lexical.candidate_count,
                        &skip,
                        &cancellation,
                    )
                    .await
                    {
                        Ok(report) => (report, "hybrid".to_owned(), None),
                        Err(reason) => (lexical, "lexical".to_owned(), Some(reason)),
                    }
                } else if snapshot.use_hybrid_retrieval {
                    (
                        lexical,
                        "lexical".to_owned(),
                        Some(HYBRID_WITHOUT_EMBEDDING_MODEL.to_owned()),
                    )
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
        let application_prompt = snapshot.application_prompt.trim();
        let application_prompt = if application_prompt.is_empty() {
            None
        } else {
            Some(application_prompt)
        };
        let (persona, scene) = prompt_layers(vault, &transcript)?;
        let layers = PromptLayers {
            application_prompt,
            user_persona: persona.as_deref(),
            scene_context: scene.as_deref(),
        };
        let mut prompt = build_prompt_with_memories(
            &snapshot.character,
            layers,
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
                    layers,
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

#[allow(clippy::too_many_arguments)]
async fn build_hybrid_retrieval(
    transport: &dyn ChatTransport,
    vault: &Vault,
    snapshot: &RequestSnapshot,
    store: &mut MemoryStore,
    budget: RetrievalBudget,
    lexical_candidate_count: usize,
    skip_memory_ids: &[String],
    cancellation: &CancellationToken,
) -> Result<RetrievalReport, String> {
    let model = snapshot
        .provider
        .embedding_model
        .as_deref()
        .filter(|model| !model.trim().is_empty())
        .ok_or_else(|| HYBRID_WITHOUT_EMBEDDING_MODEL.to_owned())?;
    let memories = store.list().map_err(|error| {
        format!("Could not load memories for embeddings ({error}); lexical fallback used.")
    })?;
    let chunks = chunks_from_memories(&memories);
    let stored = EmbeddingIndex::stored_space(vault, &snapshot.character.id).map_err(|error| {
        format!("Could not inspect the embedding index ({error}); lexical fallback used.")
    })?;
    let Some(space) = stored.filter(|space| {
        space.provider_id == snapshot.provider.id
            && space.model == model
            && space.chunking_version == CHUNKING_VERSION
            && space.index_version == EMBEDDING_INDEX_VERSION
    }) else {
        return Err(EMBEDDING_INDEX_REBUILDING.to_owned());
    };
    let index = EmbeddingIndex::open(vault, &snapshot.character.id, space).map_err(|error| {
        format!("Could not open the embedding index ({error}); lexical fallback used.")
    })?;
    if index.needs_rebuild(&chunks).map_err(|error| {
        format!("Could not inspect the embedding index ({error}); lexical fallback used.")
    })? {
        return Err(EMBEDDING_INDEX_REBUILDING.to_owned());
    }
    let query_request = EmbeddingRequest {
        provider_id: snapshot.provider.id.clone(),
        model: model.to_owned(),
        input: vec![snapshot.user_content.clone()],
    };
    let query_response = tokio::select! {
        _ = cancellation.cancelled() => return Err("Embedding request was cancelled; lexical fallback used.".into()),
        response = transport.embed(&snapshot.provider, &query_request) => response,
    }
    .map_err(|error| format!("Embedding provider unavailable ({error}); lexical fallback used."))?;
    let query_vector = query_response.vectors.first().cloned().ok_or_else(|| {
        "Embedding provider returned no query vector; lexical fallback used.".to_owned()
    })?;
    let semantic = index
        .search(&query_vector, budget.max_results.saturating_mul(8))
        .map_err(|error| format!("Semantic search failed ({error}); lexical fallback used."))?;
    let selected = hybrid_retrieve(
        store,
        &snapshot.user_content,
        &semantic,
        &[],
        skip_memory_ids,
        budget,
    )
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

fn prompt_layers(
    vault: &Vault,
    transcript: &TranscriptDocument,
) -> Result<(Option<String>, Option<String>), ConversationError> {
    let context =
        crate::scene::resolve_prompt_context(vault, transcript).map_err(|error| match &error {
            StorageError::InvalidSchema(message) | StorageError::InvalidPath(message) => {
                ConversationError::Invalid(message.clone())
            }
            _ => ConversationError::Storage(error.to_string()),
        })?;
    Ok((context.persona, context.scene))
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
    apply_auto_title(transcript);
    transcript.updated_at = now_timestamp();
    vault.save_transcript(transcript)?;
    Ok(())
}

fn owned_session_transcript(
    vault: &Vault,
    session_id: &str,
) -> Result<TranscriptDocument, ConversationError> {
    ConversationService::open_session(vault, session_id)?
        .transcript
        .ok_or_else(|| {
            ConversationError::Invalid("session transcript is missing after open".into())
        })
}

fn apply_auto_title(transcript: &mut TranscriptDocument) {
    if !transcript.title.trim().is_empty() {
        return;
    }
    if let Some(content) = first_user_content(transcript) {
        transcript.title = truncate_preview(content);
    }
}

fn first_user_content(transcript: &TranscriptDocument) -> Option<&str> {
    transcript
        .turns
        .iter()
        .find(|turn| turn.role == TurnRole::User && !turn.content.trim().is_empty())
        .map(|turn| turn.content.as_str())
}

fn session_display_title(transcript: &TranscriptDocument) -> String {
    let titled = transcript.title.trim();
    if !titled.is_empty() {
        return titled.to_owned();
    }
    first_user_content(transcript)
        .map(truncate_preview)
        .unwrap_or_default()
}

fn sort_sessions(sessions: &mut [SessionSummary]) {
    sessions.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.session_id.cmp(&left.session_id))
    });
}

fn session_search_hit(transcript: &TranscriptDocument, query: &str) -> Option<String> {
    let title = session_display_title(transcript);
    if contains_ignore_case(&title, query) {
        return Some(title);
    }
    transcript
        .turns
        .iter()
        .find_map(|turn| match_snippet(&turn.content, query))
}

fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn match_snippet(text: &str, query: &str) -> Option<String> {
    let flattened = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let haystack = flattened.to_lowercase();
    let needle = query.to_lowercase();
    let index = haystack.find(&needle)?;
    if flattened.len() != haystack.len() {
        return Some(truncate_preview(&flattened));
    }
    let start = flattened.floor_char_boundary(index.saturating_sub(24));
    let end = flattened.ceil_char_boundary((index + needle.len() + 32).min(flattened.len()));
    let mut snippet = flattened[start..end].to_string();
    if start > 0 {
        snippet = format!("…{snippet}");
    }
    if end < flattened.len() {
        snippet.push('…');
    }
    Some(snippet)
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
    use crate::embeddings::EmbeddingSpace;
    use crate::providers::ProviderKind;
    use std::{fs, sync::Mutex};

    struct FakeTransport {
        responses: Mutex<Vec<Result<Vec<ChatStreamEvent>, ProviderError>>>,
        requests: Mutex<Vec<ChatRequest>>,
        embeddings: Mutex<Vec<EmbeddingRequest>>,
    }

    impl FakeTransport {
        fn new(responses: Vec<Result<Vec<ChatStreamEvent>, ProviderError>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                requests: Mutex::new(Vec::new()),
                embeddings: Mutex::new(Vec::new()),
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

        async fn embed(
            &self,
            _config: &ProviderConfig,
            request: &EmbeddingRequest,
        ) -> Result<EmbeddingResponse, ProviderError> {
            self.embeddings.lock().unwrap().push(request.clone());
            Ok(EmbeddingResponse {
                model: request.model.clone(),
                dimensions: 2,
                vectors: request.input.iter().map(|_| vec![1.0, 0.0]).collect(),
            })
        }
    }

    fn complete_reply(text: &str) -> Vec<ChatStreamEvent> {
        vec![
            ChatStreamEvent::Delta { text: text.into() },
            ChatStreamEvent::Completed {
                finish_reason: Some("stop".into()),
            },
        ]
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
            application_prompt: String::new(),
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

    #[test]
    fn lists_opens_and_starts_sessions_without_mixing_characters() {
        let root = root("sessions");
        let vault = Vault::create(&root).unwrap();
        vault
            .save_character(&CharacterDefinition::new("lyra", "Lyra", "You are Lyra."))
            .unwrap();

        let mut older = TranscriptDocument::new("session-older", "lyra");
        older.created_at = "100".into();
        older.updated_at = "100".into();
        older.turns.push(TranscriptTurn {
            id: "user-old".into(),
            timestamp: "100".into(),
            role: TurnRole::User,
            status: TurnStatus::Complete,
            content: "Earlier talk".into(),
        });
        vault.save_transcript(&older).unwrap();

        let mut newer = TranscriptDocument::new("session-newer", "lyra");
        newer.created_at = "200".into();
        newer.updated_at = "200".into();
        newer.turns.push(TranscriptTurn {
            id: "user-new".into(),
            timestamp: "200".into(),
            role: TurnRole::User,
            status: TurnStatus::Complete,
            content: "Latest talk".into(),
        });
        vault.save_transcript(&newer).unwrap();

        let mut foreign = TranscriptDocument::new("session-foreign", "nova");
        foreign.created_at = "300".into();
        foreign.updated_at = "300".into();
        foreign.turns.push(TranscriptTurn {
            id: "user-foreign".into(),
            timestamp: "300".into(),
            role: TurnRole::User,
            status: TurnStatus::Complete,
            content: "Wrong character".into(),
        });
        vault.save_transcript(&foreign).unwrap();

        let listed = ConversationService::list_sessions(&vault).unwrap();
        assert_eq!(
            listed
                .iter()
                .map(|session| session.session_id.as_str())
                .collect::<Vec<_>>(),
            vec!["session-newer", "session-older"]
        );
        assert_eq!(listed[0].preview, "Latest talk");
        assert_eq!(listed[0].turn_count, 1);

        let opened = ConversationService::open_session(&vault, "session-older").unwrap();
        assert_eq!(opened.transcript.unwrap().session_id, "session-older");
        assert_eq!(
            vault
                .load_state()
                .unwrap()
                .last_opened_session_id
                .as_deref(),
            Some("session-older")
        );

        let started = ConversationService::start_session(&vault).unwrap();
        let started_id = started.transcript.as_ref().unwrap().session_id.clone();
        assert_ne!(started_id, "session-older");
        assert_eq!(started.transcript.as_ref().unwrap().turns.len(), 0);
        assert_eq!(
            vault
                .load_state()
                .unwrap()
                .last_opened_session_id
                .as_deref(),
            Some(started_id.as_str())
        );
        let reopened = ConversationService::open_session(&vault, &started_id).unwrap();
        assert_eq!(reopened.transcript.unwrap().turns.len(), 0);
        assert_eq!(ConversationService::list_sessions(&vault).unwrap().len(), 2);
        assert!(matches!(
            ConversationService::open_session(&vault, "session-foreign"),
            Err(ConversationError::Invalid(message)) if message.contains("different character")
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
                local: None,
                title: String::new(),
                archived: false,
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
            application_prompt: String::new(),
        }
    }

    #[tokio::test]
    async fn application_prompt_is_the_first_system_message() {
        let root = root("application-prompt");
        let vault = Vault::create(&root).unwrap();
        let fake = Arc::new(FakeTransport::new(vec![Ok(vec![
            ChatStreamEvent::Delta {
                text: "Hello back".into(),
            },
            ChatStreamEvent::Completed {
                finish_reason: Some("stop".into()),
            },
        ])]));
        let service = ConversationService::new(fake.clone());
        let mut request = snapshot("session-1", "lyra", "user-1", "Hello");
        request.application_prompt = "  Application rules first.  ".into();
        service
            .send(&vault, request, CancellationToken::new())
            .await
            .unwrap();
        let captured = fake.requests.lock().unwrap()[0].clone();
        assert_eq!(captured.messages[0].content, "Application rules first.");
        assert!(captured.messages[1].content.contains("You are lyra."));
        fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn send_injects_persona_and_live_linked_session_local() {
        let root = root("persona-scene");
        let vault = Vault::create(&root).unwrap();
        vault
            .save_character(&CharacterDefinition::new("lyra", "Lyra", "You are lyra."))
            .unwrap();
        crate::scene::save_persona(
            &vault,
            &crate::scene::PersonaNotes {
                schema_version: crate::storage::STORAGE_SCHEMA_VERSION,
                body: "I am Alex.".into(),
            },
        )
        .unwrap();
        crate::scene::save_local(
            &vault,
            &crate::scene::LocalRecord {
                schema_version: crate::storage::STORAGE_SCHEMA_VERSION,
                id: "cafe".into(),
                title: "Evening cafe".into(),
                body: "Rain on the windows.".into(),
            },
        )
        .unwrap();
        crate::scene::save_scene_settings(
            &vault,
            &crate::scene::SceneSettings {
                schema_version: crate::storage::STORAGE_SCHEMA_VERSION,
                default_local: Some("cafe".into()),
            },
        )
        .unwrap();
        let fake = Arc::new(FakeTransport::new(vec![
            Ok(complete_reply("Hello back")),
            Ok(complete_reply("Still here")),
        ]));
        let service = ConversationService::new(fake.clone());
        let first = snapshot("session-1", "lyra", "user-1", "Hello");
        service
            .send(&vault, first, CancellationToken::new())
            .await
            .unwrap();
        let first_request = fake.requests.lock().unwrap()[0].clone();
        assert!(first_request
            .messages
            .iter()
            .any(|message| { message.content == "User persona (who the user is):\nI am Alex." }));
        assert!(first_request.messages.iter().any(|message| {
            message.content == "Current scene (Evening cafe):\nRain on the windows."
        }));

        ConversationService::set_session_local(
            &vault,
            "session-1",
            crate::scene::SessionLocalSelection::Local { id: "cafe".into() },
        )
        .unwrap();
        crate::scene::save_local(
            &vault,
            &crate::scene::LocalRecord {
                schema_version: crate::storage::STORAGE_SCHEMA_VERSION,
                id: "cafe".into(),
                title: "Evening cafe".into(),
                body: "The cafe is closing.".into(),
            },
        )
        .unwrap();
        let second = snapshot("session-1", "lyra", "user-2", "Still raining?");
        service
            .send(&vault, second, CancellationToken::new())
            .await
            .unwrap();
        let second_request = fake.requests.lock().unwrap()[1].clone();
        assert!(second_request.messages.iter().any(|message| {
            message.content == "Current scene (Evening cafe):\nThe cafe is closing."
        }));
        fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn send_fails_when_session_local_is_missing() {
        let root = root("missing-local-send");
        let vault = Vault::create(&root).unwrap();
        let mut transcript = crate::storage::TranscriptDocument::new("session-1", "lyra");
        transcript.created_at = "1".into();
        transcript.updated_at = "1".into();
        transcript.local = Some("cafe".into());
        vault.save_transcript(&transcript).unwrap();
        let fake = Arc::new(FakeTransport::new(vec![Ok(complete_reply("Hello back"))]));
        let service = ConversationService::new(fake);
        let error = service
            .send(
                &vault,
                snapshot("session-1", "lyra", "user-1", "Hello"),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("add it to locals"));
        fs::remove_dir_all(root).ok();
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

    fn hybrid_snapshot(session_id: &str, content: &str) -> RequestSnapshot {
        let mut request = snapshot(session_id, "lyra", "user-hybrid", content);
        request.use_hybrid_retrieval = true;
        request.provider.embedding_model = Some("nomic-embed".into());
        request
    }

    async fn send_hybrid(
        vault: &Vault,
        fake: Arc<FakeTransport>,
        request: RequestSnapshot,
    ) -> ConversationOutcome {
        ConversationService::new(fake)
            .send(vault, request, CancellationToken::new())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn hybrid_used_when_embedding_model_and_flag_set() {
        let root = root("hybrid-ready");
        let vault = Vault::create(&root).unwrap();
        let mut store = MemoryStore::open(&vault, "lyra").unwrap();
        store
            .create(&crate::storage::MemoryRecord::new(
                "tea",
                crate::storage::MemoryType::Semantic,
                "Mina drinks tea.",
            ))
            .unwrap();
        let memories = store.list().unwrap();
        let chunks = chunks_from_memories(&memories);
        let index = EmbeddingIndex::open(
            &vault,
            "lyra",
            EmbeddingSpace::new("fake", "nomic-embed", 2),
        )
        .unwrap();
        let vectors = chunks.iter().map(|_| vec![1.0, 0.0]).collect::<Vec<_>>();
        index.rebuild_precomputed(&chunks, &vectors).unwrap();
        drop(index);
        drop(store);
        let fake = Arc::new(FakeTransport::new(vec![Ok(complete_reply("Hello back"))]));
        let outcome = send_hybrid(&vault, fake.clone(), hybrid_snapshot("session", "Hello")).await;
        assert_eq!(outcome.context_inspection.retrieval_mode, "hybrid");
        assert_eq!(outcome.context_inspection.fallback_reason, None);
        assert!(!fake.embeddings.lock().unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn lexical_fallback_when_index_needs_rebuild_does_not_embed() {
        let root = root("hybrid-rebuild");
        let vault = Vault::create(&root).unwrap();
        let mut store = crate::memory::MemoryStore::open(&vault, "lyra").unwrap();
        store
            .create(&crate::storage::MemoryRecord::new(
                "tea",
                crate::storage::MemoryType::Semantic,
                "Mina drinks tea.",
            ))
            .unwrap();
        drop(store);
        let fake = Arc::new(FakeTransport::new(vec![Ok(complete_reply("Noted"))]));
        let outcome = send_hybrid(
            &vault,
            fake.clone(),
            hybrid_snapshot("session", "Do you remember the tea?"),
        )
        .await;
        assert_eq!(outcome.context_inspection.retrieval_mode, "lexical");
        assert!(outcome
            .context_inspection
            .fallback_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("rebuilding")));
        assert!(
            fake.embeddings.lock().unwrap().is_empty(),
            "chat path must not embed while the index needs rebuild"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn lexical_when_no_embedding_model() {
        let root = root("hybrid-no-model");
        let vault = Vault::create(&root).unwrap();
        let fake = Arc::new(FakeTransport::new(vec![Ok(complete_reply("Hello back"))]));
        let mut request = snapshot("session", "lyra", "user-1", "Hello");
        request.use_hybrid_retrieval = true;
        request.provider.embedding_model = None;
        let outcome = send_hybrid(&vault, fake.clone(), request).await;
        assert_eq!(outcome.context_inspection.retrieval_mode, "lexical");
        assert_eq!(
            outcome.context_inspection.fallback_reason.as_deref(),
            Some("Hybrid retrieval requested without an embedding model; lexical fallback used.")
        );
        assert!(fake.embeddings.lock().unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn auto_titles_from_first_user_turn_and_keeps_renames() {
        let root = root("titles");
        let vault = Vault::create(&root).unwrap();
        vault
            .save_character(&CharacterDefinition::new("lyra", "Lyra", "You are Lyra."))
            .unwrap();
        let fake = Arc::new(FakeTransport::new(vec![
            Ok(complete_reply("Hi")),
            Ok(complete_reply("Still here")),
        ]));
        let service = ConversationService::new(fake);
        service
            .send(
                &vault,
                snapshot(
                    "session-1",
                    "lyra",
                    "user-1",
                    "Let's talk about Markdown notes",
                ),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let listed = ConversationService::list_sessions(&vault).unwrap();
        assert_eq!(listed[0].title, "Let's talk about Markdown notes");
        assert_eq!(
            vault.load_transcript("session-1").unwrap().title,
            "Let's talk about Markdown notes"
        );

        ConversationService::rename_session(&vault, "session-1", "Vault notes").unwrap();
        service
            .send(
                &vault,
                snapshot("session-1", "lyra", "user-2", "Another message"),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(
            vault.load_transcript("session-1").unwrap().title,
            "Vault notes"
        );
        assert_eq!(
            ConversationService::list_sessions(&vault).unwrap()[0].title,
            "Vault notes"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn archive_hides_from_default_list_and_keeps_markdown() {
        let root = root("archive");
        let vault = Vault::create(&root).unwrap();
        vault
            .save_character(&CharacterDefinition::new("lyra", "Lyra", "You are Lyra."))
            .unwrap();
        let mut transcript = TranscriptDocument::new("session-keep", "lyra");
        transcript.created_at = "1".into();
        transcript.updated_at = "1".into();
        transcript.turns.push(TranscriptTurn {
            id: "user-1".into(),
            timestamp: "1".into(),
            role: TurnRole::User,
            status: TurnStatus::Complete,
            content: "Keep this chat".into(),
        });
        vault.save_transcript(&transcript).unwrap();

        ConversationService::set_session_archived(&vault, "session-keep", true).unwrap();
        assert!(ConversationService::list_sessions(&vault)
            .unwrap()
            .is_empty());
        let with_archived = ConversationService::list_sessions_filtered(&vault, true).unwrap();
        assert_eq!(with_archived.len(), 1);
        assert!(with_archived[0].archived);
        let path = vault.transcript_path("session-keep").unwrap();
        assert!(path.exists());
        assert_eq!(path.file_name().unwrap(), "session-keep.md");
        assert!(vault.load_transcript("session-keep").unwrap().archived);

        ConversationService::set_session_archived(&vault, "session-keep", false).unwrap();
        let restored = ConversationService::list_sessions(&vault).unwrap();
        assert_eq!(restored.len(), 1);
        assert!(!restored[0].archived);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn search_matches_title_and_body_including_archived_and_isolates_characters() {
        let root = root("search");
        let vault = Vault::create(&root).unwrap();
        vault
            .save_character(&CharacterDefinition::new("lyra", "Lyra", "You are Lyra."))
            .unwrap();

        let mut cafe = TranscriptDocument::new("session-cafe", "lyra");
        cafe.created_at = "1".into();
        cafe.updated_at = "1".into();
        cafe.turns.push(TranscriptTurn {
            id: "user-cafe".into(),
            timestamp: "1".into(),
            role: TurnRole::User,
            status: TurnStatus::Complete,
            content: "Mina sketches in a quiet cafe on Sundays.".into(),
        });
        vault.save_transcript(&cafe).unwrap();

        let mut garden = TranscriptDocument::new("session-garden", "lyra");
        garden.created_at = "2".into();
        garden.updated_at = "2".into();
        garden.title = "Garden walk".into();
        garden.turns.push(TranscriptTurn {
            id: "user-garden".into(),
            timestamp: "2".into(),
            role: TurnRole::User,
            status: TurnStatus::Complete,
            content: "We talked about roses.".into(),
        });
        vault.save_transcript(&garden).unwrap();

        let mut foreign = TranscriptDocument::new("session-foreign", "nova");
        foreign.created_at = "3".into();
        foreign.updated_at = "3".into();
        foreign.turns.push(TranscriptTurn {
            id: "user-foreign".into(),
            timestamp: "3".into(),
            role: TurnRole::User,
            status: TurnStatus::Complete,
            content: "Mina sketches in a quiet cafe on Sundays.".into(),
        });
        vault.save_transcript(&foreign).unwrap();

        let cafe_hits = ConversationService::search_sessions(&vault, "cafe").unwrap();
        assert_eq!(
            cafe_hits
                .iter()
                .map(|session| session.session_id.as_str())
                .collect::<Vec<_>>(),
            vec!["session-cafe"]
        );
        assert!(cafe_hits[0]
            .snippet
            .as_deref()
            .is_some_and(|snippet| snippet.to_lowercase().contains("cafe")));

        let title_hits = ConversationService::search_sessions(&vault, "Garden").unwrap();
        assert_eq!(title_hits[0].session_id, "session-garden");

        ConversationService::set_session_archived(&vault, "session-cafe", true).unwrap();
        let archived_hits = ConversationService::search_sessions(&vault, "cafe").unwrap();
        assert_eq!(archived_hits.len(), 1);
        assert!(archived_hits[0].archived);
        fs::remove_dir_all(root).unwrap();
    }
}
