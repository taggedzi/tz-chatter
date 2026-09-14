use crate::storage::{MemoryRecord, MemoryReviewStatus, MemoryType, StorageError, Vault};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
};

const CHUNK_CHARS: usize = 1200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryError {
    Storage(String),
    Database(String),
    InvalidQuery(String),
}

impl std::fmt::Display for MemoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(message) => write!(formatter, "memory storage failed: {message}"),
            Self::Database(message) => write!(formatter, "memory index failed: {message}"),
            Self::InvalidQuery(message) => write!(formatter, "invalid memory query: {message}"),
        }
    }
}

impl std::error::Error for MemoryError {}

impl From<StorageError> for MemoryError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<io::Error> for MemoryError {
    fn from(error: io::Error) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<rusqlite::Error> for MemoryError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error.to_string())
    }
}

impl From<MemoryError> for String {
    fn from(error: MemoryError) -> Self {
        error.to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemorySearchResult {
    pub memory_id: String,
    pub memory_type: MemoryType,
    pub chunk: String,
    pub source_path: String,
    pub updated_at: String,
    pub salience: f32,
    pub pinned: bool,
    pub score: f64,
}

#[derive(Debug)]
pub struct MemoryStore {
    vault: Vault,
    character_id: String,
    connection: Connection,
}

impl MemoryStore {
    pub fn open(vault: &Vault, character_id: impl Into<String>) -> Result<Self, MemoryError> {
        let character_id = character_id.into();
        if character_id.trim().is_empty() {
            return Err(MemoryError::Storage("character id is required".into()));
        }
        let db_path =
            vault.resolve_relative(Path::new(".tz-chatter").join("memory-index.sqlite3"))?;
        let connection = Connection::open(db_path)?;
        let store = Self {
            vault: vault.clone(),
            character_id,
            connection,
        };
        store.initialize()?;
        Ok(store)
    }

    pub fn create(&mut self, memory: &MemoryRecord) -> Result<(), MemoryError> {
        self.vault.save_memory(memory)?;
        self.sync()
    }

    pub fn update(&mut self, memory: &MemoryRecord) -> Result<(), MemoryError> {
        self.update_with_expected(memory, None)
    }

    pub fn update_with_expected(
        &mut self,
        memory: &MemoryRecord,
        expected_fingerprint: Option<&str>,
    ) -> Result<(), MemoryError> {
        let path = self.vault.memory_path(&memory.memory_type, &memory.id)?;
        self.ensure_unchanged(&memory.memory_type, &memory.id, &path, expected_fingerprint)?;
        self.vault.save_memory(memory)?;
        self.sync()
    }

    pub fn rename(
        &mut self,
        old_type: &MemoryType,
        old_id: &str,
        memory: &MemoryRecord,
    ) -> Result<(), MemoryError> {
        let old_path = self.vault.memory_path(old_type, old_id)?;
        let new_path = self.vault.memory_path(&memory.memory_type, &memory.id)?;
        if old_path != new_path && new_path.exists() {
            return Err(MemoryError::Storage(format!(
                "memory destination already exists: {}",
                memory.id
            )));
        }
        self.ensure_unchanged(old_type, old_id, &old_path, None)?;
        if old_path != new_path {
            fs::rename(&old_path, &new_path)?;
        }
        self.vault.save_memory(memory)?;
        self.sync()
    }

    pub fn delete(&mut self, memory_type: &MemoryType, id: &str) -> Result<(), MemoryError> {
        let path = self.vault.memory_path(memory_type, id)?;
        if path.exists() {
            fs::remove_file(path)?;
        }
        self.sync()
    }

    pub fn sync(&mut self) -> Result<(), MemoryError> {
        let files = discover_memory_files(self.vault.root())?;
        let mut seen = HashSet::new();
        for file in files {
            let key = memory_key(&file.memory_type, &file.id);
            seen.insert(key.clone());
            let bytes = fs::read(&file.path)?;
            let fingerprint = fingerprint(&bytes);
            let indexed_fingerprint: Option<String> = self
                .connection
                .query_row(
                    "SELECT fingerprint FROM memory_files WHERE memory_key = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .optional()?;
            if indexed_fingerprint.as_deref() == Some(fingerprint.as_str()) {
                continue;
            }
            let memory = match self.vault.load_memory(&file.memory_type, &file.id) {
                Ok(memory) => memory,
                Err(error) => {
                    eprintln!("skipping invalid memory {}: {error}", file.relative_path);
                    continue;
                }
            };
            self.replace_indexed_memory(&key, &file, &memory, &fingerprint)?;
        }

        let indexed_keys = self
            .connection
            .prepare("SELECT memory_key FROM memory_files")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for key in indexed_keys {
            if !seen.contains(&key) {
                self.remove_indexed_memory(&key)?;
            }
        }
        Ok(())
    }

    pub fn rebuild(&mut self) -> Result<(), MemoryError> {
        self.connection.execute("DELETE FROM memory_fts", [])?;
        self.connection.execute("DELETE FROM memory_files", [])?;
        self.sync()
    }

    pub fn list(&mut self) -> Result<Vec<MemoryRecord>, MemoryError> {
        self.sync()?;
        let mut memories = Vec::new();
        for file in discover_memory_files(self.vault.root())? {
            let memory = match self.vault.load_memory(&file.memory_type, &file.id) {
                Ok(memory) => memory,
                Err(error) => {
                    eprintln!("skipping invalid memory {}: {error}", file.relative_path);
                    continue;
                }
            };
            memories.push(memory);
        }
        memories.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(memories)
    }

    pub fn source_path_for(memory_type: &MemoryType, id: &str) -> String {
        Path::new("memories")
            .join(memory_type_folder(memory_type))
            .join(format!("{id}.md"))
            .to_string_lossy()
            .replace('\\', "/")
    }

    pub fn search(
        &mut self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemorySearchResult>, MemoryError> {
        let query = fts_query(query)?;
        if query.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        self.sync()?;
        let results = self.search_index(&query, limit)?;
        if !results.is_empty() || !query.contains(" AND ") {
            return Ok(results);
        }
        self.search_index(&query.replace(" AND ", " OR "), limit)
    }

    fn search_index(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemorySearchResult>, MemoryError> {
        let mut statement = self.connection.prepare(
            "SELECT f.memory_id, f.memory_type, f.source_path, f.updated_at, f.salience, f.pinned,
             memory_fts.body, bm25(memory_fts)
             FROM memory_fts
             JOIN memory_files AS f ON f.memory_key = memory_fts.memory_key
             WHERE memory_fts MATCH ?1 AND memory_fts.character_id = ?2
             ORDER BY bm25(memory_fts) LIMIT ?3",
        )?;
        let rows = statement.query_map(params![query, self.character_id, limit as i64], |row| {
            let memory_type = parse_memory_type(&row.get::<_, String>(1)?)
                .ok_or(rusqlite::Error::InvalidQuery)?;
            Ok(MemorySearchResult {
                memory_id: row.get(0)?,
                memory_type,
                source_path: row.get(2)?,
                updated_at: row.get(3)?,
                salience: row.get(4)?,
                pinned: row.get::<_, i64>(5)? != 0,
                chunk: row.get(6)?,
                score: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(MemoryError::from)
    }

    pub fn search_records(
        &mut self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryRecord>, MemoryError> {
        let hits = self.search(query, limit)?;
        let mut seen = HashSet::new();
        let mut records = Vec::new();
        for hit in hits {
            let key = memory_key(&hit.memory_type, &hit.memory_id);
            if seen.insert(key) {
                records.push(self.vault.load_memory(&hit.memory_type, &hit.memory_id)?);
            }
        }
        Ok(records)
    }

    fn initialize(&self) -> Result<(), MemoryError> {
        self.connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS memory_files (
                 memory_key TEXT PRIMARY KEY,
                 memory_id TEXT NOT NULL,
                 character_id TEXT NOT NULL,
                 memory_type TEXT NOT NULL,
                source_path TEXT NOT NULL,
                fingerprint TEXT NOT NULL,
                review_status TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                salience REAL NOT NULL,
                pinned INTEGER NOT NULL
             );
             CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
                 memory_key UNINDEXED,
                 memory_id UNINDEXED,
                 character_id UNINDEXED,
                 memory_type UNINDEXED,
                updated_at UNINDEXED,
                body
            );",
        )?;
        ensure_metadata_column(&self.connection, "updated_at", "TEXT NOT NULL DEFAULT 0")?;
        ensure_metadata_column(&self.connection, "salience", "REAL NOT NULL DEFAULT 0.5")?;
        ensure_metadata_column(&self.connection, "pinned", "INTEGER NOT NULL DEFAULT 0")?;
        Ok(())
    }

    fn replace_indexed_memory(
        &mut self,
        key: &str,
        file: &MemoryFile,
        memory: &MemoryRecord,
        fingerprint: &str,
    ) -> Result<(), MemoryError> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM memory_fts WHERE memory_key = ?1", params![key])?;
        transaction.execute(
            "DELETE FROM memory_files WHERE memory_key = ?1",
            params![key],
        )?;
        transaction.execute(
            "INSERT INTO memory_files
             (memory_key, memory_id, character_id, memory_type, source_path, fingerprint, review_status, updated_at, salience, pinned)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                key,
                memory.id,
                self.character_id,
                memory_type_label(&memory.memory_type),
                file.relative_path,
                fingerprint,
                review_status_label(&memory.review_status),
                memory.updated_at,
                memory.salience,
                i64::from(memory.pinned),
            ],
        )?;
        if memory.review_status != MemoryReviewStatus::Excluded {
            for chunk in chunk_markdown(&memory.body) {
                transaction.execute(
                    "INSERT INTO memory_fts
                     (memory_key, memory_id, character_id, memory_type, updated_at, body)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        key,
                        memory.id,
                        self.character_id,
                        memory_type_label(&memory.memory_type),
                        memory.updated_at,
                        chunk,
                    ],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    fn remove_indexed_memory(&self, key: &str) -> Result<(), MemoryError> {
        self.connection
            .execute("DELETE FROM memory_fts WHERE memory_key = ?1", params![key])?;
        self.connection.execute(
            "DELETE FROM memory_files WHERE memory_key = ?1",
            params![key],
        )?;
        Ok(())
    }

    fn ensure_unchanged(
        &self,
        memory_type: &MemoryType,
        id: &str,
        path: &Path,
        expected_fingerprint: Option<&str>,
    ) -> Result<(), MemoryError> {
        if !path.exists() {
            return Err(MemoryError::Storage(format!("memory does not exist: {id}")));
        }
        let key = memory_key(memory_type, id);
        let current = fingerprint(&fs::read(path)?);
        let expected = if let Some(expected) = expected_fingerprint {
            expected.to_owned()
        } else {
            self.connection
                .query_row(
                    "SELECT fingerprint FROM memory_files WHERE memory_key = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| MemoryError::Storage(format!("memory is not indexed: {id}")))?
        };
        if expected != current {
            return Err(MemoryError::Storage(format!(
                "memory changed externally; reload before writing: {id}"
            )));
        }
        Ok(())
    }

    pub fn file_fingerprint(
        &self,
        memory_type: &MemoryType,
        id: &str,
    ) -> Result<String, MemoryError> {
        let path = self.vault.memory_path(memory_type, id)?;
        if !path.exists() {
            return Err(MemoryError::Storage(format!("memory does not exist: {id}")));
        }
        Ok(fingerprint(&fs::read(path)?))
    }
}

#[derive(Debug)]
struct MemoryFile {
    path: PathBuf,
    relative_path: String,
    memory_type: MemoryType,
    id: String,
}

fn ensure_metadata_column(
    connection: &Connection,
    column: &str,
    definition: &str,
) -> Result<(), MemoryError> {
    let exists = connection
        .prepare("PRAGMA table_info(memory_files)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|name| name == column);
    if !exists {
        connection.execute(
            &format!("ALTER TABLE memory_files ADD COLUMN {column} {definition}"),
            [],
        )?;
    }
    Ok(())
}

fn discover_memory_files(root: &Path) -> Result<Vec<MemoryFile>, MemoryError> {
    let memory_root = root.join("memories");
    if !memory_root.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for type_entry in fs::read_dir(&memory_root)? {
        let type_entry = type_entry?;
        if !type_entry.file_type()?.is_dir() {
            continue;
        }
        let Some(memory_type) = parse_memory_type(&type_entry.file_name().to_string_lossy()) else {
            continue;
        };
        for entry in fs::read_dir(type_entry.path())? {
            let entry = entry?;
            if !entry.file_type()?.is_file()
                || entry.path().extension().and_then(|value| value.to_str()) != Some("md")
            {
                continue;
            }
            let path = entry.path();
            let Some(id) = path
                .file_stem()
                .and_then(|value| value.to_str())
                .map(str::to_owned)
            else {
                continue;
            };
            let relative_path = Path::new("memories")
                .join(memory_type_folder(&memory_type))
                .join(entry.file_name())
                .to_string_lossy()
                .replace('\\', "/");
            files.push(MemoryFile {
                path,
                relative_path,
                memory_type: memory_type.clone(),
                id,
            });
        }
    }
    Ok(files)
}

pub(crate) fn chunk_markdown(body: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for paragraph in body.split("\n\n") {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }
        if !current.is_empty()
            && current.chars().count() + paragraph.chars().count() + 2 > CHUNK_CHARS
        {
            chunks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(paragraph);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    if chunks.is_empty() {
        vec![body.to_owned()]
    } else {
        chunks
    }
}

fn fts_query(query: &str) -> Result<String, MemoryError> {
    const STOP_WORDS: &[&str] = &[
        "a", "an", "and", "are", "did", "do", "does", "for", "how", "i", "in", "is", "it", "me",
        "of", "on", "or", "remember", "tell", "that", "the", "to", "was", "what", "when", "where",
        "which", "who", "why", "you",
    ];
    let terms = query
        .split_whitespace()
        .map(|term| {
            term.chars()
                .filter(|character| character.is_alphanumeric())
                .collect::<String>()
        })
        .filter(|term| {
            !term.is_empty() && !STOP_WORDS.contains(&term.to_ascii_lowercase().as_str())
        })
        .map(|term| format!("\"{term}\""))
        .collect::<Vec<_>>();
    Ok(terms.join(" AND "))
}

fn memory_key(memory_type: &MemoryType, id: &str) -> String {
    format!("{}:{id}", memory_type_label(memory_type))
}

fn memory_type_folder(memory_type: &MemoryType) -> &'static str {
    match memory_type {
        MemoryType::People => "people",
        MemoryType::Episodic => "episodic",
        MemoryType::Semantic => "semantic",
        MemoryType::Relationships => "relationships",
        MemoryType::OpenThreads => "open-threads",
    }
}

fn memory_type_label(memory_type: &MemoryType) -> &'static str {
    memory_type_folder(memory_type)
}

fn parse_memory_type(value: &str) -> Option<MemoryType> {
    match value {
        "people" => Some(MemoryType::People),
        "episodic" => Some(MemoryType::Episodic),
        "semantic" => Some(MemoryType::Semantic),
        "relationships" => Some(MemoryType::Relationships),
        "open-threads" => Some(MemoryType::OpenThreads),
        _ => None,
    }
}

fn review_status_label(status: &MemoryReviewStatus) -> &'static str {
    match status {
        MemoryReviewStatus::Accepted => "accepted",
        MemoryReviewStatus::NeedsReview => "needs_review",
        MemoryReviewStatus::Excluded => "excluded",
    }
}

fn fingerprint(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemoryType;

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "tz-chatter-memory-{label}-{}",
            crate::storage::new_stable_id()
        ))
    }

    fn memory(id: &str, body: &str) -> MemoryRecord {
        let mut memory = MemoryRecord::new(id, MemoryType::Semantic, body);
        memory.created_at = "1".into();
        memory.updated_at = "1".into();
        memory
    }

    #[test]
    fn create_external_edit_delete_and_rebuild_stay_in_sync() {
        let root = root("sync");
        let vault = Vault::create(&root).unwrap();
        let mut index = MemoryStore::open(&vault, "lyra").unwrap();
        index.create(&memory("fact", "Mina likes tea")).unwrap();
        assert_eq!(index.search("Mina tea", 10).unwrap().len(), 1);

        let mut edited = memory("fact", "Mina likes coffee");
        edited.updated_at = "2".into();
        vault.save_memory(&edited).unwrap();
        let stale = memory("fact", "Mina likes tea");
        assert!(matches!(
            index.update(&stale),
            Err(MemoryError::Storage(message)) if message.contains("changed externally")
        ));
        assert!(index
            .search("coffee", 10)
            .unwrap()
            .iter()
            .any(|result| result.memory_id == "fact"));
        assert!(index.search("tea", 10).unwrap().is_empty());

        let renamed = memory("renamed", "Mina likes coffee");
        index
            .rename(&MemoryType::Semantic, "fact", &renamed)
            .unwrap();
        assert_eq!(index.search("coffee", 10).unwrap()[0].memory_id, "renamed");
        let path = vault.memory_path(&MemoryType::Semantic, "renamed").unwrap();

        fs::remove_file(path).unwrap();
        assert!(index.search("coffee", 10).unwrap().is_empty());
        index
            .create(&memory("fact", "Mina likes tea again"))
            .unwrap();
        index.rebuild().unwrap();
        assert_eq!(index.search("tea", 10).unwrap().len(), 1);
        drop(index);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn excluded_memories_and_other_vaults_are_not_searchable() {
        let root_a = root("isolation-a");
        let root_b = root("isolation-b");
        let vault_a = Vault::create(&root_a).unwrap();
        let vault_b = Vault::create(&root_b).unwrap();
        let mut index_a = MemoryStore::open(&vault_a, "lyra").unwrap();
        let mut index_b = MemoryStore::open(&vault_b, "nova").unwrap();
        index_a
            .create(&memory("same", "private Lyra detail"))
            .unwrap();
        let mut excluded = memory("excluded", "private excluded detail");
        excluded.review_status = MemoryReviewStatus::Excluded;
        index_a.create(&excluded).unwrap();
        index_b
            .create(&memory("same", "private Nova detail"))
            .unwrap();
        assert_eq!(index_a.search("Lyra", 10).unwrap().len(), 1);
        assert!(index_a.search("excluded", 10).unwrap().is_empty());
        assert_eq!(index_b.search("Nova", 10).unwrap().len(), 1);
        assert!(index_b.search("Lyra", 10).unwrap().is_empty());
        drop(index_a);
        drop(index_b);
        fs::remove_dir_all(root_a).unwrap();
        fs::remove_dir_all(root_b).unwrap();
    }

    #[test]
    fn excluded_and_locked_memories_are_browsable() {
        let root = root("browse-excluded");
        let vault = Vault::create(&root).unwrap();
        let mut index = MemoryStore::open(&vault, "lyra").unwrap();
        let mut excluded = memory("excluded", "Still available for review");
        excluded.review_status = MemoryReviewStatus::Excluded;
        excluded.locked = true;
        index.create(&excluded).unwrap();

        let memories = index.list().unwrap();
        assert_eq!(memories.len(), 1);
        assert!(memories[0].locked);
        assert_eq!(memories[0].review_status, MemoryReviewStatus::Excluded);

        drop(index);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn markdown_is_chunked_without_losing_paragraphs() {
        let body = format!("{}\n\n{}", "A".repeat(700), "B".repeat(700));
        let chunks = chunk_markdown(&body);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].starts_with('A'));
        assert!(chunks[1].starts_with('B'));
    }

    #[test]
    fn natural_language_queries_do_not_require_stop_words() {
        let query = fts_query("Do you remember what Mina likes?").unwrap();
        assert_eq!(query, "\"Mina\" AND \"likes\"");
    }

    #[test]
    fn search_falls_back_to_any_meaningful_term_when_all_terms_miss() {
        let root = root("query-fallback");
        let vault = Vault::create(&root).unwrap();
        let mut index = MemoryStore::open(&vault, "lyra").unwrap();
        index.create(&memory("fact", "Mina likes tea")).unwrap();

        let results = index
            .search("Do you remember whether Mina likes coffee?", 10)
            .unwrap();
        assert!(results.iter().any(|result| result.memory_id == "fact"));

        drop(index);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn search_records_return_the_complete_canonical_memory() {
        let root = root("canonical-search");
        let vault = Vault::create(&root).unwrap();
        let mut index = MemoryStore::open(&vault, "lyra").unwrap();
        let mut expected = memory(
            "full-record",
            &format!("needle in the first paragraph\n\n{}", "detail ".repeat(240)),
        );
        expected.memory_type = MemoryType::People;
        expected.source_session_id = Some("session-7".into());
        expected.source_turn_ids = vec!["turn-1".into(), "turn-2".into()];
        expected.topics = vec!["tea".into(), "friendship".into()];
        expected.salience = 0.9;
        expected.confidence = 0.8;
        expected.pinned = true;
        expected.locked = true;
        index.create(&expected).unwrap();

        let records = index.search_records("needle", 10).unwrap();
        assert_eq!(records, vec![expected]);

        drop(index);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn markdown_body_is_authoritative_and_invalid_files_are_skipped() {
        let root = root("body-authority");
        let vault = Vault::create(&root).unwrap();
        let mut index = MemoryStore::open(&vault, "lyra").unwrap();
        index.create(&memory("fact", "Old body.")).unwrap();
        let path = vault.memory_path(&MemoryType::Semantic, "fact").unwrap();
        let bytes = fs::read_to_string(&path).unwrap();
        let at = bytes.rfind("Old body.").unwrap();
        let edited = format!(
            "{}New body.{}",
            &bytes[..at],
            &bytes[at + "Old body.".len()..]
        );
        fs::write(&path, edited).unwrap();
        assert_eq!(
            vault
                .load_memory(&MemoryType::Semantic, "fact")
                .unwrap()
                .body,
            "New body."
        );
        let broken = vault.memory_path(&MemoryType::Semantic, "broken").unwrap();
        fs::create_dir_all(broken.parent().unwrap()).unwrap();
        fs::write(&broken, "invalid markdown").unwrap();
        let listed = index.list().unwrap();
        assert!(listed.iter().any(|memory| memory.id == "fact"));
        assert!(!listed.iter().any(|memory| memory.id == "broken"));
        drop(index);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_editor_fingerprint_conflicts_after_index_refresh() {
        let root = root("stale-editor");
        let vault = Vault::create(&root).unwrap();
        let mut store = MemoryStore::open(&vault, "lyra").unwrap();
        let mut record = memory("fact", "Initial body.");
        store.create(&record).unwrap();
        let loaded = store
            .file_fingerprint(&record.memory_type, &record.id)
            .unwrap();
        record.body = "New external fact.".into();
        vault.save_memory(&record).unwrap();
        store.sync().unwrap();
        let mut stale = memory("fact", "Stale editor body.");
        stale.updated_at = "2".into();
        assert!(matches!(
            store.update_with_expected(&stale, Some(&loaded)),
            Err(MemoryError::Storage(message)) if message.contains("changed externally")
        ));
        assert_eq!(
            vault
                .load_memory(&record.memory_type, &record.id)
                .unwrap()
                .body,
            "New external fact."
        );
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
}
