use crate::memory::chunk_markdown;
use crate::storage::{MemoryRecord, MemoryReviewStatus, Vault};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::path::Path;

pub const EMBEDDING_INDEX_VERSION: u32 = 1;
pub const CHUNKING_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmbeddingSpace {
    pub provider_id: String,
    pub model: String,
    #[serde(default)]
    pub endpoint: String,
    pub dimensions: usize,
    pub chunking_version: u32,
    pub index_version: u32,
}

impl EmbeddingSpace {
    pub fn new(
        provider_id: impl Into<String>,
        model: impl Into<String>,
        dimensions: usize,
        endpoint: impl Into<String>,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            model: model.into(),
            endpoint: endpoint.into(),
            dimensions,
            chunking_version: CHUNKING_VERSION,
            index_version: EMBEDDING_INDEX_VERSION,
        }
    }

    pub fn matches_provider(&self, provider_id: &str, model: &str, endpoint: &str) -> bool {
        self.provider_id == provider_id
            && self.model == model
            && self.endpoint == endpoint
            && self.chunking_version == CHUNKING_VERSION
            && self.index_version == EMBEDDING_INDEX_VERSION
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmbeddingChunk {
    pub memory_id: String,
    pub fingerprint: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SemanticSearchResult {
    pub memory_id: String,
    pub fingerprint: String,
    pub content: String,
    pub score: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbeddingIndexError {
    Storage(String),
    Database(String),
    InvalidVector(String),
    Embedding(String),
    IncompatibleSpace(String),
}

impl fmt::Display for EmbeddingIndexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "embedding storage failed: {error}"),
            Self::Database(error) => write!(formatter, "embedding index failed: {error}"),
            Self::InvalidVector(error) => write!(formatter, "invalid embedding vector: {error}"),
            Self::Embedding(error) => write!(formatter, "embedding provider failed: {error}"),
            Self::IncompatibleSpace(error) => {
                write!(formatter, "incompatible embedding space: {error}")
            }
        }
    }
}

impl std::error::Error for EmbeddingIndexError {}

impl From<rusqlite::Error> for EmbeddingIndexError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error.to_string())
    }
}

pub struct EmbeddingIndex {
    connection: Connection,
    character_id: String,
    space: EmbeddingSpace,
}

impl EmbeddingIndex {
    pub fn open(
        vault: &Vault,
        character_id: impl Into<String>,
        space: EmbeddingSpace,
    ) -> Result<Self, EmbeddingIndexError> {
        let character_id = character_id.into();
        if character_id.trim().is_empty() {
            return Err(EmbeddingIndexError::Storage(
                "character id is required".into(),
            ));
        }
        if space.provider_id.trim().is_empty()
            || space.model.trim().is_empty()
            || space.dimensions == 0
        {
            return Err(EmbeddingIndexError::IncompatibleSpace(
                "provider, model, and dimensions are required".into(),
            ));
        }
        let directory = vault.root().join(".tz-chatter");
        std::fs::create_dir_all(&directory)
            .map_err(|error| EmbeddingIndexError::Storage(error.to_string()))?;
        let path =
            vault.resolve_relative(Path::new(".tz-chatter").join("embedding-index.sqlite3"))?;
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS embedding_metadata (
                 character_id TEXT PRIMARY KEY,
                 provider_id TEXT NOT NULL,
                 model TEXT NOT NULL,
                 endpoint TEXT NOT NULL DEFAULT '',
                 dimensions INTEGER NOT NULL,
                 chunking_version INTEGER NOT NULL,
                 index_version INTEGER NOT NULL,
                 status TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS embedding_vectors (
                 character_id TEXT NOT NULL,
                 memory_id TEXT NOT NULL,
                 fingerprint TEXT NOT NULL,
                 content TEXT NOT NULL,
                 dimensions INTEGER NOT NULL,
                 vector BLOB NOT NULL,
                 PRIMARY KEY (character_id, memory_id, fingerprint)
             );",
        )?;
        let _ = connection.execute(
            "ALTER TABLE embedding_metadata ADD COLUMN endpoint TEXT NOT NULL DEFAULT ''",
            [],
        );
        let mut index = Self {
            connection,
            character_id,
            space,
        };
        index.initialize_space()?;
        Ok(index)
    }

    pub fn stored_space(
        vault: &Vault,
        character_id: &str,
    ) -> Result<Option<EmbeddingSpace>, EmbeddingIndexError> {
        let path =
            vault.resolve_relative(Path::new(".tz-chatter").join("embedding-index.sqlite3"))?;
        if !path.exists() {
            return Ok(None);
        }
        let connection = Connection::open(path)?;
        let _ = connection.execute(
            "ALTER TABLE embedding_metadata ADD COLUMN endpoint TEXT NOT NULL DEFAULT ''",
            [],
        );
        let mut statement = match connection.prepare(
            "SELECT provider_id, model, endpoint, dimensions, chunking_version, index_version
             FROM embedding_metadata WHERE character_id = ?1",
        ) {
            Ok(statement) => statement,
            Err(error) if error.to_string().contains("no such table") => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let existing: Option<(String, String, String, i64, i64, i64)> = statement
            .query_row(params![character_id], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })
            .optional()?;
        Ok(existing.map(
            |(provider_id, model, endpoint, dimensions, chunking_version, index_version)| {
                EmbeddingSpace {
                    provider_id,
                    model,
                    endpoint,
                    dimensions: dimensions as usize,
                    chunking_version: chunking_version as u32,
                    index_version: index_version as u32,
                }
            },
        ))
    }

    pub fn space(&self) -> &EmbeddingSpace {
        &self.space
    }

    pub fn is_ready(&self) -> Result<bool, EmbeddingIndexError> {
        self.ensure_space()?;
        let status: String = self.connection.query_row(
            "SELECT status FROM embedding_metadata WHERE character_id = ?1",
            params![self.character_id],
            |row| row.get(0),
        )?;
        Ok(status == "ready")
    }

    pub fn begin_rebuild(&self) -> Result<(), EmbeddingIndexError> {
        self.ensure_space()?;
        self.connection.execute(
            "UPDATE embedding_metadata SET status = 'building' WHERE character_id = ?1",
            params![self.character_id],
        )?;
        self.connection.execute(
            "DELETE FROM embedding_vectors WHERE character_id = ?1",
            params![self.character_id],
        )?;
        Ok(())
    }

    pub fn finish_rebuild(&self) -> Result<(), EmbeddingIndexError> {
        self.ensure_space()?;
        self.connection.execute(
            "UPDATE embedding_metadata SET status = 'ready' WHERE character_id = ?1",
            params![self.character_id],
        )?;
        Ok(())
    }

    pub fn rebuild<F>(
        &mut self,
        chunks: &[EmbeddingChunk],
        mut embed: F,
    ) -> Result<(), EmbeddingIndexError>
    where
        F: FnMut(&[String]) -> Result<Vec<Vec<f32>>, String>,
    {
        self.begin_rebuild()?;
        let inputs = chunks
            .iter()
            .map(|chunk| chunk.content.clone())
            .collect::<Vec<_>>();
        let vectors = embed(&inputs).map_err(EmbeddingIndexError::Embedding)?;
        if vectors.len() != chunks.len() {
            return Err(EmbeddingIndexError::InvalidVector(format!(
                "provider returned {} vectors for {} chunks",
                vectors.len(),
                chunks.len()
            )));
        }
        for (chunk, vector) in chunks.iter().zip(vectors) {
            self.upsert(chunk, &vector)?;
        }
        self.finish_rebuild()
    }

    pub fn needs_rebuild(&self, chunks: &[EmbeddingChunk]) -> Result<bool, EmbeddingIndexError> {
        self.ensure_space()?;
        if !self.is_ready()? {
            return Ok(true);
        }
        let expected = chunks
            .iter()
            .map(|chunk| (chunk.memory_id.clone(), chunk.fingerprint.clone()))
            .collect::<HashSet<_>>();
        let actual = self
            .connection
            .prepare(
                "SELECT memory_id, fingerprint FROM embedding_vectors WHERE character_id = ?1",
            )?
            .query_map(params![self.character_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<HashSet<_>, _>>()?;
        Ok(actual != expected)
    }

    pub fn rebuild_precomputed(
        &self,
        chunks: &[EmbeddingChunk],
        vectors: &[Vec<f32>],
    ) -> Result<(), EmbeddingIndexError> {
        if chunks.len() != vectors.len() {
            return Err(EmbeddingIndexError::InvalidVector(format!(
                "provider returned {} vectors for {} chunks",
                vectors.len(),
                chunks.len()
            )));
        }
        self.begin_rebuild()?;
        for (chunk, vector) in chunks.iter().zip(vectors) {
            self.upsert(chunk, vector)?;
        }
        self.finish_rebuild()
    }

    pub fn upsert(
        &self,
        chunk: &EmbeddingChunk,
        vector: &[f32],
    ) -> Result<(), EmbeddingIndexError> {
        self.ensure_space()?;
        validate_vector(vector, self.space.dimensions)?;
        let bytes = vector
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        self.connection.execute(
            "INSERT OR REPLACE INTO embedding_vectors
             (character_id, memory_id, fingerprint, content, dimensions, vector)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                self.character_id,
                chunk.memory_id,
                chunk.fingerprint,
                chunk.content,
                self.space.dimensions as i64,
                bytes
            ],
        )?;
        Ok(())
    }

    pub fn search(
        &self,
        query: &[f32],
        limit: usize,
    ) -> Result<Vec<SemanticSearchResult>, EmbeddingIndexError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        if !self.is_ready()? {
            return Ok(Vec::new());
        }
        validate_vector(query, self.space.dimensions)?;
        let mut statement = self.connection.prepare(
            "SELECT memory_id, fingerprint, content, dimensions, vector
             FROM embedding_vectors WHERE character_id = ?1",
        )?;
        let rows = statement.query_map(params![self.character_id], |row| {
            let dimensions = row.get::<_, i64>(3)? as usize;
            let bytes: Vec<u8> = row.get(4)?;
            let vector =
                decode_vector(&bytes, dimensions).map_err(|_| rusqlite::Error::InvalidQuery)?;
            let score = cosine_similarity(query, &vector);
            Ok(SemanticSearchResult {
                memory_id: row.get(0)?,
                fingerprint: row.get(1)?,
                content: row.get(2)?,
                score,
            })
        })?;
        let mut results = rows.collect::<Result<Vec<_>, _>>()?;
        results.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(Ordering::Equal)
        });
        results.truncate(limit);
        Ok(results)
    }

    fn initialize_space(&mut self) -> Result<(), EmbeddingIndexError> {
        let existing: Option<(String, String, String, i64, i64, i64, String)> = self
            .connection
            .query_row(
                "SELECT provider_id, model, endpoint, dimensions, chunking_version, index_version, status
             FROM embedding_metadata WHERE character_id = ?1",
                params![self.character_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .optional()?;
        let compatible = existing.as_ref().is_some_and(|row| {
            row.0 == self.space.provider_id
                && row.1 == self.space.model
                && row.2 == self.space.endpoint
                && row.3 == self.space.dimensions as i64
                && row.4 == self.space.chunking_version as i64
                && row.5 == self.space.index_version as i64
        });
        if !compatible {
            self.connection.execute(
                "DELETE FROM embedding_vectors WHERE character_id = ?1",
                params![self.character_id],
            )?;
            self.connection.execute(
                "INSERT OR REPLACE INTO embedding_metadata
                 (character_id, provider_id, model, endpoint, dimensions, chunking_version, index_version, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'ready')",
                params![
                    self.character_id,
                    self.space.provider_id,
                    self.space.model,
                    self.space.endpoint,
                    self.space.dimensions as i64,
                    self.space.chunking_version as i64,
                    self.space.index_version as i64
                ],
            )?;
        } else if existing.as_ref().is_some_and(|row| row.6 == "building") {
            // A process that died during rebuild leaves a durable marker. Clear
            // partial vectors so the next rebuild cannot mix old and new chunks.
            self.connection.execute(
                "DELETE FROM embedding_vectors WHERE character_id = ?1",
                params![self.character_id],
            )?;
            self.connection.execute(
                "UPDATE embedding_metadata SET status = 'ready' WHERE character_id = ?1",
                params![self.character_id],
            )?;
        }
        Ok(())
    }

    fn ensure_space(&self) -> Result<(), EmbeddingIndexError> {
        let existing: Option<(String, String, String, i64, i64, i64)> = self
            .connection
            .query_row(
                "SELECT provider_id, model, endpoint, dimensions, chunking_version, index_version
             FROM embedding_metadata WHERE character_id = ?1",
                params![self.character_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .optional()?;
        let Some(row) = existing else {
            return Err(EmbeddingIndexError::IncompatibleSpace(
                "embedding metadata is missing".into(),
            ));
        };
        if row.0 != self.space.provider_id
            || row.1 != self.space.model
            || row.2 != self.space.endpoint
            || row.3 != self.space.dimensions as i64
            || row.4 != self.space.chunking_version as i64
            || row.5 != self.space.index_version as i64
        {
            return Err(EmbeddingIndexError::IncompatibleSpace(
                "active embedding space changed".into(),
            ));
        }
        Ok(())
    }
}

fn validate_vector(vector: &[f32], dimensions: usize) -> Result<(), EmbeddingIndexError> {
    if vector.len() != dimensions || vector.iter().any(|value| !value.is_finite()) {
        return Err(EmbeddingIndexError::InvalidVector(format!(
            "expected {dimensions} finite values, received {}",
            vector.len()
        )));
    }
    Ok(())
}

fn decode_vector(bytes: &[u8], dimensions: usize) -> Result<Vec<f32>, EmbeddingIndexError> {
    if bytes.len() != dimensions * std::mem::size_of::<f32>() {
        return Err(EmbeddingIndexError::InvalidVector(
            "stored vector length does not match metadata".into(),
        ));
    }
    Ok(bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect())
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    let mut dot = 0.0;
    let mut left_norm = 0.0;
    let mut right_norm = 0.0;
    for (left_value, right_value) in left.iter().zip(right) {
        dot += left_value * right_value;
        left_norm += left_value * left_value;
        right_norm += right_value * right_value;
    }
    if left_norm == 0.0 || right_norm == 0.0 {
        return 0.0;
    }
    dot / (left_norm.sqrt() * right_norm.sqrt())
}

pub fn chunk_fingerprint(content: &str, chunking_version: u32) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    chunking_version.hash(&mut hasher);
    content.hash(&mut hasher);
    format!("{chunking_version}:{}", hasher.finish())
}

pub fn chunks_from_memories(memories: &[MemoryRecord]) -> Vec<EmbeddingChunk> {
    memories
        .iter()
        .filter(|memory| memory.review_status != MemoryReviewStatus::Excluded)
        .flat_map(|memory| {
            chunk_markdown(&memory.body)
                .into_iter()
                .enumerate()
                .map(|(index, content)| EmbeddingChunk {
                    memory_id: memory.id.clone(),
                    fingerprint: chunk_fingerprint(&format!("{index}:{content}"), CHUNKING_VERSION),
                    content,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

impl From<crate::storage::StorageError> for EmbeddingIndexError {
    fn from(error: crate::storage::StorageError) -> Self {
        Self::Storage(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryStore;
    use crate::storage::{new_stable_id, MemoryRecord, MemoryType, Vault};
    use std::fs;
    use std::path::PathBuf;

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("tz-chatter-embeddings-{label}-{}", new_stable_id()))
    }

    fn space(model: &str, dimensions: usize) -> EmbeddingSpace {
        EmbeddingSpace::new("ollama", model, dimensions, "http://127.0.0.1:11434")
    }

    fn chunk(id: &str, content: &str) -> EmbeddingChunk {
        EmbeddingChunk {
            memory_id: id.into(),
            fingerprint: chunk_fingerprint(content, CHUNKING_VERSION),
            content: content.into(),
        }
    }

    #[test]
    fn model_and_dimension_changes_invalidate_old_vectors() {
        let root = root("invalidate");
        let vault = Vault::create(&root).unwrap();
        let mut index = EmbeddingIndex::open(&vault, "lyra", space("embed-a", 2)).unwrap();
        index
            .rebuild(&[chunk("one", "tea")], |_| Ok(vec![vec![1.0, 0.0]]))
            .unwrap();
        assert_eq!(index.search(&[1.0, 0.0], 10).unwrap().len(), 1);
        drop(index);
        let index = EmbeddingIndex::open(&vault, "lyra", space("embed-b", 2)).unwrap();
        assert!(index.search(&[1.0, 0.0], 10).unwrap().is_empty());
        drop(index);
        let index = EmbeddingIndex::open(&vault, "lyra", space("embed-b", 3)).unwrap();
        assert!(index.search(&[1.0, 0.0, 0.0], 10).unwrap().is_empty());
        drop(index);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn endpoint_changes_invalidate_equal_dimension_vectors() {
        let root = root("endpoint-invalidate");
        let vault = Vault::create(&root).unwrap();
        let first = EmbeddingSpace::new("ollama", "embed-a", 2, "http://127.0.0.1:11434");
        let mut index = EmbeddingIndex::open(&vault, "lyra", first).unwrap();
        index
            .rebuild(&[chunk("one", "tea")], |_| Ok(vec![vec![1.0, 0.0]]))
            .unwrap();
        assert_eq!(index.search(&[1.0, 0.0], 10).unwrap().len(), 1);
        drop(index);
        let switched = EmbeddingSpace::new("ollama", "embed-a", 2, "http://127.0.0.1:11435");
        assert!(!switched.matches_provider("ollama", "embed-a", "http://127.0.0.1:11434"));
        let index = EmbeddingIndex::open(&vault, "lyra", switched).unwrap();
        assert!(index.search(&[1.0, 0.0], 10).unwrap().is_empty());
        drop(index);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn direct_similarity_search_is_sorted_and_space_isolated() {
        let root = root("search");
        let vault = Vault::create(&root).unwrap();
        let mut index = EmbeddingIndex::open(&vault, "lyra", space("embed", 2)).unwrap();
        index
            .rebuild(&[chunk("one", "tea"), chunk("two", "cafes")], |_| {
                Ok(vec![vec![1.0, 0.0], vec![0.0, 1.0]])
            })
            .unwrap();
        let results = index.search(&[0.9, 0.1], 2).unwrap();
        assert_eq!(results[0].memory_id, "one");
        assert!(results[0].score > results[1].score);
        let mut other = EmbeddingIndex::open(&vault, "nova", space("embed", 2)).unwrap();
        other
            .rebuild(&[chunk("other", "private")], |_| Ok(vec![vec![1.0, 0.0]]))
            .unwrap();
        assert!(other
            .search(&[1.0, 0.0], 10)
            .unwrap()
            .iter()
            .all(|result| result.memory_id == "other"));
        drop(other);
        drop(index);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interrupted_rebuild_recovers_without_partial_vectors() {
        let root = root("recovery");
        let vault = Vault::create(&root).unwrap();
        let index = EmbeddingIndex::open(&vault, "lyra", space("embed", 2)).unwrap();
        index.begin_rebuild().unwrap();
        index
            .upsert(&chunk("partial", "partial"), &[1.0, 0.0])
            .unwrap();
        drop(index);
        let index = EmbeddingIndex::open(&vault, "lyra", space("embed", 2)).unwrap();
        assert!(index.is_ready().unwrap());
        assert!(index.search(&[1.0, 0.0], 10).unwrap().is_empty());
        drop(index);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rebuild_rejects_provider_shape_errors_and_keeps_lexical_path_independent() {
        let root = root("fallback");
        let vault = Vault::create(&root).unwrap();
        let mut index = EmbeddingIndex::open(&vault, "lyra", space("embed", 2)).unwrap();
        let error = index
            .rebuild(&[chunk("one", "tea")], |_| Ok(vec![vec![1.0]]))
            .unwrap_err();
        assert!(matches!(error, EmbeddingIndexError::InvalidVector(_)));
        assert!(index.search(&[1.0, 0.0], 10).unwrap().is_empty());
        let mut lexical = MemoryStore::open(&vault, "lyra").unwrap();
        lexical
            .create(&MemoryRecord::new(
                "lexical",
                MemoryType::Semantic,
                "tea remains searchable",
            ))
            .unwrap();
        assert_eq!(lexical.search("tea", 10).unwrap().len(), 1);
        drop(lexical);
        drop(index);
        fs::remove_dir_all(root).unwrap();
    }
}
