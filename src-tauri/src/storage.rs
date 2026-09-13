use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    fmt, fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;

pub const STORAGE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CharacterDefinition {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub summary: String,
    pub system_prompt: String,
    pub traits: Vec<String>,
    pub boundaries: Vec<String>,
    pub tags: Vec<String>,
}

impl CharacterDefinition {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        system_prompt: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: STORAGE_SCHEMA_VERSION,
            id: id.into(),
            name: name.into(),
            summary: String::new(),
            system_prompt: system_prompt.into(),
            traits: Vec::new(),
            boundaries: Vec::new(),
            tags: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    People,
    Episodic,
    Semantic,
    Relationships,
    OpenThreads,
}

impl MemoryType {
    fn folder(&self) -> &'static str {
        match self {
            Self::People => "people",
            Self::Episodic => "episodic",
            Self::Semantic => "semantic",
            Self::Relationships => "relationships",
            Self::OpenThreads => "open-threads",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryReviewStatus {
    Accepted,
    NeedsReview,
    Excluded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryRecord {
    pub schema_version: u32,
    pub id: String,
    pub memory_type: MemoryType,
    pub created_at: String,
    pub updated_at: String,
    pub source_session_id: Option<String>,
    pub source_turn_ids: Vec<String>,
    pub topics: Vec<String>,
    pub salience: f32,
    pub confidence: f32,
    pub review_status: MemoryReviewStatus,
    pub pinned: bool,
    #[serde(default)]
    pub locked: bool,
    pub body: String,
}

impl MemoryRecord {
    pub fn new(id: impl Into<String>, memory_type: MemoryType, body: impl Into<String>) -> Self {
        Self {
            schema_version: STORAGE_SCHEMA_VERSION,
            id: id.into(),
            memory_type,
            created_at: String::new(),
            updated_at: String::new(),
            source_session_id: None,
            source_turn_ids: Vec::new(),
            topics: Vec::new(),
            salience: 0.5,
            confidence: 0.5,
            review_status: MemoryReviewStatus::Accepted,
            pinned: false,
            locked: false,
            body: body.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TurnRole {
    System,
    User,
    Assistant,
    Initiative,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Complete,
    Interrupted,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranscriptTurn {
    pub id: String,
    pub timestamp: String,
    pub role: TurnRole,
    pub status: TurnStatus,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranscriptDocument {
    pub schema_version: u32,
    pub session_id: String,
    pub character_id: String,
    pub created_at: String,
    pub updated_at: String,
    /// Missing follows the character default local. `Some("")` clears the scene.
    /// `Some(id)` live-links `locals/{id}.md`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub archived: bool,
    pub turns: Vec<TranscriptTurn>,
}

impl TranscriptDocument {
    pub fn new(session_id: impl Into<String>, character_id: impl Into<String>) -> Self {
        Self {
            schema_version: STORAGE_SCHEMA_VERSION,
            session_id: session_id.into(),
            character_id: character_id.into(),
            created_at: String::new(),
            updated_at: String::new(),
            local: None,
            title: String::new(),
            archived: false,
            turns: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingJob {
    pub id: String,
    pub kind: String,
    pub source_key: String,
    pub attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperationalState {
    pub schema_version: u32,
    pub pending_jobs: Vec<PendingJob>,
    pub deleted_memory_ids: Vec<String>,
    pub last_opened_session_id: Option<String>,
}

impl Default for OperationalState {
    fn default() -> Self {
        Self {
            schema_version: STORAGE_SCHEMA_VERSION,
            pending_jobs: Vec::new(),
            deleted_memory_ids: Vec::new(),
            last_opened_session_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    InvalidSchema(String),
    InvalidPath(String),
    Parse(String),
    Persistence(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSchema(message) => write!(f, "invalid storage schema: {message}"),
            Self::InvalidPath(message) => write!(f, "invalid vault path: {message}"),
            Self::Parse(message) => write!(f, "could not parse storage file: {message}"),
            Self::Persistence(message) => write!(f, "storage persistence failed: {message}"),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<io::Error> for StorageError {
    fn from(error: io::Error) -> Self {
        Self::Persistence(error.to_string())
    }
}

impl From<StorageError> for String {
    fn from(error: StorageError) -> Self {
        error.to_string()
    }
}

#[derive(Debug, Clone)]
pub struct Vault {
    root: PathBuf,
}

impl Vault {
    pub fn create(root: impl AsRef<Path>) -> Result<Self, StorageError> {
        fs::create_dir_all(root.as_ref())?;
        Self::open(root)
    }

    pub fn open(root: impl AsRef<Path>) -> Result<Self, StorageError> {
        if !root.as_ref().is_dir() {
            return Err(StorageError::InvalidPath(format!(
                "vault directory does not exist: {}",
                root.as_ref().display()
            )));
        }
        let root = fs::canonicalize(root).map_err(StorageError::from)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn resolve_relative(&self, relative: impl AsRef<Path>) -> Result<PathBuf, StorageError> {
        let relative = relative.as_ref();
        if relative.as_os_str().is_empty() {
            return Err(StorageError::InvalidPath(
                "relative path may not be empty".into(),
            ));
        }
        if relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(StorageError::InvalidPath(format!(
                "path must stay inside the selected vault: {}",
                relative.display()
            )));
        }
        let candidate = self.root.join(relative);
        let existing_parent = candidate
            .parent()
            .ok_or_else(|| StorageError::InvalidPath("path has no parent".into()))?;
        fs::create_dir_all(existing_parent)?;
        let canonical_parent = fs::canonicalize(existing_parent)?;
        if !canonical_parent.starts_with(&self.root) {
            return Err(StorageError::InvalidPath(
                "path escapes the selected vault".into(),
            ));
        }
        if candidate.exists() {
            let canonical_candidate = fs::canonicalize(&candidate)?;
            if !canonical_candidate.starts_with(&self.root) {
                return Err(StorageError::InvalidPath(
                    "path resolves outside the selected vault".into(),
                ));
            }
        }
        Ok(candidate)
    }

    pub fn character_path(&self) -> Result<PathBuf, StorageError> {
        self.resolve_relative("character.md")
    }

    pub fn memory_path(&self, memory_type: &MemoryType, id: &str) -> Result<PathBuf, StorageError> {
        validate_id(id)?;
        self.resolve_relative(
            Path::new("memories")
                .join(memory_type.folder())
                .join(format!("{id}.md")),
        )
    }

    pub fn transcript_path(&self, session_id: &str) -> Result<PathBuf, StorageError> {
        validate_id(session_id)?;
        self.resolve_relative(Path::new("chats").join(format!("{session_id}.md")))
    }

    pub fn state_path(&self) -> Result<PathBuf, StorageError> {
        self.resolve_relative(Path::new(".tz-chatter").join("state.json"))
    }

    pub fn save_character(&self, character: &CharacterDefinition) -> Result<(), StorageError> {
        validate_character(character)?;
        let path = self.character_path()?;
        atomic_write(&path, render_markdown(character, &character.system_prompt)?)
    }

    pub fn load_character(&self) -> Result<CharacterDefinition, StorageError> {
        let path = self.character_path()?;
        let (character, body): (CharacterDefinition, String) =
            parse_markdown(&read_recovering(&path)?)?;
        if body.trim_end_matches('\n') != character.system_prompt.trim_end_matches('\n') {
            return Err(StorageError::InvalidSchema(
                "character body must match system_prompt metadata".into(),
            ));
        }
        validate_character(&character)?;
        Ok(character)
    }

    pub fn save_memory(&self, memory: &MemoryRecord) -> Result<(), StorageError> {
        validate_memory(memory)?;
        let path = self.memory_path(&memory.memory_type, &memory.id)?;
        atomic_write(&path, render_markdown(memory, &memory.body)?)
    }

    pub fn load_memory(
        &self,
        memory_type: &MemoryType,
        id: &str,
    ) -> Result<MemoryRecord, StorageError> {
        let path = self.memory_path(memory_type, id)?;
        let (memory, body): (MemoryRecord, String) = parse_markdown(&read_recovering(&path)?)?;
        if body.trim_end_matches('\n') != memory.body.trim_end_matches('\n') {
            return Err(StorageError::InvalidSchema(
                "memory body must match its metadata body".into(),
            ));
        }
        validate_memory(&memory)?;
        Ok(memory)
    }

    pub fn save_transcript(&self, transcript: &TranscriptDocument) -> Result<(), StorageError> {
        validate_transcript(transcript)?;
        let path = self.transcript_path(&transcript.session_id)?;
        atomic_write(&path, render_transcript(transcript)?)
    }

    pub fn load_transcript(&self, session_id: &str) -> Result<TranscriptDocument, StorageError> {
        let path = self.transcript_path(session_id)?;
        let (metadata, body): (TranscriptMetadata, String) =
            parse_markdown(&read_recovering(&path)?)?;
        let transcript = parse_transcript_body(metadata, &body)?;
        validate_transcript(&transcript)?;
        Ok(transcript)
    }

    pub fn list_transcripts(&self) -> Result<Vec<TranscriptDocument>, StorageError> {
        let chats = self.root.join("chats");
        if !chats.is_dir() {
            return Ok(Vec::new());
        }
        let mut transcripts = Vec::new();
        for entry in fs::read_dir(&chats)? {
            let path = entry?.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
                continue;
            }
            let Some(session_id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if validate_id(session_id).is_err() {
                continue;
            }
            if let Ok(transcript) = self.load_transcript(session_id) {
                transcripts.push(transcript);
            }
        }
        Ok(transcripts)
    }

    pub fn save_state(&self, state: &OperationalState) -> Result<(), StorageError> {
        validate_state(state)?;
        let path = self.state_path()?;
        let bytes = serde_json::to_vec_pretty(state)
            .map_err(|error| StorageError::Parse(error.to_string()))?;
        atomic_write(&path, bytes)
    }

    pub fn load_state(&self) -> Result<OperationalState, StorageError> {
        let path = self.state_path()?;
        if !path.exists() && !path.with_extension("json.bak").exists() {
            return Ok(OperationalState::default());
        }
        let state: OperationalState = serde_json::from_slice(&read_recovering(&path)?)
            .map_err(|error| StorageError::Parse(error.to_string()))?;
        validate_state(&state)?;
        Ok(state)
    }
}

pub fn new_stable_id() -> String {
    Uuid::new_v4().to_string()
}

pub fn validate_id(id: &str) -> Result<(), StorageError> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(StorageError::InvalidPath(format!(
            "invalid stable id: {id}"
        )));
    }
    Ok(())
}

fn validate_character(character: &CharacterDefinition) -> Result<(), StorageError> {
    if character.schema_version != STORAGE_SCHEMA_VERSION {
        return Err(StorageError::InvalidSchema(format!(
            "unsupported character version {}",
            character.schema_version
        )));
    }
    validate_id(&character.id)?;
    if character.name.trim().is_empty() || character.system_prompt.trim().is_empty() {
        return Err(StorageError::InvalidSchema(
            "character name and system_prompt are required".into(),
        ));
    }
    Ok(())
}

fn validate_memory(memory: &MemoryRecord) -> Result<(), StorageError> {
    if memory.schema_version != STORAGE_SCHEMA_VERSION {
        return Err(StorageError::InvalidSchema(format!(
            "unsupported memory version {}",
            memory.schema_version
        )));
    }
    validate_id(&memory.id)?;
    if memory.body.trim().is_empty()
        || !(0.0..=1.0).contains(&memory.salience)
        || !(0.0..=1.0).contains(&memory.confidence)
    {
        return Err(StorageError::InvalidSchema(
            "memory body, salience, and confidence are invalid".into(),
        ));
    }
    Ok(())
}

fn validate_transcript(transcript: &TranscriptDocument) -> Result<(), StorageError> {
    if transcript.schema_version != STORAGE_SCHEMA_VERSION {
        return Err(StorageError::InvalidSchema(format!(
            "unsupported transcript version {}",
            transcript.schema_version
        )));
    }
    validate_id(&transcript.session_id)?;
    validate_id(&transcript.character_id)?;
    if let Some(local) = transcript
        .local
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        validate_id(local)?;
    }
    for turn in &transcript.turns {
        validate_id(&turn.id)?;
        if turn.content.is_empty() && turn.status == TurnStatus::Complete {
            return Err(StorageError::InvalidSchema(format!(
                "transcript turn {} has empty content",
                turn.id
            )));
        }
    }
    Ok(())
}

fn validate_state(state: &OperationalState) -> Result<(), StorageError> {
    if state.schema_version != STORAGE_SCHEMA_VERSION {
        return Err(StorageError::InvalidSchema(format!(
            "unsupported operational state version {}",
            state.schema_version
        )));
    }
    for id in &state.deleted_memory_ids {
        validate_id(id)?;
    }
    for job in &state.pending_jobs {
        validate_id(&job.id)?;
    }
    Ok(())
}

pub(crate) fn render_markdown<T: Serialize>(
    metadata: &T,
    body: &str,
) -> Result<Vec<u8>, StorageError> {
    let yaml =
        serde_yaml::to_string(metadata).map_err(|error| StorageError::Parse(error.to_string()))?;
    Ok(format!("---\n{yaml}---\n{body}").into_bytes())
}

pub(crate) fn parse_markdown<T: DeserializeOwned>(
    bytes: &[u8],
) -> Result<(T, String), StorageError> {
    let text = String::from_utf8(bytes.to_vec())
        .map_err(|error| StorageError::Parse(error.to_string()))?;
    let text = text.replace("\r\n", "\n");
    if !text.starts_with("---\n") {
        return Err(StorageError::Parse(
            "Markdown file must begin with YAML frontmatter".into(),
        ));
    }
    let remainder = &text[4..];
    let separator = remainder
        .find("\n---\n")
        .ok_or_else(|| StorageError::Parse("YAML frontmatter is not closed".into()))?;
    let metadata = serde_yaml::from_str(&remainder[..separator])
        .map_err(|error| StorageError::Parse(error.to_string()))?;
    Ok((metadata, remainder[separator + 5..].to_string()))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct TranscriptMetadata {
    schema_version: u32,
    session_id: String,
    character_id: String,
    created_at: String,
    updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    local: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    title: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct TurnHeader {
    id: String,
    timestamp: String,
    role: TurnRole,
    status: TurnStatus,
    content_length: usize,
}

fn render_transcript(transcript: &TranscriptDocument) -> Result<Vec<u8>, StorageError> {
    let metadata = TranscriptMetadata {
        schema_version: transcript.schema_version,
        session_id: transcript.session_id.clone(),
        character_id: transcript.character_id.clone(),
        created_at: transcript.created_at.clone(),
        updated_at: transcript.updated_at.clone(),
        local: transcript.local.clone(),
        title: transcript.title.clone(),
        archived: transcript.archived,
    };
    let yaml =
        serde_yaml::to_string(&metadata).map_err(|error| StorageError::Parse(error.to_string()))?;
    let mut body = String::new();
    for (index, turn) in transcript.turns.iter().enumerate() {
        let header = TurnHeader {
            id: turn.id.clone(),
            timestamp: turn.timestamp.clone(),
            role: turn.role.clone(),
            status: turn.status.clone(),
            content_length: turn.content.len(),
        };
        let header_json = serde_json::to_string(&header)
            .map_err(|error| StorageError::Parse(error.to_string()))?;
        body.push_str("<!-- tz-chatter-turn: ");
        body.push_str(&header_json);
        body.push_str(" -->\n");
        body.push_str(&turn.content);
        if index + 1 < transcript.turns.len() {
            body.push('\n');
        }
    }
    Ok(format!("---\n{yaml}---\n{body}").into_bytes())
}

fn parse_transcript_body(
    metadata: TranscriptMetadata,
    body: &str,
) -> Result<TranscriptDocument, StorageError> {
    const PREFIX: &str = "<!-- tz-chatter-turn: ";
    let mut cursor = 0usize;
    let mut turns = Vec::new();
    while cursor < body.len() {
        while cursor < body.len() && body.as_bytes()[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor == body.len() {
            break;
        }
        if !body[cursor..].starts_with(PREFIX) {
            return Err(StorageError::Parse(
                "transcript contains text outside a turn marker".into(),
            ));
        }
        let marker_end = body[cursor..]
            .find(" -->\n")
            .map(|offset| cursor + offset)
            .ok_or_else(|| StorageError::Parse("transcript turn marker is not closed".into()))?;
        let header_json = &body[cursor + PREFIX.len()..marker_end];
        let header: TurnHeader = serde_json::from_str(header_json)
            .map_err(|error| StorageError::Parse(error.to_string()))?;
        let content_start = marker_end + 5;
        let content_end = content_start
            .checked_add(header.content_length)
            .ok_or_else(|| StorageError::Parse("transcript content length overflow".into()))?;
        if content_end > body.len() || !body.is_char_boundary(content_end) {
            return Err(StorageError::Parse(format!(
                "turn {} has invalid content length",
                header.id
            )));
        }
        turns.push(TranscriptTurn {
            id: header.id,
            timestamp: header.timestamp,
            role: header.role,
            status: header.status,
            content: body[content_start..content_end].to_string(),
        });
        cursor = content_end;
    }
    Ok(TranscriptDocument {
        schema_version: metadata.schema_version,
        session_id: metadata.session_id,
        character_id: metadata.character_id,
        created_at: metadata.created_at,
        updated_at: metadata.updated_at,
        local: metadata.local,
        title: metadata.title,
        archived: metadata.archived,
        turns,
    })
}

fn read_recovering(path: &Path) -> Result<Vec<u8>, StorageError> {
    if path.exists() {
        return Ok(fs::read(path)?);
    }
    let backup = path.with_extension(format!(
        "{}.bak",
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("tmp")
    ));
    if backup.exists() {
        fs::rename(&backup, path)?;
        return Ok(fs::read(path)?);
    }
    Err(StorageError::Persistence(format!(
        "file does not exist: {}",
        path.display()
    )))
}

pub(crate) fn atomic_write(path: &Path, bytes: Vec<u8>) -> Result<(), StorageError> {
    atomic_write_internal(path, bytes, false)
}

fn atomic_write_internal(
    path: &Path,
    bytes: Vec<u8>,
    fail_before_replace: bool,
) -> Result<(), StorageError> {
    let parent = path
        .parent()
        .ok_or_else(|| StorageError::Persistence("target path has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("storage"),
        new_stable_id()
    ));
    let mut file = fs::File::create(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    if fail_before_replace {
        let _ = fs::remove_file(&temporary);
        return Err(StorageError::Persistence(
            "write interrupted before replacement".into(),
        ));
    }

    let backup = path.with_extension(format!(
        "{}.bak",
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("tmp")
    ));
    if backup.exists() {
        fs::remove_file(&backup)?;
    }
    let had_previous = path.exists();
    if had_previous {
        fs::rename(path, &backup)?;
    }
    match fs::rename(&temporary, path) {
        Ok(()) => {
            if had_previous {
                let _ = fs::remove_file(backup);
            }
            Ok(())
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            if had_previous && backup.exists() {
                let _ = fs::rename(backup, path);
            }
            Err(StorageError::Persistence(error.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("tz-chatter-{label}-{}", new_stable_id()))
    }

    #[test]
    fn fixture_vault_round_trips_identity_memory_transcript_and_state() {
        let root = test_root("round-trip");
        let vault = Vault::create(&root).unwrap();
        let mut character =
            CharacterDefinition::new("lyra", "Lyra", "You are Lyra, a thoughtful archivist.");
        character.summary = "A patient guide for unfinished ideas.".into();
        character.traits = vec!["curious".into(), "precise".into()];
        character.boundaries = vec!["Never claim a memory without a source.".into()];
        character.tags = vec!["sample".into(), "archivist".into()];
        vault.save_character(&character).unwrap();

        let mut memory = MemoryRecord::new(
            "memory-001",
            MemoryType::People,
            "Mina prefers quiet cafés.\n\nShe sketches there on Sundays.",
        );
        memory.created_at = "2026-09-11T12:00:00Z".into();
        memory.updated_at = "2026-09-11T12:30:00Z".into();
        memory.source_session_id = Some("session-001".into());
        memory.source_turn_ids = vec!["turn-001".into()];
        memory.topics = vec!["Mina".into(), "cafés".into()];
        memory.salience = 0.8;
        memory.confidence = 0.9;
        vault.save_memory(&memory).unwrap();

        let mut transcript = TranscriptDocument::new("session-001", "lyra");
        transcript.created_at = "2026-09-11T12:00:00Z".into();
        transcript.updated_at = "2026-09-11T12:30:00Z".into();
        transcript.turns = vec![
            TranscriptTurn {
                id: "turn-001".into(),
                timestamp: "2026-09-11T12:00:00Z".into(),
                role: TurnRole::User,
                status: TurnStatus::Complete,
                content: "I met Mina today.\nShe brought a blue notebook.".into(),
            },
            TranscriptTurn {
                id: "turn-002".into(),
                timestamp: "2026-09-11T12:30:00Z".into(),
                role: TurnRole::Assistant,
                status: TurnStatus::Interrupted,
                content: "That sounds like a meaningful detail.\n\nWhat did she draw?".into(),
            },
        ];
        vault.save_transcript(&transcript).unwrap();

        let state = OperationalState {
            schema_version: STORAGE_SCHEMA_VERSION,
            pending_jobs: vec![PendingJob {
                id: "job-001".into(),
                kind: "extract_memory".into(),
                source_key: "session-001:turn-001".into(),
                attempts: 1,
            }],
            deleted_memory_ids: vec!["memory-deleted".into()],
            last_opened_session_id: Some("session-001".into()),
        };
        vault.save_state(&state).unwrap();

        assert_eq!(vault.load_character().unwrap(), character);
        assert_eq!(
            vault
                .load_memory(&MemoryType::People, "memory-001")
                .unwrap(),
            memory
        );
        assert_eq!(vault.load_transcript("session-001").unwrap(), transcript);
        assert_eq!(vault.load_state().unwrap(), state);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_invalid_schema_and_path_traversal() {
        let root = test_root("validation");
        let vault = Vault::create(&root).unwrap();
        assert!(matches!(
            vault.resolve_relative("../outside.md"),
            Err(StorageError::InvalidPath(_))
        ));
        assert!(matches!(
            vault.memory_path(&MemoryType::People, "../outside"),
            Err(StorageError::InvalidPath(_))
        ));

        let character_path = vault.character_path().unwrap();
        fs::write(character_path, "---\nschema_version: 99\nid: lyra\nname: Lyra\nsummary: ''\nsystem_prompt: prompt\ntraits: []\nboundaries: []\ntags: []\n---\nprompt").unwrap();
        assert!(matches!(
            vault.load_character(),
            Err(StorageError::InvalidSchema(_))
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn identity_and_writable_memory_are_separate_files() {
        let root = test_root("separation");
        let vault = Vault::create(&root).unwrap();
        let character = CharacterDefinition::new("lyra", "Lyra", "Author-controlled identity.");
        let memory = MemoryRecord::new("memory-001", MemoryType::Semantic, "Writable memory body.");
        vault.save_character(&character).unwrap();
        vault.save_memory(&memory).unwrap();
        let character_text = fs::read_to_string(vault.character_path().unwrap()).unwrap();
        let memory_text = fs::read_to_string(
            vault
                .memory_path(&MemoryType::Semantic, &memory.id)
                .unwrap(),
        )
        .unwrap();
        assert!(character_text.contains("Author-controlled identity."));
        assert!(!character_text.contains("Writable memory body."));
        assert!(memory_text.contains("Writable memory body."));
        assert!(!memory_text.contains("Author-controlled identity."));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interrupted_write_leaves_previous_valid_file_untouched() {
        let root = test_root("atomic");
        let vault = Vault::create(&root).unwrap();
        let character = CharacterDefinition::new("lyra", "Lyra", "Original prompt.");
        vault.save_character(&character).unwrap();
        let path = vault.character_path().unwrap();
        let error =
            atomic_write_internal(&path, b"partial invalid content".to_vec(), true).unwrap_err();
        assert!(matches!(error, StorageError::Persistence(_)));
        assert_eq!(vault.load_character().unwrap(), character);
        assert!(!path.with_extension("md.bak").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bundled_sample_character_is_loadable() {
        let source = include_bytes!("../../examples/characters/lyra/character.md");
        let (character, body): (CharacterDefinition, String) = parse_markdown(source).unwrap();
        validate_character(&character).unwrap();
        assert_eq!(
            body.trim_end_matches('\n'),
            character.system_prompt.trim_end_matches('\n')
        );
    }

    #[test]
    fn bundled_sample_vault_includes_memories_and_history() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("examples")
            .join("characters")
            .join("lyra");
        let vault = Vault::open(&root).unwrap();
        let character = vault.load_character().unwrap();
        assert_eq!(character.id, "lyra");
        assert_eq!(character.name, "Lyra");

        let mina = vault.load_memory(&MemoryType::People, "mina").unwrap();
        assert!(mina.body.contains("Mina"));
        assert!(mina.pinned);
        assert_eq!(
            vault
                .load_memory(&MemoryType::People, "user-preferences")
                .unwrap()
                .memory_type,
            MemoryType::People
        );
        assert_eq!(
            vault
                .load_memory(&MemoryType::Episodic, "first-session")
                .unwrap()
                .id,
            "first-session"
        );
        assert_eq!(
            vault
                .load_memory(&MemoryType::Semantic, "local-first")
                .unwrap()
                .memory_type,
            MemoryType::Semantic
        );
        assert_eq!(
            vault
                .load_memory(&MemoryType::Relationships, "working-style")
                .unwrap()
                .memory_type,
            MemoryType::Relationships
        );
        assert_eq!(
            vault
                .load_memory(&MemoryType::OpenThreads, "try-local-model")
                .unwrap()
                .memory_type,
            MemoryType::OpenThreads
        );

        let sessions = vault.list_transcripts().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "session-welcome");
        assert!(sessions[0].turns.len() >= 3);
        assert!(sessions[0].turns[0].content.contains("Mina"));

        let persona = crate::scene::load_persona(&vault).unwrap();
        assert!(persona.body.contains("unfinished ideas"));
        assert_eq!(
            crate::scene::load_scene_settings(&vault)
                .unwrap()
                .default_local
                .as_deref(),
            Some("cafe")
        );
        assert!(crate::scene::load_local(&vault, "cafe")
            .unwrap()
            .body
            .contains("quiet"));
    }

    #[test]
    fn transcript_title_and_archived_round_trip_and_legacy_files_still_load() {
        let root = test_root("transcript-title");
        let vault = Vault::create(&root).unwrap();
        let chats = vault.root().join("chats");
        fs::create_dir_all(&chats).unwrap();
        fs::write(
            chats.join("session-legacy.md"),
            "---\n\
schema_version: 1\n\
session_id: session-legacy\n\
character_id: lyra\n\
created_at: '1'\n\
updated_at: '1'\n\
---\n\
<!-- tz-chatter-turn: {\"id\":\"turn-1\",\"timestamp\":\"1\",\"role\":\"user\",\"status\":\"complete\",\"content_length\":5} -->\n\
Hello",
        )
        .unwrap();
        let legacy = vault.load_transcript("session-legacy").unwrap();
        assert_eq!(legacy.title, "");
        assert!(!legacy.archived);
        assert_eq!(legacy.turns[0].content, "Hello");

        let mut titled = TranscriptDocument::new("session-named", "lyra");
        titled.created_at = "2".into();
        titled.updated_at = "2".into();
        titled.title = "Garden walk".into();
        titled.archived = true;
        titled.turns.push(TranscriptTurn {
            id: "turn-2".into(),
            timestamp: "2".into(),
            role: TurnRole::User,
            status: TurnStatus::Complete,
            content: "We walked in the garden.".into(),
        });
        vault.save_transcript(&titled).unwrap();
        let loaded = vault.load_transcript("session-named").unwrap();
        assert_eq!(loaded, titled);
        let text = fs::read_to_string(vault.transcript_path("session-named").unwrap()).unwrap();
        assert!(text.contains("title: Garden walk"));
        assert!(text.contains("archived: true"));
        fs::remove_dir_all(root).unwrap();
    }
}
