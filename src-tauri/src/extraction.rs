use crate::{
    providers::{ChatMessage, ChatRequest, ChatRole},
    storage::{
        new_stable_id, CharacterDefinition, MemoryType, TranscriptDocument, TurnRole, TurnStatus,
        Vault,
    },
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_MAX_ATTEMPTS: u32 = 3;
pub const EXTRACTION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionJobStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalOrigin {
    UserStated,
    CharacterFact,
    ConversationEvent,
    Inferred,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalRelation {
    Contradicts,
    Supersedes,
    Related,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    NeedsReview,
    Accepted,
    Rejected,
    Superseded,
    Committed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProposalEvidence {
    pub turn_id: String,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtractionCandidate {
    pub memory_type: MemoryType,
    pub body: String,
    pub source_turn_ids: Vec<String>,
    pub evidence: Vec<ProposalEvidence>,
    pub confidence: f32,
    pub origin: ProposalOrigin,
    #[serde(default)]
    pub related_memory_ids: Vec<String>,
    #[serde(default)]
    pub relation: Option<ProposalRelation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtractionResponse {
    pub schema_version: u32,
    pub proposals: Vec<ExtractionCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtractionJob {
    pub id: String,
    pub character_id: String,
    pub session_id: String,
    pub source_turn_ids: Vec<String>,
    pub priority: i32,
    pub status: ExtractionJobStatus,
    pub attempts: u32,
    pub max_attempts: u32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryProposal {
    pub id: String,
    pub job_id: String,
    pub character_id: String,
    pub source_session_id: String,
    pub candidate: ExtractionCandidate,
    pub status: ProposalStatus,
}

#[derive(Debug)]
pub enum ExtractionError {
    Storage(String),
    InvalidInput(String),
    Parse(String),
}

impl fmt::Display for ExtractionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "storage error: {error}"),
            Self::InvalidInput(error) => write!(formatter, "invalid extraction input: {error}"),
            Self::Parse(error) => write!(formatter, "extraction parse error: {error}"),
        }
    }
}

impl std::error::Error for ExtractionError {}

impl From<rusqlite::Error> for ExtractionError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Storage(error.to_string())
    }
}

pub struct ExtractionQueue {
    connection: Connection,
    character_id: String,
}

impl ExtractionQueue {
    pub fn open(vault: &Vault, character_id: &str) -> Result<Self, ExtractionError> {
        let state_dir = vault.root().join(".tz-chatter");
        std::fs::create_dir_all(&state_dir)
            .map_err(|error| ExtractionError::Storage(error.to_string()))?;
        let connection = Connection::open(state_dir.join("state.sqlite3"))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS extraction_jobs (
                 id TEXT PRIMARY KEY,
                 character_id TEXT NOT NULL,
                 session_id TEXT NOT NULL,
                 source_turn_ids TEXT NOT NULL,
                 priority INTEGER NOT NULL,
                 status TEXT NOT NULL,
                 attempts INTEGER NOT NULL,
                 max_attempts INTEGER NOT NULL,
                 last_error TEXT,
                 created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL,
                 UNIQUE(character_id, session_id, source_turn_ids)
             );
             CREATE TABLE IF NOT EXISTS memory_proposals (
                 id TEXT PRIMARY KEY,
                 job_id TEXT NOT NULL REFERENCES extraction_jobs(id),
                 character_id TEXT NOT NULL,
                 source_session_id TEXT NOT NULL,
                 candidate_json TEXT NOT NULL,
                 status TEXT NOT NULL,
                 created_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS memory_relationships (
                 from_memory_id TEXT NOT NULL,
                 to_memory_id TEXT NOT NULL,
                 relation TEXT NOT NULL,
                 proposal_id TEXT NOT NULL,
                 created_at TEXT NOT NULL,
                 PRIMARY KEY (from_memory_id, to_memory_id, relation)
             );
             CREATE TABLE IF NOT EXISTS memory_suppressions (
                 character_id TEXT NOT NULL,
                 source_session_id TEXT NOT NULL,
                 source_turn_id TEXT NOT NULL,
                 memory_type TEXT NOT NULL,
                 reason TEXT NOT NULL,
                 created_at TEXT NOT NULL,
                 PRIMARY KEY (character_id, source_session_id, source_turn_id, memory_type)
             );",
        )?;
        connection.execute(
            "UPDATE extraction_jobs
             SET status = 'pending',
                 attempts = CASE WHEN attempts > 0 THEN attempts - 1 ELSE 0 END,
                 last_error = ?1,
                 updated_at = ?2
             WHERE character_id = ?3 AND status = 'running'",
            params!["recovered after application restart", now(), character_id],
        )?;
        Ok(Self {
            connection,
            character_id: character_id.to_owned(),
        })
    }

    pub fn enqueue_transcript(
        &mut self,
        transcript: &TranscriptDocument,
        assistant_turn_id: &str,
        priority: i32,
    ) -> Result<ExtractionJob, ExtractionError> {
        if transcript.character_id != self.character_id {
            return Err(ExtractionError::InvalidInput(
                "transcript belongs to another character".into(),
            ));
        }
        let assistant_index = transcript
            .turns
            .iter()
            .position(|turn| turn.id == assistant_turn_id)
            .ok_or_else(|| ExtractionError::InvalidInput("source turn does not exist".into()))?;
        let assistant = &transcript.turns[assistant_index];
        if assistant.role != TurnRole::Assistant || assistant.status != TurnStatus::Complete {
            return Err(ExtractionError::InvalidInput(
                "only completed assistant turns can be extracted".into(),
            ));
        }
        let user_turn_id = transcript.turns[..assistant_index]
            .iter()
            .rev()
            .find(|turn| turn.role == TurnRole::User && turn.status == TurnStatus::Complete)
            .map(|turn| turn.id.clone());
        let mut source_turn_ids = user_turn_id.into_iter().collect::<Vec<_>>();
        source_turn_ids.push(assistant.id.clone());
        let source_turn_ids_json = serde_json::to_string(&source_turn_ids)
            .map_err(|error| ExtractionError::Storage(error.to_string()))?;
        let timestamp = now();
        let id = new_stable_id();
        self.connection.execute(
            "INSERT OR IGNORE INTO extraction_jobs
             (id, character_id, session_id, source_turn_ids, priority, status, attempts, max_attempts, last_error, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending', 0, ?6, NULL, ?7, ?7)",
            params![
                id,
                self.character_id,
                transcript.session_id,
                source_turn_ids_json,
                priority,
                DEFAULT_MAX_ATTEMPTS,
                timestamp
            ],
        )?;
        let job_id = self
            .connection
            .query_row(
                "SELECT id FROM extraction_jobs WHERE character_id = ?1 AND session_id = ?2 AND source_turn_ids = ?3",
                params![self.character_id, transcript.session_id, source_turn_ids_json],
                |row| row.get::<_, String>(0),
            )?;
        self.get_job(&job_id)?.ok_or_else(|| {
            ExtractionError::Storage("inserted extraction job could not be reloaded".into())
        })
    }

    pub fn claim_next(&mut self) -> Result<Option<ExtractionJob>, ExtractionError> {
        let candidate = self
            .connection
            .query_row(
                "SELECT id FROM extraction_jobs
             WHERE character_id = ?1 AND status = 'pending' AND attempts < max_attempts
             ORDER BY priority DESC, created_at ASC LIMIT 1",
                params![self.character_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(id) = candidate else {
            return Ok(None);
        };
        self.connection.execute(
            "UPDATE extraction_jobs SET status = 'running', attempts = attempts + 1, updated_at = ?1
             WHERE id = ?2 AND status = 'pending'",
            params![now(), id],
        )?;
        self.get_job(&id)
    }

    pub fn fail(
        &mut self,
        job_id: &str,
        error: &str,
    ) -> Result<Option<ExtractionJob>, ExtractionError> {
        let job = self
            .get_job(job_id)?
            .ok_or_else(|| ExtractionError::InvalidInput("job not found".into()))?;
        if job.status != ExtractionJobStatus::Running {
            return Err(ExtractionError::InvalidInput(
                "only a running job can fail".into(),
            ));
        }
        let status = if job.attempts >= job.max_attempts {
            "failed"
        } else {
            "pending"
        };
        self.connection.execute(
            "UPDATE extraction_jobs SET status = ?1, last_error = ?2, updated_at = ?3 WHERE id = ?4",
            params![status, error, now(), job_id],
        )?;
        self.get_job(job_id)
    }

    pub fn defer(
        &mut self,
        job_id: &str,
        reason: &str,
    ) -> Result<Option<ExtractionJob>, ExtractionError> {
        let job = self
            .get_job(job_id)?
            .ok_or_else(|| ExtractionError::InvalidInput("job not found".into()))?;
        if job.status != ExtractionJobStatus::Running {
            return Err(ExtractionError::InvalidInput(
                "only a running job can be deferred".into(),
            ));
        }
        self.connection.execute(
            "UPDATE extraction_jobs
             SET status = 'pending',
                 attempts = CASE WHEN attempts > 0 THEN attempts - 1 ELSE 0 END,
                 last_error = ?1,
                 updated_at = ?2
             WHERE id = ?3",
            params![reason, now(), job_id],
        )?;
        self.get_job(job_id)
    }

    pub fn accept_model_output(
        &mut self,
        job_id: &str,
        raw_output: &str,
    ) -> Result<Vec<MemoryProposal>, ExtractionError> {
        let job = self
            .get_job(job_id)?
            .ok_or_else(|| ExtractionError::InvalidInput("job not found".into()))?;
        if job.status != ExtractionJobStatus::Running {
            return Err(ExtractionError::InvalidInput(
                "only a running job can accept output".into(),
            ));
        }
        let response = match parse_response(raw_output, &job.source_turn_ids) {
            Ok(response) => response,
            Err(error) => {
                self.fail(job_id, &error.to_string())?;
                return Err(error);
            }
        };
        let transaction = self.connection.transaction()?;
        let mut proposals = Vec::new();
        for candidate in response.proposals {
            let proposal = MemoryProposal {
                id: new_stable_id(),
                job_id: job.id.clone(),
                character_id: self.character_id.clone(),
                source_session_id: job.session_id.clone(),
                candidate,
                status: ProposalStatus::NeedsReview,
            };
            let candidate_json = serde_json::to_string(&proposal.candidate)
                .map_err(|error| ExtractionError::Storage(error.to_string()))?;
            transaction.execute(
                "INSERT INTO memory_proposals (id, job_id, character_id, source_session_id, candidate_json, status, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'needs_review', ?6)",
                params![proposal.id, proposal.job_id, proposal.character_id, proposal.source_session_id, candidate_json, now()],
            )?;
            proposals.push(proposal);
        }
        transaction.execute(
            "UPDATE extraction_jobs SET status = 'succeeded', last_error = NULL, updated_at = ?1 WHERE id = ?2",
            params![now(), job_id],
        )?;
        transaction.commit()?;
        Ok(proposals)
    }

    pub fn reclassify_proposal(
        &self,
        proposal_id: &str,
        origin: ProposalOrigin,
    ) -> Result<MemoryProposal, ExtractionError> {
        let mut proposal = self
            .get_proposal(proposal_id)?
            .ok_or_else(|| ExtractionError::InvalidInput("proposal not found".into()))?;
        if proposal.status != ProposalStatus::NeedsReview {
            return Err(ExtractionError::InvalidInput(
                "only a pending proposal can be reclassified".into(),
            ));
        }
        proposal.candidate.origin = origin;
        let candidate_json = serde_json::to_string(&proposal.candidate)
            .map_err(|error| ExtractionError::Storage(error.to_string()))?;
        self.connection.execute(
            "UPDATE memory_proposals SET candidate_json = ?1 WHERE id = ?2 AND character_id = ?3",
            params![candidate_json, proposal_id, self.character_id],
        )?;
        Ok(proposal)
    }

    pub fn pending_proposals(&self) -> Result<Vec<MemoryProposal>, ExtractionError> {
        let mut statement = self.connection.prepare(
            "SELECT id, job_id, character_id, source_session_id, candidate_json, status
             FROM memory_proposals WHERE character_id = ?1 AND status = 'needs_review' ORDER BY created_at ASC",
        )?;
        let rows = statement.query_map(params![self.character_id], |row| {
            let candidate: ExtractionCandidate = serde_json::from_str(&row.get::<_, String>(4)?)
                .map_err(|_| rusqlite::Error::InvalidQuery)?;
            Ok(MemoryProposal {
                id: row.get(0)?,
                job_id: row.get(1)?,
                character_id: row.get(2)?,
                source_session_id: row.get(3)?,
                candidate,
                status: proposal_status(&row.get::<_, String>(5)?)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(ExtractionError::from)
    }

    pub fn get_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<MemoryProposal>, ExtractionError> {
        self.connection
            .query_row(
                "SELECT id, job_id, character_id, source_session_id, candidate_json, status
             FROM memory_proposals WHERE id = ?1 AND character_id = ?2",
                params![proposal_id, self.character_id],
                |row| {
                    let candidate: ExtractionCandidate =
                        serde_json::from_str(&row.get::<_, String>(4)?)
                            .map_err(|_| rusqlite::Error::InvalidQuery)?;
                    Ok(MemoryProposal {
                        id: row.get(0)?,
                        job_id: row.get(1)?,
                        character_id: row.get(2)?,
                        source_session_id: row.get(3)?,
                        candidate,
                        status: proposal_status(&row.get::<_, String>(5)?)?,
                    })
                },
            )
            .optional()
            .map_err(ExtractionError::from)
    }

    pub fn edit_proposal(
        &self,
        proposal_id: &str,
        body: &str,
        confidence: f32,
    ) -> Result<MemoryProposal, ExtractionError> {
        let mut proposal = self
            .get_proposal(proposal_id)?
            .ok_or_else(|| ExtractionError::InvalidInput("proposal not found".into()))?;
        if proposal.status != ProposalStatus::NeedsReview {
            return Err(ExtractionError::InvalidInput(
                "only a pending proposal can be edited".into(),
            ));
        }
        if body.trim().is_empty()
            || body.len() > 4000
            || !confidence.is_finite()
            || !(0.0..=1.0).contains(&confidence)
        {
            return Err(ExtractionError::InvalidInput(
                "proposal body or confidence is invalid".into(),
            ));
        }
        proposal.candidate.body = body.to_owned();
        proposal.candidate.confidence = confidence;
        let candidate_json = serde_json::to_string(&proposal.candidate)
            .map_err(|error| ExtractionError::Storage(error.to_string()))?;
        self.connection.execute(
            "UPDATE memory_proposals SET candidate_json = ?1 WHERE id = ?2 AND character_id = ?3",
            params![candidate_json, proposal_id, self.character_id],
        )?;
        Ok(proposal)
    }

    pub fn mark_proposal(
        &self,
        proposal_id: &str,
        status: ProposalStatus,
    ) -> Result<(), ExtractionError> {
        let changed = self.connection.execute(
            "UPDATE memory_proposals SET status = ?1 WHERE id = ?2 AND character_id = ?3",
            params![
                proposal_status_label(&status),
                proposal_id,
                self.character_id
            ],
        )?;
        if changed == 0 {
            return Err(ExtractionError::InvalidInput("proposal not found".into()));
        }
        Ok(())
    }

    pub fn suppress_memory(
        &self,
        memory: &crate::storage::MemoryRecord,
        reason: &str,
    ) -> Result<(), ExtractionError> {
        let Some(session_id) = memory.source_session_id.as_deref() else {
            return Ok(());
        };
        let memory_type = memory_type_label(&memory.memory_type);
        for turn_id in &memory.source_turn_ids {
            self.connection.execute(
                "INSERT OR REPLACE INTO memory_suppressions (character_id, source_session_id, source_turn_id, memory_type, reason, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![self.character_id, session_id, turn_id, memory_type, reason, now()],
            )?;
        }
        Ok(())
    }

    pub fn is_suppressed(
        &self,
        session_id: &str,
        turn_ids: &[String],
        memory_type: &MemoryType,
    ) -> Result<bool, ExtractionError> {
        let memory_type = memory_type_label(memory_type);
        for turn_id in turn_ids {
            let found: Option<i64> = self.connection.query_row(
                "SELECT 1 FROM memory_suppressions WHERE character_id = ?1 AND source_session_id = ?2 AND source_turn_id = ?3 AND memory_type = ?4",
                params![self.character_id, session_id, turn_id, memory_type],
                |row| row.get(0),
            ).optional()?;
            if found.is_some() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn superseded_targets(&self) -> Result<Vec<String>, ExtractionError> {
        let mut statement = self.connection.prepare(
            "SELECT DISTINCT to_memory_id FROM memory_relationships WHERE relation = 'supersedes' ORDER BY to_memory_id",
        )?;
        let rows = statement.query_map([], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(ExtractionError::from)
    }

    pub fn record_relationship(
        &self,
        from_memory_id: &str,
        to_memory_id: &str,
        relation: &ProposalRelation,
        proposal_id: &str,
    ) -> Result<(), ExtractionError> {
        self.connection.execute(
            "INSERT OR IGNORE INTO memory_relationships (from_memory_id, to_memory_id, relation, proposal_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![from_memory_id, to_memory_id, proposal_relation_label(relation), proposal_id, now()],
        )?;
        Ok(())
    }

    pub fn relationships_for(
        &self,
        memory_id: &str,
    ) -> Result<Vec<(String, String, String)>, ExtractionError> {
        let mut statement = self.connection.prepare(
            "SELECT to_memory_id, relation, proposal_id FROM memory_relationships WHERE from_memory_id = ?1 ORDER BY to_memory_id",
        )?;
        let rows = statement.query_map(params![memory_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(ExtractionError::from)
    }

    fn get_job(&self, id: &str) -> Result<Option<ExtractionJob>, ExtractionError> {
        self.connection
            .query_row(
                "SELECT id, character_id, session_id, source_turn_ids, priority, status, attempts, max_attempts, last_error
                 FROM extraction_jobs WHERE id = ?1 AND character_id = ?2",
                params![id, self.character_id],
                |row| {
                    let status = match row.get::<_, String>(5)?.as_str() {
                        "pending" => ExtractionJobStatus::Pending,
                        "running" => ExtractionJobStatus::Running,
                        "succeeded" => ExtractionJobStatus::Succeeded,
                        "failed" => ExtractionJobStatus::Failed,
                        _ => return Err(rusqlite::Error::InvalidQuery),
                    };
                    let source_turn_ids = serde_json::from_str(&row.get::<_, String>(3)?)
                        .map_err(|_| rusqlite::Error::InvalidQuery)?;
                    Ok(ExtractionJob { id: row.get(0)?, character_id: row.get(1)?, session_id: row.get(2)?, source_turn_ids, priority: row.get(4)?, status, attempts: row.get(6)?, max_attempts: row.get(7)?, last_error: row.get(8)? })
                },
            )
            .optional()
            .map_err(ExtractionError::from)
    }
}

pub fn effective_origin(
    candidate: &ExtractionCandidate,
    transcript: &TranscriptDocument,
) -> ProposalOrigin {
    match candidate.origin {
        ProposalOrigin::Inferred => ProposalOrigin::Inferred,
        ProposalOrigin::UserStated => {
            let quotes_user = candidate.evidence.iter().any(|evidence| {
                transcript
                    .turns
                    .iter()
                    .any(|turn| turn.id == evidence.turn_id && turn.role == TurnRole::User)
            });
            if quotes_user {
                ProposalOrigin::UserStated
            } else {
                ProposalOrigin::Inferred
            }
        }
        ProposalOrigin::ConversationEvent | ProposalOrigin::CharacterFact => {
            candidate.origin.clone()
        }
    }
}

pub fn is_auto_writable(origin: &ProposalOrigin) -> bool {
    matches!(
        origin,
        ProposalOrigin::UserStated
            | ProposalOrigin::ConversationEvent
            | ProposalOrigin::CharacterFact
    )
}

/// Build a bounded, data-labelled request for the selected provider. The model's
/// response remains untrusted and must pass accept_model_output before any
/// proposal is persisted.
pub fn build_extraction_request(
    character: &CharacterDefinition,
    transcript: &TranscriptDocument,
    job: &ExtractionJob,
) -> ChatRequest {
    let source_turns = transcript
        .turns
        .iter()
        .filter(|turn| job.source_turn_ids.contains(&turn.id))
        .map(|turn| {
            let role = match turn.role {
                TurnRole::User => "user",
                TurnRole::Assistant => "assistant",
                TurnRole::Initiative => "initiative",
                TurnRole::System => "system",
            };
            format!("[{}:{}] {}", role, turn.id, turn.content)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let system = format!(
        "You extract conservative candidate memories for the character {}. Return only a JSON object with schema_version {} and a proposals array. Each proposal must use only the allowed source turn IDs, quote supporting evidence, choose an origin, and never treat assistant speculation as a user-stated fact. If nothing durable is supported, return an empty proposals array. Do not include Markdown fences or commentary.",
        character.id, EXTRACTION_SCHEMA_VERSION
    );
    let user = format!(
        "Allowed source turn IDs: {}\nCharacter summary: {}\nConversation evidence:\n{}",
        job.source_turn_ids.join(", "),
        character.summary,
        source_turns
    );
    ChatRequest {
        provider_id: String::new(),
        model: String::new(),
        messages: vec![
            ChatMessage {
                role: ChatRole::System,
                content: system,
            },
            ChatMessage {
                role: ChatRole::User,
                content: user,
            },
        ],
        cancellation_id: format!("extract-{}", job.id),
    }
}

fn parse_response(
    raw_output: &str,
    allowed_turn_ids: &[String],
) -> Result<ExtractionResponse, ExtractionError> {
    let trimmed = raw_output.trim();
    let without_fence = trimmed
        .strip_prefix("```")
        .and_then(|value| value.find('\n').map(|index| &value[index + 1..]))
        .unwrap_or(trimmed);
    let without_fence = without_fence
        .strip_suffix("```")
        .map(str::trim)
        .unwrap_or(without_fence.trim());
    let json = if serde_json::from_str::<ExtractionResponse>(without_fence).is_ok() {
        without_fence.to_owned()
    } else {
        let start = without_fence
            .find('{')
            .ok_or_else(|| ExtractionError::Parse("output did not contain a JSON object".into()))?;
        let end = without_fence.rfind('}').ok_or_else(|| {
            ExtractionError::Parse("output did not contain a complete JSON object".into())
        })?;
        without_fence[start..=end].to_owned()
    };
    let response: ExtractionResponse =
        serde_json::from_str(&json).map_err(|error| ExtractionError::Parse(error.to_string()))?;
    if response.schema_version != EXTRACTION_SCHEMA_VERSION {
        return Err(ExtractionError::Parse(
            "unsupported extraction schema version".into(),
        ));
    }
    for candidate in &response.proposals {
        if candidate.body.trim().is_empty() || candidate.body.len() > 4000 {
            return Err(ExtractionError::InvalidInput(
                "proposal body must contain 1-4000 characters".into(),
            ));
        }
        if !candidate.confidence.is_finite() || !(0.0..=1.0).contains(&candidate.confidence) {
            return Err(ExtractionError::InvalidInput(
                "proposal confidence must be between 0 and 1".into(),
            ));
        }
        if candidate.source_turn_ids.is_empty()
            || candidate
                .source_turn_ids
                .iter()
                .any(|id| !allowed_turn_ids.contains(id))
        {
            return Err(ExtractionError::InvalidInput(
                "proposal references an unavailable source turn".into(),
            ));
        }
        if candidate
            .evidence
            .iter()
            .any(|evidence| !candidate.source_turn_ids.contains(&evidence.turn_id))
        {
            return Err(ExtractionError::InvalidInput(
                "proposal evidence references an unlisted source turn".into(),
            ));
        }
    }
    Ok(response)
}

fn now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    seconds.to_string()
}

fn proposal_status(value: &str) -> rusqlite::Result<ProposalStatus> {
    match value {
        "needs_review" => Ok(ProposalStatus::NeedsReview),
        "accepted" => Ok(ProposalStatus::Accepted),
        "rejected" => Ok(ProposalStatus::Rejected),
        "superseded" => Ok(ProposalStatus::Superseded),
        "committed" => Ok(ProposalStatus::Committed),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

fn proposal_status_label(status: &ProposalStatus) -> &'static str {
    match status {
        ProposalStatus::NeedsReview => "needs_review",
        ProposalStatus::Accepted => "accepted",
        ProposalStatus::Rejected => "rejected",
        ProposalStatus::Superseded => "superseded",
        ProposalStatus::Committed => "committed",
    }
}

fn memory_type_label(memory_type: &MemoryType) -> &'static str {
    match memory_type {
        MemoryType::People => "people",
        MemoryType::Episodic => "episodic",
        MemoryType::Semantic => "semantic",
        MemoryType::Relationships => "relationships",
        MemoryType::OpenThreads => "open_threads",
    }
}

fn proposal_relation_label(relation: &ProposalRelation) -> &'static str {
    match relation {
        ProposalRelation::Contradicts => "contradicts",
        ProposalRelation::Supersedes => "supersedes",
        ProposalRelation::Related => "related",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{new_stable_id, TranscriptTurn};
    use std::path::PathBuf;

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("tz-chatter-extraction-{label}-{}", new_stable_id()))
    }

    fn transcript() -> TranscriptDocument {
        let mut transcript = TranscriptDocument::new("session-1", "lyra");
        transcript.turns.push(TranscriptTurn {
            id: "user-1".into(),
            timestamp: "1".into(),
            role: TurnRole::User,
            status: TurnStatus::Complete,
            content: "I like quiet cafes.".into(),
        });
        transcript.turns.push(TranscriptTurn {
            id: "assistant-1".into(),
            timestamp: "2".into(),
            role: TurnRole::Assistant,
            status: TurnStatus::Complete,
            content: "I will remember that.".into(),
        });
        transcript
    }

    #[test]
    fn extraction_prompt_labels_only_allowed_source_turns() {
        let transcript = transcript();
        let job = ExtractionJob {
            id: "job-1".into(),
            character_id: "lyra".into(),
            session_id: "session-1".into(),
            source_turn_ids: vec!["user-1".into(), "assistant-1".into()],
            priority: 100,
            status: ExtractionJobStatus::Running,
            attempts: 1,
            max_attempts: 3,
            last_error: None,
        };
        let mut character = CharacterDefinition::new("lyra", "Lyra", "You are Lyra.");
        character.summary = "A thoughtful local companion.".into();
        let request = build_extraction_request(&character, &transcript, &job);
        assert_eq!(request.cancellation_id, "extract-job-1");
        assert!(request.messages[0].content.contains("schema_version 1"));
        assert!(request.messages[1].content.contains("[user:user-1]"));
        assert!(request.messages[1]
            .content
            .contains("[assistant:assistant-1]"));
        assert!(request.messages[1]
            .content
            .contains("Allowed source turn IDs"));
    }

    fn output(source: &str, origin: &str) -> String {
        format!(
            r#"{{"schema_version":1,"proposals":[{{"memory_type":"semantic","body":"User likes quiet cafes.","source_turn_ids":["{source}"],"evidence":[{{"turn_id":"{source}","quote":"quiet cafes"}}],"confidence":0.8,"origin":"{origin}"}}]}}"#
        )
    }

    #[test]
    fn enqueue_is_idempotent_and_rejects_incomplete_turns() {
        let root = root("enqueue");
        let vault = Vault::create(&root).unwrap();
        let mut queue = ExtractionQueue::open(&vault, "lyra").unwrap();
        let transcript = transcript();
        let first = queue
            .enqueue_transcript(&transcript, "assistant-1", 10)
            .unwrap();
        let second = queue
            .enqueue_transcript(&transcript, "assistant-1", 10)
            .unwrap();
        assert_eq!(first.id, second.id);
        let mut interrupted = transcript;
        interrupted.turns[1].status = TurnStatus::Interrupted;
        assert!(queue
            .enqueue_transcript(&interrupted, "assistant-1", 10)
            .is_err());
        drop(queue);
        fs_remove(&root);
    }

    #[test]
    fn parser_accepts_fenced_json_but_never_auto_promotes_speculation() {
        let root = root("parse");
        let vault = Vault::create(&root).unwrap();
        let mut queue = ExtractionQueue::open(&vault, "lyra").unwrap();
        let job = queue
            .enqueue_transcript(&transcript(), "assistant-1", 10)
            .unwrap();
        let claimed = queue.claim_next().unwrap().unwrap();
        assert_eq!(claimed.id, job.id);
        let proposals = queue
            .accept_model_output(
                &job.id,
                &format!("```json\n{}\n```", output("user-1", "user_stated")),
            )
            .unwrap();
        assert_eq!(proposals[0].status, ProposalStatus::NeedsReview);
        assert_eq!(queue.pending_proposals().unwrap().len(), 1);
        drop(queue);
        fs_remove(&root);
    }

    #[test]
    fn pending_proposals_can_be_edited_and_decisions_persist() {
        let root = root("edit");
        let vault = Vault::create(&root).unwrap();
        let mut queue = ExtractionQueue::open(&vault, "lyra").unwrap();
        let job = queue
            .enqueue_transcript(&transcript(), "assistant-1", 10)
            .unwrap();
        queue.claim_next().unwrap();
        let proposal = queue
            .accept_model_output(&job.id, &output("user-1", "user_stated"))
            .unwrap()
            .remove(0);
        let edited = queue
            .edit_proposal(&proposal.id, "User likes tea and quiet cafes.", 0.9)
            .unwrap();
        assert_eq!(edited.candidate.confidence, 0.9);
        assert_eq!(
            queue
                .get_proposal(&proposal.id)
                .unwrap()
                .unwrap()
                .candidate
                .body,
            "User likes tea and quiet cafes."
        );
        queue
            .mark_proposal(&proposal.id, ProposalStatus::Rejected)
            .unwrap();
        assert_eq!(
            queue.get_proposal(&proposal.id).unwrap().unwrap().status,
            ProposalStatus::Rejected
        );
        assert!(queue.pending_proposals().unwrap().is_empty());
        drop(queue);
        fs_remove(&root);
    }

    #[test]
    fn invented_sources_and_malformed_output_are_rejected_with_bounded_retries() {
        let root = root("reject");
        let vault = Vault::create(&root).unwrap();
        let mut queue = ExtractionQueue::open(&vault, "lyra").unwrap();
        let job = queue
            .enqueue_transcript(&transcript(), "assistant-1", 10)
            .unwrap();
        for attempt in 0..DEFAULT_MAX_ATTEMPTS {
            assert!(queue.claim_next().unwrap().is_some());
            let result = if attempt == 0 {
                queue.accept_model_output(&job.id, &output("invented", "user_stated"))
            } else {
                queue.accept_model_output(&job.id, "not json")
            };
            assert!(result.is_err());
        }
        assert_eq!(queue.claim_next().unwrap(), None);
        assert_eq!(
            queue.get_job(&job.id).unwrap().unwrap().status,
            ExtractionJobStatus::Failed
        );
        drop(queue);
        fs_remove(&root);
    }

    #[test]
    fn running_jobs_resume_after_restart() {
        let root = root("restart");
        let vault = Vault::create(&root).unwrap();
        let job_id;
        {
            let mut queue = ExtractionQueue::open(&vault, "lyra").unwrap();
            let job = queue
                .enqueue_transcript(&transcript(), "assistant-1", 10)
                .unwrap();
            job_id = job.id;
            queue.claim_next().unwrap();
        }
        let mut queue = ExtractionQueue::open(&vault, "lyra").unwrap();
        let reclaimed = queue.claim_next().unwrap().unwrap();
        assert_eq!(reclaimed.id, job_id);
        assert_eq!(reclaimed.attempts, 1);
        drop(queue);
        fs_remove(&root);
    }

    #[test]
    fn foreground_preemption_defers_without_consuming_an_attempt() {
        let root = root("defer");
        let vault = Vault::create(&root).unwrap();
        let mut queue = ExtractionQueue::open(&vault, "lyra").unwrap();
        let job = queue
            .enqueue_transcript(&transcript(), "assistant-1", 10)
            .unwrap();
        let claimed = queue.claim_next().unwrap().unwrap();
        assert_eq!(claimed.attempts, 1);
        let deferred = queue
            .defer(&job.id, "deferred for foreground conversation")
            .unwrap()
            .unwrap();
        assert_eq!(deferred.status, ExtractionJobStatus::Pending);
        assert_eq!(deferred.attempts, 0);
        let reclaimed = queue.claim_next().unwrap().unwrap();
        assert_eq!(reclaimed.attempts, 1);
        drop(queue);
        fs_remove(&root);
    }

    #[test]
    fn user_stated_without_user_turn_quote_is_inferred() {
        let mut transcript = TranscriptDocument::new("session-1", "lyra");
        transcript.turns.push(TranscriptTurn {
            id: "user-1".into(),
            timestamp: "1".into(),
            role: TurnRole::User,
            status: TurnStatus::Complete,
            content: "I like quiet cafes.".into(),
        });
        transcript.turns.push(TranscriptTurn {
            id: "asst-1".into(),
            timestamp: "2".into(),
            role: TurnRole::Assistant,
            status: TurnStatus::Complete,
            content: "I will remember that.".into(),
        });
        let mut candidate = ExtractionCandidate {
            memory_type: MemoryType::Semantic,
            body: "User likes quiet cafes.".into(),
            source_turn_ids: vec!["asst-1".into()],
            evidence: vec![ProposalEvidence {
                turn_id: "asst-1".into(),
                quote: "I will remember that.".into(),
            }],
            confidence: 0.8,
            origin: ProposalOrigin::UserStated,
            related_memory_ids: Vec::new(),
            relation: None,
        };
        assert_eq!(
            effective_origin(&candidate, &transcript),
            ProposalOrigin::Inferred
        );
        candidate.evidence[0].turn_id = "user-1".into();
        assert_eq!(
            effective_origin(&candidate, &transcript),
            ProposalOrigin::UserStated
        );
    }

    #[test]
    fn inferred_origin_never_becomes_auto_writable() {
        assert!(!is_auto_writable(&ProposalOrigin::Inferred));
        assert!(is_auto_writable(&ProposalOrigin::UserStated));
        assert!(is_auto_writable(&ProposalOrigin::ConversationEvent));
        assert!(is_auto_writable(&ProposalOrigin::CharacterFact));
    }

    fn fs_remove(path: &PathBuf) {
        std::fs::remove_dir_all(path).unwrap();
    }
}
