use crate::{
    memory::MemoryStore,
    storage::{
        CharacterDefinition, MemoryRecord, MemoryType, OperationalState, StorageError, Vault,
    },
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    fmt, fs, io,
    path::{Component, Path, PathBuf},
};

pub const PACK_SCHEMA_VERSION: u32 = 1;
const PACK_FORMAT: &str = "tz-chatter-character-pack";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackManifest {
    pub schema_version: u32,
    pub format: String,
    pub character_id: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportResult {
    pub character_id: String,
    pub files_restored: usize,
    pub backup_path: Option<String>,
}

#[derive(Debug)]
pub enum PortabilityError {
    Invalid(String),
    Conflict(String),
    Storage(String),
}

impl fmt::Display for PortabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => write!(formatter, "invalid character pack: {message}"),
            Self::Conflict(message) => write!(formatter, "character pack conflict: {message}"),
            Self::Storage(message) => write!(formatter, "character pack storage failed: {message}"),
        }
    }
}

impl std::error::Error for PortabilityError {}

impl From<StorageError> for PortabilityError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<io::Error> for PortabilityError {
    fn from(error: io::Error) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<serde_json::Error> for PortabilityError {
    fn from(error: serde_json::Error) -> Self {
        Self::Invalid(error.to_string())
    }
}

impl From<rusqlite::Error> for PortabilityError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<PortabilityError> for String {
    fn from(error: PortabilityError) -> Self {
        error.to_string()
    }
}

pub fn export_pack(
    vault: &Vault,
    destination: impl AsRef<Path>,
) -> Result<PackManifest, PortabilityError> {
    let destination = destination.as_ref();
    if destination.exists() {
        return Err(PortabilityError::Conflict(format!(
            "export destination already exists: {}",
            destination.display()
        )));
    }
    let character = vault.load_character()?;
    let files = canonical_files(vault, &character)?;
    let manifest = PackManifest {
        schema_version: PACK_SCHEMA_VERSION,
        format: PACK_FORMAT.into(),
        character_id: character.id.clone(),
        files: files.clone(),
    };

    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}-export-{}",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("character-pack"),
        crate::storage::new_stable_id()
    ));
    fs::create_dir_all(&temporary)?;
    let result = (|| {
        for relative in &files {
            let source = vault_source_path(vault, relative)?;
            let target = temporary.join(relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(source, target)?;
        }
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
        fs::write(temporary.join("manifest.json"), manifest_bytes)?;
        fs::rename(&temporary, destination)?;
        Ok::<(), PortabilityError>(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    result.map(|()| manifest)
}

pub fn import_pack(
    pack_root: impl AsRef<Path>,
    destination_vault: impl AsRef<Path>,
    replace_existing: bool,
) -> Result<ImportResult, PortabilityError> {
    let pack_root = fs::canonicalize(pack_root.as_ref()).map_err(|error| {
        PortabilityError::Invalid(format!("pack directory is unavailable: {error}"))
    })?;
    if !pack_root.is_dir() {
        return Err(PortabilityError::Invalid(
            "pack path must be a directory".into(),
        ));
    }
    let manifest = migrate_manifest(serde_json::from_slice(&fs::read(
        pack_root.join("manifest.json"),
    )?)?)?;
    validate_manifest(&manifest)?;
    for relative in &manifest.files {
        let _ = safe_pack_file(&pack_root, relative)?;
    }

    let destination = destination_vault.as_ref();
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}-import-{}",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("character-vault"),
        crate::storage::new_stable_id()
    ));
    fs::create_dir_all(&temporary)?;
    let result = (|| {
        for relative in &manifest.files {
            let source = safe_pack_file(&pack_root, relative)?;
            let target = temporary.join(target_relative(relative));
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(source, target)?;
        }
        let staged = Vault::create(&temporary)?;
        validate_staged_vault(&staged, &manifest)?;
        ensure_destination_safe(destination, &manifest, replace_existing)?;

        let backup_path = if destination.exists() {
            let backup = parent.join(format!(
                ".{}-backup-{}",
                destination
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("character-vault"),
                crate::storage::new_stable_id()
            ));
            fs::rename(destination, &backup)?;
            Some(backup)
        } else {
            None
        };
        if let Err(error) = fs::rename(&temporary, destination) {
            if let Some(backup) = &backup_path {
                let _ = fs::rename(backup, destination);
            }
            return Err(PortabilityError::Storage(error.to_string()));
        }
        Ok::<Option<PathBuf>, PortabilityError>(backup_path)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    let backup_path = result?.map(|path| path.to_string_lossy().into_owned());
    Ok(ImportResult {
        character_id: manifest.character_id,
        files_restored: manifest.files.len(),
        backup_path,
    })
}

fn canonical_files(
    vault: &Vault,
    character: &CharacterDefinition,
) -> Result<Vec<String>, PortabilityError> {
    let mut files = vec!["character.md".to_owned()];
    for extra in ["persona.md", "scene.md"] {
        if vault.root().join(extra).exists() {
            files.push(extra.to_owned());
        }
    }
    if crate::characters::load_portrait(vault.root())
        .map_err(|error| PortabilityError::Invalid(error.to_string()))?
        .is_some()
    {
        files.push("assets/portrait.png".to_owned());
    }
    let locals = vault.root().join("locals");
    if locals.is_dir() {
        for entry in fs::read_dir(locals)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("md") {
                continue;
            }
            let id = path
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or_else(|| {
                    PortabilityError::Invalid("local filename is not valid UTF-8".into())
                })?;
            crate::storage::validate_id(id)
                .map_err(|error| PortabilityError::Invalid(error.to_string()))?;
            let _ = crate::scene::load_local(vault, id)
                .map_err(|error| PortabilityError::Invalid(error.to_string()))?;
            files.push(format!("locals/{id}.md"));
        }
    }
    for (memory_type, folder) in memory_type_folders() {
        let directory = vault.root().join("memories").join(folder);
        if !directory.exists() {
            continue;
        }
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("md") {
                continue;
            }
            let id = path
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or_else(|| {
                    PortabilityError::Invalid("memory filename is not valid UTF-8".into())
                })?;
            let memory: MemoryRecord = vault.load_memory(&memory_type, id)?;
            if memory.schema_version == 0 {
                return Err(PortabilityError::Invalid(
                    "memory schema version is invalid".into(),
                ));
            }
            files.push(format!("memories/{folder}/{id}.md"));
        }
    }

    let chats = vault.root().join("chats");
    if chats.exists() {
        for entry in fs::read_dir(chats)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("md") {
                continue;
            }
            let session_id = path
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or_else(|| {
                    PortabilityError::Invalid("transcript filename is not valid UTF-8".into())
                })?;
            let transcript = vault.load_transcript(session_id)?;
            if transcript.character_id != character.id {
                return Err(PortabilityError::Invalid(format!(
                    "transcript {session_id} belongs to another character"
                )));
            }
            files.push(format!("chats/{session_id}.md"));
        }
    }

    let state_json = vault.state_path()?;
    if state_json.exists() {
        let _: OperationalState = vault.load_state()?;
        files.push("state/state.json".into());
    }
    let state_db = vault.root().join(".tz-chatter").join("state.sqlite3");
    if state_db.exists() {
        validate_sqlite(&state_db)?;
        files.push("state/state.sqlite3".into());
    }
    Ok(files)
}

fn validate_manifest(manifest: &PackManifest) -> Result<(), PortabilityError> {
    if manifest.schema_version != PACK_SCHEMA_VERSION {
        return Err(PortabilityError::Invalid(format!(
            "unsupported pack schema version {}",
            manifest.schema_version
        )));
    }
    if manifest.format != PACK_FORMAT {
        return Err(PortabilityError::Invalid("unsupported pack format".into()));
    }
    crate::storage::validate_id(&manifest.character_id)?;
    if !manifest.files.iter().any(|file| file == "character.md") {
        return Err(PortabilityError::Invalid(
            "pack must contain character.md".into(),
        ));
    }
    let mut seen = HashSet::new();
    for file in &manifest.files {
        if !seen.insert(file) {
            return Err(PortabilityError::Invalid(format!(
                "duplicate pack file: {file}"
            )));
        }
        let path = Path::new(file);
        if path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
            || !(file == "character.md"
                || file == "persona.md"
                || file == "scene.md"
                || file == "assets/portrait.png"
                || file.starts_with("locals/")
                || file.starts_with("memories/")
                || file.starts_with("chats/")
                || file == "state/state.json"
                || file == "state/state.sqlite3")
        {
            return Err(PortabilityError::Invalid(format!(
                "pack file path is not permitted: {file}"
            )));
        }
    }
    Ok(())
}

fn migrate_manifest(value: Value) -> Result<PackManifest, PortabilityError> {
    let version = value
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| PortabilityError::Invalid("pack schema_version is required".into()))?;
    if version == 0 {
        let character_id = value
            .get("character_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                PortabilityError::Invalid("legacy pack character_id is required".into())
            })?;
        let files = value
            .get("files")
            .and_then(Value::as_array)
            .ok_or_else(|| PortabilityError::Invalid("legacy pack files are required".into()))?
            .iter()
            .map(|file| {
                file.as_str().map(str::to_owned).ok_or_else(|| {
                    PortabilityError::Invalid("legacy pack file must be text".into())
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(PackManifest {
            schema_version: PACK_SCHEMA_VERSION,
            format: PACK_FORMAT.into(),
            character_id: character_id.into(),
            files,
        });
    }
    Ok(serde_json::from_value(value)?)
}

fn validate_staged_vault(vault: &Vault, manifest: &PackManifest) -> Result<(), PortabilityError> {
    let character = vault.load_character()?;
    if character.id != manifest.character_id {
        return Err(PortabilityError::Invalid(
            "manifest character id does not match character.md".into(),
        ));
    }
    for relative in &manifest.files {
        if let Some((folder, file)) = relative
            .strip_prefix("memories/")
            .and_then(|value| value.split_once('/'))
        {
            let memory_type = memory_type_folders()
                .into_iter()
                .find(|(_, candidate)| *candidate == folder)
                .map(|(memory_type, _)| memory_type)
                .ok_or_else(|| {
                    PortabilityError::Invalid(format!("unknown memory folder: {folder}"))
                })?;
            let id = file.strip_suffix(".md").ok_or_else(|| {
                PortabilityError::Invalid(format!("invalid memory filename: {file}"))
            })?;
            let _ = vault.load_memory(&memory_type, id)?;
        } else if relative == "assets/portrait.png" {
            let bytes = crate::characters::load_portrait(vault.root())
                .map_err(|error| PortabilityError::Invalid(error.to_string()))?
                .ok_or_else(|| PortabilityError::Invalid("pack portrait is missing".into()))?;
            crate::characters::validate_portrait_bytes(&bytes)
                .map_err(|error| PortabilityError::Invalid(error.to_string()))?;
        } else if relative == "persona.md" {
            let _ = crate::scene::load_persona(vault)
                .map_err(|error| PortabilityError::Invalid(error.to_string()))?;
        } else if relative == "scene.md" {
            let _ = crate::scene::load_scene_settings(vault)
                .map_err(|error| PortabilityError::Invalid(error.to_string()))?;
        } else if let Some(id) = relative
            .strip_prefix("locals/")
            .and_then(|value| value.strip_suffix(".md"))
        {
            let _ = crate::scene::load_local(vault, id)
                .map_err(|error| PortabilityError::Invalid(error.to_string()))?;
        } else if let Some(session_id) = relative
            .strip_prefix("chats/")
            .and_then(|value| value.strip_suffix(".md"))
        {
            let transcript = vault.load_transcript(session_id)?;
            if transcript.character_id != character.id {
                return Err(PortabilityError::Invalid(format!(
                    "transcript {session_id} belongs to another character"
                )));
            }
        }
    }
    if vault.state_path()?.exists() {
        let _: OperationalState = vault.load_state()?;
    }
    let state_db = vault.root().join(".tz-chatter").join("state.sqlite3");
    if state_db.exists() {
        validate_sqlite(&state_db)?;
    }
    let mut index = MemoryStore::open(vault, &character.id)
        .map_err(|error| PortabilityError::Storage(error.to_string()))?;
    index
        .rebuild()
        .map_err(|error| PortabilityError::Storage(error.to_string()))?;
    Ok(())
}

fn ensure_destination_safe(
    destination: &Path,
    manifest: &PackManifest,
    replace_existing: bool,
) -> Result<(), PortabilityError> {
    if !destination.exists() {
        return Ok(());
    }
    if !destination.is_dir() {
        return Err(PortabilityError::Conflict(
            "restore destination is not a directory".into(),
        ));
    }
    let character_path = destination.join("character.md");
    if !character_path.exists() {
        return Err(PortabilityError::Conflict(
            "refusing to overwrite an unrelated directory".into(),
        ));
    }
    let existing = Vault::create(destination)?.load_character()?;
    if existing.id != manifest.character_id {
        return Err(PortabilityError::Conflict(
            "restore destination belongs to another character".into(),
        ));
    }
    if !replace_existing {
        return Err(PortabilityError::Conflict(
            "restore destination already exists; explicit replacement is required".into(),
        ));
    }
    Ok(())
}

fn safe_pack_file(root: &Path, relative: &str) -> Result<PathBuf, PortabilityError> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(PortabilityError::Invalid(format!(
            "pack path escapes its root: {relative}"
        )));
    }
    let candidate = root.join(path);
    if !candidate.is_file() {
        return Err(PortabilityError::Invalid(format!(
            "pack file is missing: {relative}"
        )));
    }
    let canonical_root = fs::canonicalize(root)?;
    let canonical_candidate = fs::canonicalize(&candidate)?;
    if !canonical_candidate.starts_with(&canonical_root) {
        return Err(PortabilityError::Invalid(format!(
            "pack file resolves outside its root: {relative}"
        )));
    }
    Ok(canonical_candidate)
}

fn target_relative(relative: &str) -> PathBuf {
    match relative {
        "state/state.json" => PathBuf::from(".tz-chatter/state.json"),
        "state/state.sqlite3" => PathBuf::from(".tz-chatter/state.sqlite3"),
        _ => PathBuf::from(relative),
    }
}

fn vault_source_path(vault: &Vault, relative: &str) -> Result<PathBuf, PortabilityError> {
    let path = match relative {
        "state/state.json" => vault.state_path()?,
        "state/state.sqlite3" => vault.resolve_relative(".tz-chatter/state.sqlite3")?,
        _ => vault.resolve_relative(relative)?,
    };
    if !path.is_file() {
        return Err(PortabilityError::Storage(format!(
            "canonical file is missing: {relative}"
        )));
    }
    Ok(path)
}

fn validate_sqlite(path: &Path) -> Result<(), PortabilityError> {
    let connection = Connection::open(path)?;
    let result: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if result != "ok" {
        return Err(PortabilityError::Invalid(format!(
            "SQLite integrity check failed: {result}"
        )));
    }
    Ok(())
}

fn memory_type_folders() -> [(MemoryType, &'static str); 5] {
    [
        (MemoryType::People, "people"),
        (MemoryType::Episodic, "episodic"),
        (MemoryType::Semantic, "semantic"),
        (MemoryType::Relationships, "relationships"),
        (MemoryType::OpenThreads, "open-threads"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        memory::MemoryStore,
        storage::{MemoryReviewStatus, TranscriptDocument, TranscriptTurn, TurnRole, TurnStatus},
    };

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "tz-chatter-portability-{label}-{}",
            crate::storage::new_stable_id()
        ))
    }

    fn seed_vault(root: &Path, character_id: &str) -> Vault {
        let vault = Vault::create(root).unwrap();
        let character = CharacterDefinition::new(character_id, character_id, "Stay grounded.");
        vault.save_character(&character).unwrap();
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
        let mut memory = MemoryRecord::new("topic", MemoryType::OpenThreads, "Plan the garden.");
        memory.created_at = "1".into();
        memory.updated_at = "2".into();
        memory.review_status = MemoryReviewStatus::Accepted;
        memory.topics = vec!["garden".into()];
        let mut store = MemoryStore::open(&vault, character_id).unwrap();
        store.create(&memory).unwrap();
        let transcript = TranscriptDocument {
            schema_version: crate::storage::STORAGE_SCHEMA_VERSION,
            session_id: "session".into(),
            character_id: character_id.into(),
            created_at: "1".into(),
            updated_at: "2".into(),
            local: None,
            title: String::new(),
            archived: false,
            turns: vec![TranscriptTurn {
                id: "user".into(),
                timestamp: "1".into(),
                role: TurnRole::User,
                status: TurnStatus::Complete,
                content: "Hello".into(),
            }],
        };
        vault.save_transcript(&transcript).unwrap();
        vault
            .save_state(&OperationalState {
                last_opened_session_id: Some("session".into()),
                ..OperationalState::default()
            })
            .unwrap();
        drop(store);
        vault
    }

    fn tiny_png() -> Vec<u8> {
        vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ]
    }

    #[test]
    fn export_import_includes_portrait_png() {
        let source_root = root("portrait-source");
        let pack_root = root("portrait-pack");
        let target_root = root("portrait-target");
        let source = seed_vault(&source_root, "lyra");
        crate::characters::set_portrait(source.root(), &tiny_png()).unwrap();
        let manifest = export_pack(&source, &pack_root).unwrap();
        assert!(manifest
            .files
            .iter()
            .any(|file| file == "assets/portrait.png"));
        let _ = import_pack(&pack_root, &target_root, false).unwrap();
        assert_eq!(
            crate::characters::load_portrait(&target_root)
                .unwrap()
                .as_deref(),
            Some(tiny_png().as_slice())
        );
        fs::remove_dir_all(source_root).unwrap();
        fs::remove_dir_all(pack_root).unwrap();
        fs::remove_dir_all(target_root).unwrap();
    }

    #[test]
    fn export_omits_character_generation_settings() {
        let source_root = root("generation-source");
        let pack_root = root("generation-pack");
        let target_root = root("generation-target");
        let source = seed_vault(&source_root, "lyra");
        crate::generation::save(
            &source,
            &crate::generation::CharacterGeneration {
                schema_version: 1,
                chat_model: Some("lyra-voice".into()),
                temperature: Some(0.7),
                max_tokens: Some(256),
            },
        )
        .unwrap();
        let manifest = export_pack(&source, &pack_root).unwrap();
        assert!(!manifest
            .files
            .iter()
            .any(|file| file.contains("generation")));
        let _ = import_pack(&pack_root, &target_root, false).unwrap();
        let target = Vault::open(&target_root).unwrap();
        let imported = crate::generation::load(&target).unwrap();
        assert_eq!(imported.chat_model, None);
        assert_eq!(imported.temperature, None);
        fs::remove_dir_all(source_root).unwrap();
        fs::remove_dir_all(pack_root).unwrap();
        fs::remove_dir_all(target_root).unwrap();
    }

    #[test]
    fn export_import_round_trip_preserves_canonical_and_durable_state() {
        let source_root = root("source");
        let pack_root = root("pack");
        let target_root = root("target");
        let source = seed_vault(&source_root, "lyra");
        let manifest = export_pack(&source, &pack_root).unwrap();
        assert_eq!(manifest.character_id, "lyra");
        assert!(!manifest
            .files
            .iter()
            .any(|file| file.contains("memory-index")));
        let result = import_pack(&pack_root, &target_root, false).unwrap();
        assert_eq!(result.files_restored, manifest.files.len());
        let target = Vault::create(&target_root).unwrap();
        assert_eq!(target.load_character().unwrap().id, "lyra");
        assert!(manifest.files.iter().any(|file| file == "persona.md"));
        assert!(manifest.files.iter().any(|file| file == "scene.md"));
        assert!(manifest.files.iter().any(|file| file == "locals/cafe.md"));
        assert_eq!(
            crate::scene::load_persona(&target).unwrap().body.trim(),
            "I am Alex."
        );
        assert_eq!(
            crate::scene::load_scene_settings(&target)
                .unwrap()
                .default_local
                .as_deref(),
            Some("cafe")
        );
        assert!(crate::scene::load_local(&target, "cafe")
            .unwrap()
            .body
            .contains("Rain on the windows."));
        assert_eq!(
            target
                .load_memory(&MemoryType::OpenThreads, "topic")
                .unwrap()
                .body,
            "Plan the garden."
        );
        assert_eq!(target.load_transcript("session").unwrap().turns.len(), 1);
        assert_eq!(
            target
                .load_state()
                .unwrap()
                .last_opened_session_id
                .as_deref(),
            Some("session")
        );
        let mut index = MemoryStore::open(&target, "lyra").unwrap();
        assert_eq!(index.search("garden", 5).unwrap().len(), 1);
        drop(index);
        fs::remove_dir_all(source_root).unwrap();
        fs::remove_dir_all(pack_root).unwrap();
        fs::remove_dir_all(target_root).unwrap();
    }

    #[test]
    fn restore_rebuilds_a_corrupt_source_index_and_refuses_unrelated_targets() {
        let source_root = root("corrupt-source");
        let pack_root = root("corrupt-pack");
        let unrelated_root = root("unrelated");
        let source = seed_vault(&source_root, "lyra");
        drop(source);
        fs::write(
            source_root.join(".tz-chatter/memory-index.sqlite3"),
            b"corrupt",
        )
        .unwrap();
        let source = Vault::create(&source_root).unwrap();
        let _ = export_pack(&source, &pack_root).unwrap();
        let unrelated = seed_vault(&unrelated_root, "nova");
        let error = import_pack(&pack_root, &unrelated_root, false).unwrap_err();
        assert!(
            matches!(error, PortabilityError::Conflict(message) if message.contains("another character"))
        );
        drop(unrelated);
        fs::remove_dir_all(source_root).unwrap();
        fs::remove_dir_all(pack_root).unwrap();
        fs::remove_dir_all(unrelated_root).unwrap();
    }

    #[test]
    fn replacing_the_same_character_keeps_a_backup() {
        let source_root = root("replace-source");
        let pack_root = root("replace-pack");
        let target_root = root("replace-target");
        let source = seed_vault(&source_root, "lyra");
        let _ = export_pack(&source, &pack_root).unwrap();
        let target = seed_vault(&target_root, "lyra");
        let mut store = MemoryStore::open(&target, "lyra").unwrap();
        let mut old = target
            .load_memory(&MemoryType::OpenThreads, "topic")
            .unwrap();
        old.body = "Old local copy".into();
        old.updated_at = "3".into();
        store.update(&old).unwrap();
        drop(store);

        let result = import_pack(&pack_root, &target_root, true).unwrap();
        let backup = result
            .backup_path
            .expect("replacement should preserve a backup");
        assert!(Path::new(&backup).is_dir());
        let restored = Vault::create(&target_root).unwrap();
        assert_eq!(
            restored
                .load_memory(&MemoryType::OpenThreads, "topic")
                .unwrap()
                .body,
            "Plan the garden."
        );
        let preserved = Vault::create(&backup).unwrap();
        assert_eq!(
            preserved
                .load_memory(&MemoryType::OpenThreads, "topic")
                .unwrap()
                .body,
            "Old local copy"
        );
        fs::remove_dir_all(source_root).unwrap();
        fs::remove_dir_all(pack_root).unwrap();
        fs::remove_dir_all(target_root).unwrap();
        fs::remove_dir_all(backup).unwrap();
    }

    #[test]
    fn traversal_and_unknown_schema_are_rejected_before_writes() {
        let pack_root = root("malicious-pack");
        let target_root = root("malicious-target");
        fs::create_dir_all(&pack_root).unwrap();
        let manifest = PackManifest {
            schema_version: PACK_SCHEMA_VERSION,
            format: PACK_FORMAT.into(),
            character_id: "lyra".into(),
            files: vec!["character.md".into(), "../outside.md".into()],
        };
        fs::write(
            pack_root.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        fs::write(pack_root.join("character.md"), b"not used").unwrap();
        let error = import_pack(&pack_root, &target_root, false).unwrap_err();
        assert!(
            matches!(error, PortabilityError::Invalid(message) if message.contains("not permitted"))
        );
        let old_manifest = PackManifest {
            schema_version: 99,
            ..manifest
        };
        fs::write(
            pack_root.join("manifest.json"),
            serde_json::to_vec(&old_manifest).unwrap(),
        )
        .unwrap();
        let error = import_pack(&pack_root, &target_root, false).unwrap_err();
        assert!(
            matches!(error, PortabilityError::Invalid(message) if message.contains("schema version"))
        );
        assert!(!target_root.exists());
        fs::remove_dir_all(pack_root).unwrap();
    }

    #[test]
    fn legacy_manifest_is_migrated_before_validation() {
        let legacy = serde_json::json!({
            "schema_version": 0,
            "character_id": "lyra",
            "files": ["character.md"]
        });
        let migrated = migrate_manifest(legacy).unwrap();
        assert_eq!(migrated.schema_version, PACK_SCHEMA_VERSION);
        assert_eq!(migrated.format, PACK_FORMAT);
        assert_eq!(migrated.files, vec!["character.md"]);
    }
}
